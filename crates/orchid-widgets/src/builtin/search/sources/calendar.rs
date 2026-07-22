//! Search calendar events across live calendar widgets.

use async_trait::async_trait;

use super::{ActionTarget, SearchCandidate, SearchSource};
use crate::builtin::calendar;

/// Source id.
pub const SOURCE_ID: &str = "calendar";

/// Calendar events search source.
#[derive(Debug, Default)]
pub struct CalendarSource;

impl CalendarSource {
    /// Convenience constructor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SearchSource for CalendarSource {
    fn id(&self) -> &'static str {
        SOURCE_ID
    }

    fn name_key(&self) -> &'static str {
        "search-source-calendar"
    }

    fn icon(&self) -> &'static str {
        "calendar"
    }

    async fn search(&self, query: &str, limit: usize) -> Vec<SearchCandidate> {
        calendar::search_all_events(query, limit)
            .into_iter()
            .map(|h| {
                let title = if h.title.is_empty() {
                    // Resolved in UI via untitled key when empty? Keep a marker.
                    "…".to_string()
                } else {
                    h.title
                };
                SearchCandidate {
                    id: format!("calendar:{}:{}", h.instance_id, h.event_id),
                    source_id: SOURCE_ID,
                    title,
                    subtitle: Some(h.subtitle),
                    icon: "calendar",
                    score: h.score,
                    action_hint: None,
                    action_target: ActionTarget::OpenCalendarEvent {
                        instance_id: h.instance_id.to_string(),
                        event_id: h.event_id,
                        date: h.date,
                    },
                }
            })
            .collect()
    }
}
