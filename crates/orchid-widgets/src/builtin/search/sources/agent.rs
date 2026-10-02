//! Agent source — queries that start with `?` become one question.
//!
//! The source does not call the model. Activating the row runs the request
//! on the background job queue.

use async_trait::async_trait;

use super::{ActionTarget, SearchCandidate, SearchSource};

/// Source id.
pub const SOURCE_ID: &str = "agent";

/// Offers `?question` as a single agent candidate.
#[derive(Debug, Default)]
pub struct AgentSource;

impl AgentSource {
    /// Convenience constructor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SearchSource for AgentSource {
    fn id(&self) -> &'static str {
        SOURCE_ID
    }
    fn name_key(&self) -> &'static str {
        "search-source-agent"
    }
    fn icon(&self) -> &'static str {
        "agent"
    }
    async fn search(&self, query: &str, limit: usize) -> Vec<SearchCandidate> {
        if limit == 0 {
            return Vec::new();
        }
        let trimmed = query.trim();
        let Some(rest) = trimmed.strip_prefix('?') else {
            return Vec::new();
        };
        let prompt = rest.trim();
        if prompt.is_empty() {
            return Vec::new();
        }
        let title: String = prompt.chars().take(80).collect();
        let title = if prompt.chars().count() > 80 {
            format!("{title}…")
        } else {
            title
        };
        vec![SearchCandidate {
            id: format!("agent:{prompt}"),
            source_id: SOURCE_ID,
            title,
            subtitle: None,
            icon: "agent",
            score: 220,
            action_hint: None,
            action_target: ActionTarget::AskAgent(prompt.to_string()),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn question_mark_is_one_agent_row() {
        let rows = AgentSource::new().search("?  where is the dog", 5).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "where is the dog");
        match &rows[0].action_target {
            ActionTarget::AskAgent(prompt) => assert_eq!(prompt, "where is the dog"),
            other => panic!("unexpected target {other:?}"),
        }
        assert!(AgentSource::new()
            .search("where is the dog", 5)
            .await
            .is_empty());
        assert!(AgentSource::new().search("?", 5).await.is_empty());
    }
}
