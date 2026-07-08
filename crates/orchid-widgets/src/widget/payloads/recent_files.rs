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
    /// Build a payload from store entries with localized relative timestamps.
    #[must_use]
    pub fn from_entries(
        entries: &[crate::recent_files::RecentFileEntry],
        locale: &orchid_i18n::LocaleManager,
    ) -> Self {
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
                        opened_text: format_opened_relative(locale, now, e.opened_at),
                    }
                })
                .collect(),
        }
    }
}

fn format_opened_relative(
    locale: &orchid_i18n::LocaleManager,
    now: SystemTime,
    at: SystemTime,
) -> String {
    let Ok(secs) = now.duration_since(at).map(|d| d.as_secs()) else {
        return String::new();
    };
    if secs < 60 {
        locale.tr("relative-just-now")
    } else if secs < 3600 {
        locale.tr_args(
            "relative-minutes",
            &orchid_i18n::FluentArgs::new().with("m", (secs / 60).to_string()),
        )
    } else if secs < 86_400 {
        locale.tr_args(
            "relative-hours",
            &orchid_i18n::FluentArgs::new().with("h", (secs / 3600).to_string()),
        )
    } else {
        locale.tr_args(
            "relative-days",
            &orchid_i18n::FluentArgs::new().with("d", (secs / 86_400).to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_i18n::{default_language, LocaleManager};
    use std::time::{Duration, SystemTime};

    fn test_locale() -> LocaleManager {
        LocaleManager::new(default_language(), None).expect("locale")
    }

    #[test]
    fn from_entries_uses_localized_relative_time() {
        let locale = test_locale();
        let now = SystemTime::now();
        let entries = vec![crate::recent_files::RecentFileEntry {
            path: r"C:\docs\readme.txt".into(),
            opened_at: now - Duration::from_secs(90),
        }];
        let payload = RecentFilesPayload::from_entries(&entries, &locale);
        assert_eq!(payload.items.len(), 1);
        assert_eq!(
            payload.items[0].opened_text,
            locale.tr_args(
                "relative-minutes",
                &orchid_i18n::FluentArgs::new().with("m", "1"),
            )
        );
    }
}
