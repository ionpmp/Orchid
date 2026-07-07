//! Recent-files widget payload.

use std::time::SystemTime;

use orchid_fs::FsPath;

/// One row in the recent-files widget.
#[derive(Debug, Clone)]
pub struct RecentFileItemView {
    /// Stable row id (path).
    pub id: String,
    /// File name for display.
    pub name: String,
    /// Full path subtitle.
    pub path: String,
    /// Human-readable opened-at label.
    pub opened_text: String,
}

/// Snapshot payload for the recent-files widget.
#[derive(Debug, Clone, Default)]
pub struct RecentFilesPayload {
    /// Recent entries in display order.
    pub items: Vec<RecentFileItemView>,
}

impl RecentFilesPayload {
    /// Build a payload from store entries.
    #[must_use]
    pub fn from_entries(entries: &[crate::recent_files::RecentFileEntry]) -> Self {
        let now = SystemTime::now();
        Self {
            items: entries
                .iter()
                .map(|e| {
                    let name = FsPath::new(&e.path)
                        .ok()
                        .and_then(|p| p.file_name().map(String::from))
                        .unwrap_or_else(|| e.path.clone());
                    RecentFileItemView {
                        id: e.path.clone(),
                        name,
                        path: e.path.clone(),
                        opened_text: format_opened_relative(now, e.opened_at),
                    }
                })
                .collect(),
        }
    }
}

fn format_opened_relative(now: SystemTime, at: SystemTime) -> String {
    let Ok(secs) = now.duration_since(at).map(|d| d.as_secs()) else {
        return String::new();
    };
    if secs < 60 {
        "just now".into()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}
