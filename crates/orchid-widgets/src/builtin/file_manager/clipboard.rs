//! File-manager clipboard: tracks paths staged for copy / cut paste.

use parking_lot::RwLock;

/// What operation the clipboard holds.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardOperation {
    None,
    Copy,
    Cut,
}

/// Shared file clipboard. Cheap to clone when wrapped in `Arc`.
pub struct FileClipboard {
    entries: RwLock<Vec<orchid_fs::FsPath>>,
    operation: RwLock<ClipboardOperation>,
}

impl Default for FileClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for FileClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileClipboard")
            .field("operation", &*self.operation.read())
            .field("entries", &self.entries.read().len())
            .finish()
    }
}

impl FileClipboard {
    /// Empty clipboard.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            operation: RwLock::new(ClipboardOperation::None),
        }
    }

    /// Stage a copy of `paths`.
    pub fn copy(&self, paths: Vec<orchid_fs::FsPath>) {
        *self.entries.write() = paths;
        *self.operation.write() = ClipboardOperation::Copy;
    }

    /// Stage a cut of `paths`.
    pub fn cut(&self, paths: Vec<orchid_fs::FsPath>) {
        *self.entries.write() = paths;
        *self.operation.write() = ClipboardOperation::Cut;
    }

    /// Paste: returns the staged paths + operation kind. Cut auto-clears
    /// on paste; Copy leaves the clipboard intact for repeat pastes.
    pub fn paste(&self, _to: &orchid_fs::FsPath) -> (Vec<orchid_fs::FsPath>, ClipboardOperation) {
        let paths = self.entries.read().clone();
        let op = *self.operation.read();
        if op == ClipboardOperation::Cut {
            self.clear();
        }
        (paths, op)
    }

    /// Clear the clipboard.
    pub fn clear(&self) {
        self.entries.write().clear();
        *self.operation.write() = ClipboardOperation::None;
    }

    /// Whether the clipboard holds anything pasteable.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }

    /// Current clipboard operation.
    #[must_use]
    pub fn operation(&self) -> ClipboardOperation {
        *self.operation.read()
    }

    /// Number of staged paths.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> orchid_fs::FsPath {
        orchid_fs::FsPath::new(s).unwrap()
    }

    #[test]
    fn copy_then_paste_preserves_clipboard() {
        let c = FileClipboard::new();
        c.copy(vec![p("local:/a/b")]);
        let (entries, op) = c.paste(&p("local:/dest"));
        assert_eq!(op, ClipboardOperation::Copy);
        assert_eq!(entries.len(), 1);
        assert!(!c.is_empty());
    }

    #[test]
    fn cut_then_paste_clears_clipboard() {
        let c = FileClipboard::new();
        c.cut(vec![p("local:/a/b")]);
        let (entries, op) = c.paste(&p("local:/dest"));
        assert_eq!(op, ClipboardOperation::Cut);
        assert_eq!(entries.len(), 1);
        assert!(c.is_empty());
    }
}
