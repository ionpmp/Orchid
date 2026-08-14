//! File-manager clipboard: tracks paths staged for copy / cut paste.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
    /// Files available on the OS clipboard (Explorer / other apps).
    os_count: AtomicUsize,
    os_is_cut: AtomicBool,
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
            os_count: AtomicUsize::new(0),
            os_is_cut: AtomicBool::new(false),
        }
    }

    /// Staged paths (internal Orchid clipboard).
    #[must_use]
    pub fn paths(&self) -> Vec<orchid_fs::FsPath> {
        self.entries.read().clone()
    }

    /// Remember that the OS clipboard currently holds `count` files.
    pub fn note_os_files(&self, count: usize, is_cut: bool) {
        self.os_count.store(count, Ordering::Relaxed);
        self.os_is_cut.store(is_cut, Ordering::Relaxed);
    }

    /// Internal entries or files offered by another application.
    #[must_use]
    pub fn can_paste(&self) -> bool {
        !self.is_empty() || self.os_count.load(Ordering::Relaxed) > 0
    }

    /// Count / cut flag for the status strip (internal wins over OS).
    #[must_use]
    pub fn display_state(&self) -> (u32, bool) {
        let n = self.len();
        if n > 0 {
            (n as u32, *self.operation.read() == ClipboardOperation::Cut)
        } else {
            (
                self.os_count.load(Ordering::Relaxed) as u32,
                self.os_is_cut.load(Ordering::Relaxed),
            )
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

    #[test]
    fn os_files_enable_paste_when_internal_empty() {
        let c = FileClipboard::new();
        assert!(!c.can_paste());
        c.note_os_files(2, true);
        assert!(c.can_paste());
        assert_eq!(c.display_state(), (2, true));
        c.copy(vec![p("local:/a")]);
        assert_eq!(c.display_state(), (1, false));
    }
}
