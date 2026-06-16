//! Virtual-folder helpers used by the file-manager's sidebar.
//!
//! The MVP delivers a tiny resolver: it recognises the `virtual:` scheme
//! and lists a hardcoded catalog of root entries (Recent, Starred, Tags,
//! Categories) without touching the filesystem. Real data feeds in
//! through the widget's navigation pipeline — this module only enumerates
//! the "directories".

use orchid_fs::{FsEntry, FsEntryKind};

/// Category bucket for `virtual:categories/*` folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum FileCategory {
    Images,
    Documents,
    Video,
    Audio,
    Archives,
}

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

/// Map a `virtual:categories/…` path to a [`FileCategory`].
#[must_use]
pub fn category_for_virtual_path(raw: &str) -> Option<FileCategory> {
    match raw {
        "virtual:categories/images" => Some(FileCategory::Images),
        "virtual:categories/documents" => Some(FileCategory::Documents),
        "virtual:categories/video" => Some(FileCategory::Video),
        "virtual:categories/audio" => Some(FileCategory::Audio),
        "virtual:categories/archives" => Some(FileCategory::Archives),
        _ => None,
    }
}

/// Fluent key for the display label of a virtual path, when known.
#[must_use]
pub fn label_key_for_virtual_path(raw: &str) -> Option<&'static str> {
    match raw {
        "virtual:recent" => Some("fm-virtual-recent"),
        "virtual:starred" => Some("fm-virtual-starred"),
        "virtual:tags" => Some("fm-virtual-tags"),
        "virtual:categories" => Some("fm-sidebar-categories"),
        "virtual:categories/images" => Some("fm-category-images"),
        "virtual:categories/documents" => Some("fm-category-documents"),
        "virtual:categories/video" => Some("fm-category-video"),
        "virtual:categories/audio" => Some("fm-category-audio"),
        "virtual:categories/archives" => Some("fm-category-archives"),
        "virtual:network" => Some("fm-sidebar-network-all"),
        _ => None,
    }
}

/// Sentinel error token shown when a virtual folder has no entries yet.
#[must_use]
pub fn empty_placeholder_for_path(raw: &str) -> Option<&'static str> {
    match raw {
        "virtual:recent" => Some("virtual-empty-recent"),
        "virtual:starred" => Some("virtual-empty-starred"),
        "virtual:tags" => Some("virtual-empty-tags"),
        "virtual:network" => Some("network-placeholder"),
        path if category_for_virtual_path(path).is_some() => Some("virtual-empty-category"),
        _ => None,
    }
}

/// Whether `entry` belongs in the given category virtual folder.
#[must_use]
pub fn entry_matches_category(entry: &FsEntry, cat: FileCategory) -> bool {
    if matches!(entry.metadata.kind, FsEntryKind::Directory) {
        return false;
    }
    let mime = entry.metadata.mime.as_deref();
    let ext = entry.path.extension().map(|e| e.to_lowercase());
    match cat {
        FileCategory::Images => mime_is_prefix(mime, "image/")
            || ext_in(&ext, &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "avif", "svg"]),
        FileCategory::Documents => {
            mime_is_prefix(mime, "text/")
                || mime == Some("application/pdf")
                || mime.map(|m| m.contains("document") || m.contains("msword") || m.contains("spreadsheet"))
                    .unwrap_or(false)
                || ext_in(
                    &ext,
                    &[
                        "pdf", "doc", "docx", "odt", "rtf", "txt", "md", "xls", "xlsx", "ppt", "pptx",
                        "csv", "ods", "odp",
                    ],
                )
        }
        FileCategory::Video => mime_is_prefix(mime, "video/")
            || ext_in(&ext, &["mp4", "mkv", "avi", "mov", "webm", "wmv", "m4v", "flv"]),
        FileCategory::Audio => mime_is_prefix(mime, "audio/")
            || ext_in(&ext, &["mp3", "wav", "flac", "aac", "ogg", "m4a", "wma", "opus"]),
        FileCategory::Archives => mime.map(|m| m.contains("zip") || m.contains("archive") || m.contains("compressed"))
            .unwrap_or(false)
            || ext_in(&ext, &["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "cab"]),
    }
}

fn mime_is_prefix(mime: Option<&str>, prefix: &str) -> bool {
    mime.map(|m| m.starts_with(prefix)).unwrap_or(false)
}

/// Representative extensions used to query the search index for a category.
#[must_use]
pub fn category_search_extensions(cat: FileCategory) -> &'static [&'static str] {
    match cat {
        FileCategory::Images => &[
            "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "avif", "svg",
        ],
        FileCategory::Documents => &[
            "pdf", "doc", "docx", "odt", "rtf", "txt", "md", "xls", "xlsx", "ppt", "pptx", "csv",
            "ods", "odp",
        ],
        FileCategory::Video => &["mp4", "mkv", "avi", "mov", "webm", "wmv", "m4v", "flv"],
        FileCategory::Audio => &["mp3", "wav", "flac", "aac", "ogg", "m4a", "wma", "opus"],
        FileCategory::Archives => &["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "cab"],
    }
}

fn ext_in(ext: &Option<String>, list: &[&str]) -> bool {
    ext.as_ref()
        .map(|e| list.iter().any(|x| x == e))
        .unwrap_or(false)
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

    #[test]
    fn label_key_maps_virtual_paths() {
        assert_eq!(
            label_key_for_virtual_path("virtual:recent"),
            Some("fm-virtual-recent")
        );
        assert_eq!(
            label_key_for_virtual_path("virtual:categories/images"),
            Some("fm-category-images")
        );
        assert!(label_key_for_virtual_path("local:/tmp").is_none());
    }

    #[test]
    fn category_filter_matches_image_extension() {
        let entry = FsEntry {
            path: orchid_fs::FsPath::new("local:/tmp/photo.png").unwrap(),
            name: "photo.png".into(),
            metadata: orchid_fs::FsMetadata {
                kind: FsEntryKind::File,
                size: 0,
                created: None,
                modified: None,
                accessed: None,
                readonly: false,
                hidden: false,
                system: false,
                mime: None,
                extended: orchid_fs::ExtendedAttributes::default(),
            },
        };
        assert!(entry_matches_category(&entry, FileCategory::Images));
        assert!(!entry_matches_category(&entry, FileCategory::Audio));
    }
}
