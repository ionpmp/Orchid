//! Virtual-folder helpers used by the file-manager's sidebar.
//!
//! The MVP delivers a tiny resolver: it recognises the `virtual:` scheme
//! and lists a hardcoded catalog of root entries (Recent, Starred, Tags,
//! Categories) without touching the filesystem. Real data feeds in
//! through the widget's navigation pipeline — this module only enumerates
//! the "directories".

/// One top-level virtual folder.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct VirtualFolder {
    /// Full `virtual:` path.
    pub path: orchid_fs::FsPath,
    /// Translation key for the display name.
    pub label_key: &'static str,
    /// Icon name.
    pub icon: &'static str,
}

/// Top-level virtual folders shown in the sidebar.
#[must_use]
pub fn sidebar_catalog() -> Vec<VirtualFolder> {
    [
        ("virtual:recent", "fm-virtual-recent", "sidebar-recent"),
        ("virtual:starred", "fm-virtual-starred", "sidebar-starred"),
        ("virtual:tags", "fm-virtual-tags", "sidebar-tags"),
        (
            "virtual:categories/images",
            "fm-category-images",
            "sidebar-images",
        ),
        (
            "virtual:categories/documents",
            "fm-category-documents",
            "sidebar-documents",
        ),
        (
            "virtual:categories/video",
            "fm-category-video",
            "sidebar-video",
        ),
        (
            "virtual:categories/audio",
            "fm-category-audio",
            "sidebar-audio",
        ),
        (
            "virtual:categories/archives",
            "fm-category-archives",
            "sidebar-archives",
        ),
    ]
    .into_iter()
    .filter_map(|(raw, label_key, icon)| {
        let path = orchid_fs::FsPath::new(raw).ok()?;
        Some(VirtualFolder {
            path,
            label_key,
            icon,
        })
    })
    .collect()
}

/// `true` when `path` points to a virtual folder.
#[must_use]
pub fn is_virtual(path: &orchid_fs::FsPath) -> bool {
    path.scheme() == "virtual"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_has_expected_entries() {
        let cat = sidebar_catalog();
        assert!(cat.iter().any(|v| v.path.as_str() == "virtual:recent"));
        assert!(cat.iter().any(|v| v.label_key == "fm-virtual-starred"));
    }

    #[test]
    fn is_virtual_detects_scheme() {
        let v = orchid_fs::FsPath::new("virtual:recent").unwrap();
        let l = orchid_fs::FsPath::new("local:/tmp").unwrap();
        assert!(is_virtual(&v));
        assert!(!is_virtual(&l));
    }
}
