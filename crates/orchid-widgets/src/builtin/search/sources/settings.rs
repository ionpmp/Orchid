//! Static settings-section source. Matches the query against a hardcoded
//! catalog of top-level settings sections. The settings UI itself lives
//! downstream; activating a candidate only publishes a request event.

use async_trait::async_trait;

use super::{ActionTarget, SearchCandidate, SearchSource};

/// Source id.
pub const SOURCE_ID: &str = "settings";

/// One entry in the settings catalog.
#[derive(Debug, Clone, Copy)]
struct Section {
    id: &'static str,
    english_label: &'static str,
    icon: &'static str,
}

const SECTIONS: &[Section] = &[
    Section {
        id: "general",
        english_label: "General",
        icon: "settings-general",
    },
    Section {
        id: "appearance",
        english_label: "Appearance",
        icon: "settings-appearance",
    },
    Section {
        id: "input",
        english_label: "Input",
        icon: "settings-input",
    },
    Section {
        id: "shortcuts",
        english_label: "Shortcuts",
        icon: "settings-shortcuts",
    },
    Section {
        id: "locale",
        english_label: "Locale",
        icon: "settings-locale",
    },
    Section {
        id: "privacy",
        english_label: "Privacy",
        icon: "settings-privacy",
    },
];

/// Settings source.
#[derive(Debug, Default)]
pub struct SettingsSource;

impl SettingsSource {
    /// Convenience constructor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SearchSource for SettingsSource {
    fn id(&self) -> &'static str {
        SOURCE_ID
    }
    fn name_key(&self) -> &'static str {
        "search-source-settings"
    }
    fn icon(&self) -> &'static str {
        "settings"
    }
    async fn search(&self, query: &str, limit: usize) -> Vec<SearchCandidate> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for sec in SECTIONS {
            let label_l = sec.english_label.to_lowercase();
            let id_l = sec.id;
            let score = if label_l == q || id_l == q {
                100
            } else if label_l.starts_with(&q) || id_l.starts_with(&q) {
                80
            } else if label_l.contains(&q) || id_l.contains(&q) {
                60
            } else {
                continue;
            };
            hits.push(SearchCandidate {
                id: format!("settings:{}", sec.id),
                source_id: SOURCE_ID,
                title: sec.english_label.to_string(),
                subtitle: None,
                icon: sec.icon,
                score,
                action_hint: None,
                action_target: ActionTarget::OpenSettings(sec.id.to_string()),
            });
        }
        hits.sort_by(|a, b| b.score.cmp(&a.score));
        hits.truncate(limit);
        hits
    }
}
