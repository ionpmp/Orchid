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

    /// Flatten nested files and folders under `path` (branch view).
    pub async fn list_branch(
        &self,
        path: &orchid_fs::FsPath,
        show_hidden: bool,
    ) -> NavigationResult {
        const MAX_ENTRIES: usize = 8000;
        const MAX_DEPTH: usize = 6;
        let mut entries = Vec::new();
        let mut dirs = std::collections::VecDeque::from([(path.clone(), 0usize)]);
        while let Some((dir, depth)) = dirs.pop_front() {
            if entries.len() >= MAX_ENTRIES {
                break;
            }
            let listed = self.navigate(&dir, show_hidden).await;
            if listed.error.is_some() && depth == 0 {
                return listed;
            }
            for e in listed.entries {
                let is_dir = matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory);
                if is_dir && depth + 1 < MAX_DEPTH {
                    dirs.push_back((e.path.clone(), depth + 1));
                }
                let mut e = e;
                e.name = relative_display_name(path, &e.path);
                entries.push(e);
                if entries.len() >= MAX_ENTRIES {
                    break;
                }
            }
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

/// One local volume / mount shown in the drive switcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveItem {
    /// Canonical [`orchid_fs::FsPath`] string (`local:c:/`).
    pub path: String,
    /// Short label (`C:` or `C:  Windows`).
    pub label: String,
}

/// Drive or volume root of `path` (`local:c:/foo` → `local:c:/`).
#[must_use]
pub fn drive_root(path: &orchid_fs::FsPath) -> Option<orchid_fs::FsPath> {
    let body = path.without_scheme();
    let bytes = body.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        let letter = (bytes[0] as char).to_ascii_lowercase();
        return orchid_fs::FsPath::new(format!("{}:{letter}:/", path.scheme())).ok();
    }
    if body.starts_with('/') {
        return orchid_fs::FsPath::new(format!("{}:/", path.scheme())).ok();
    }
    None
}

/// Mounted local volumes for the drive switcher.
#[must_use]
pub fn list_local_drives() -> Vec<DriveItem> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut out: Vec<DriveItem> = Vec::new();
    for disk in disks.list() {
        let mp = disk.mount_point();
        let Ok(fp) = orchid_fs::FsPath::from_local(mp) else {
            continue;
        };
        let raw = mp.to_string_lossy();
        let trimmed = raw.trim_end_matches(['\\', '/']);
        let letter = if trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':' {
            format!(
                "{}:",
                trimmed.chars().next().unwrap_or('?').to_ascii_uppercase()
            )
        } else if trimmed.is_empty() {
            fp.as_str().to_string()
        } else {
            trimmed.to_string()
        };
        let extra = disk.name().to_string_lossy();
        let label = if extra.is_empty() {
            letter
        } else {
            format!("{letter}  {extra}")
        };
        if out.iter().any(|d| d.path == fp.as_str()) {
            continue;
        }
        out.push(DriveItem {
            path: fp.as_str().to_string(),
            label,
        });
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

fn relative_display_name(root: &orchid_fs::FsPath, entry: &orchid_fs::FsPath) -> String {
    let root_s = root.as_str().trim_end_matches('/');
    let entry_s = entry.as_str();
    entry_s
        .strip_prefix(root_s)
        .map(|rest| rest.trim_start_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            entry
                .file_name()
                .unwrap_or_else(|| entry.as_str())
                .to_string()
        })
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

    #[test]
    fn drive_root_windows_and_unix() {
        let win = orchid_fs::FsPath::new("local:c:/Users/docs").unwrap();
        assert_eq!(drive_root(&win).unwrap().as_str(), "local:c:/");
        let unix = orchid_fs::FsPath::new("local:/home/a").unwrap();
        assert_eq!(drive_root(&unix).unwrap().as_str(), "local:/");
    }

    #[test]
    fn relative_display_strips_root() {
        let root = orchid_fs::FsPath::new("local:c:/proj").unwrap();
        let nested = orchid_fs::FsPath::new("local:c:/proj/src/main.rs").unwrap();
        assert_eq!(relative_display_name(&root, &nested), "src/main.rs");
    }

    #[test]
    fn list_local_drives_does_not_panic() {
        let _ = list_local_drives();
    }
}
