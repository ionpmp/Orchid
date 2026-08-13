//! Navigation helper: lists a directory via the provider registry and
//! builds breadcrumb segments.

use std::sync::Arc;

/// One step in a breadcrumb trail.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct BreadcrumbSegment {
    pub path: orchid_fs::FsPath,
    pub display_name: String,
}

/// Result of a navigation request.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct NavigationResult {
    pub entries: Vec<orchid_fs::FsEntry>,
    pub breadcrumbs: Vec<BreadcrumbSegment>,
    pub parent: Option<orchid_fs::FsPath>,
    pub total_entries: usize,
    pub error: Option<String>,
}

/// Listing helper.
pub struct Navigator {
    registry: Arc<orchid_fs::FsProviderRegistry>,
}

impl std::fmt::Debug for Navigator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Navigator").finish_non_exhaustive()
    }
}

impl Navigator {
    /// Build a navigator over the given provider registry.
    #[must_use]
    pub fn new(registry: Arc<orchid_fs::FsProviderRegistry>) -> Self {
        Self { registry }
    }

    /// List `path`. On failure the result carries the error in `error`
    /// and returns an empty entry list.
    pub async fn navigate(&self, path: &orchid_fs::FsPath, show_hidden: bool) -> NavigationResult {
        let Some(provider) = self.registry.for_path(path) else {
            return NavigationResult {
                entries: Vec::new(),
                breadcrumbs: self.breadcrumbs_for(path),
                parent: path.parent(),
                total_entries: 0,
                error: Some(format!("no provider for scheme `{}`", path.scheme())),
            };
        };
        let mut entries = match provider.list(path).await {
            Ok(entries) => entries,
            Err(e) => {
                return NavigationResult {
                    entries: Vec::new(),
                    breadcrumbs: self.breadcrumbs_for(path),
                    parent: path.parent(),
                    total_entries: 0,
                    error: Some(e.to_string()),
                };
            }
        };
        if !show_hidden {
            entries.retain(|e| !e.metadata.hidden);
        }
        let total = entries.len();
        NavigationResult {
            entries,
            breadcrumbs: self.breadcrumbs_for(path),
            parent: path.parent(),
            total_entries: total,
            error: None,
        }
    }

    /// Compute breadcrumb segments for `path` by walking its parents.
    #[must_use]
    pub fn breadcrumbs_for(&self, path: &orchid_fs::FsPath) -> Vec<BreadcrumbSegment> {
        let mut trail = Vec::new();
        let mut cursor = Some(path.clone());
        while let Some(p) = cursor {
            let display = p
                .file_name()
                .map(String::from)
                .unwrap_or_else(|| p.scheme().to_string());
            trail.push(BreadcrumbSegment {
                path: p.clone(),
                display_name: display,
            });
            cursor = p.parent();
        }
        trail.reverse();
        trail
    }
}

/// One address-bar autocomplete row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathCompleteItem {
    /// Canonical [`orchid_fs::FsPath`] string.
    pub path: String,
    /// Last path segment for the dropdown label.
    pub label: String,
}

/// Parse a typed address (canonical `scheme:…` or a native Windows/Unix path).
#[must_use]
pub fn coerce_typed_path(raw: &str) -> Option<orchid_fs::FsPath> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(p) = orchid_fs::FsPath::new(t) {
        return Some(p);
    }
    let n = t.replace('\\', "/");
    let bytes = n.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let rest = &n[1..];
        let body = if n.len() == 2 {
            format!("{drive}{rest}/")
        } else {
            format!("{drive}{rest}")
        };
        return orchid_fs::FsPath::new(format!("local:{body}")).ok();
    }
    if n.starts_with('/') {
        return orchid_fs::FsPath::new(format!("local:{n}")).ok();
    }
    None
}

/// Split typed input into the directory to list and the unmatched prefix.
#[must_use]
pub fn complete_parent_and_prefix(typed: &str) -> Option<(orchid_fs::FsPath, String)> {
    let n = typed.trim().replace('\\', "/");
    if n.is_empty() {
        return None;
    }
    if n.ends_with('/') {
        return Some((coerce_typed_path(&n)?, String::new()));
    }
    let slash = n.rfind('/')?;
    let dir = &n[..=slash];
    let prefix = n[slash + 1..].to_string();
    Some((coerce_typed_path(dir)?, prefix))
}

#[cfg(test)]
mod complete_tests {
    use super::*;

    #[test]
    fn coerce_accepts_canonical_and_windows_paths() {
        let a = coerce_typed_path("local:c:/Users").unwrap();
        let b = coerce_typed_path(r"C:\Users").unwrap();
        assert_eq!(a.as_str(), b.as_str());
    }

    #[test]
    fn complete_parts_split_prefix() {
        let (dir, prefix) = complete_parent_and_prefix("local:c:/Users/Doc").unwrap();
        assert_eq!(dir.as_str(), "local:c:/Users");
        assert_eq!(prefix, "Doc");
        let (root, empty) = complete_parent_and_prefix("local:c:/").unwrap();
        assert_eq!(root.as_str(), "local:c:/");
        assert!(empty.is_empty());
    }
}
