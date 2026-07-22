//! Search Jyotish (Vedic panchanga) keywords across live Jyotish widgets.

use async_trait::async_trait;

use super::{ActionTarget, SearchCandidate, SearchSource};
use crate::builtin::jyotish;

/// Source id.
pub const SOURCE_ID: &str = "jyotish";

/// Jyotish keyword search source.
#[derive(Debug, Default)]
pub struct JyotishSource;

impl JyotishSource {
    /// Convenience constructor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SearchSource for JyotishSource {
    fn id(&self) -> &'static str {
        SOURCE_ID
    }

    fn name_key(&self) -> &'static str {
        "search-source-jyotish"
    }

    fn icon(&self) -> &'static str {
        "jyotish"
    }

    async fn search(&self, query: &str, limit: usize) -> Vec<SearchCandidate> {
        jyotish::search_catalog(query, limit)
            .into_iter()
            .map(|h| SearchCandidate {
                id: format!("jyotish:{}:{}", h.instance_id, h.day_offset),
                source_id: SOURCE_ID,
                title: h.title,
                subtitle: Some(h.subtitle),
                icon: "jyotish",
                score: h.score,
                action_hint: None,
                action_target: ActionTarget::OpenJyotishDay {
                    instance_id: h.instance_id.to_string(),
                    day_offset: h.day_offset,
                },
            })
            .collect()
    }
}
