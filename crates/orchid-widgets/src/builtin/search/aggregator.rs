//! Aggregator combining the output of multiple [`SearchSource`]s.

use std::sync::Arc;

use futures::future::join_all;

use super::sources::{SearchCandidate, SearchSource};

/// Runs every source in parallel and merges + re-ranks the results.
pub struct SearchAggregator {
    sources: Vec<Arc<dyn SearchSource>>,
}

impl std::fmt::Debug for SearchAggregator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchAggregator")
            .field("sources", &self.sources.len())
            .finish()
    }
}

impl SearchAggregator {
    /// Build with an ordered list of sources. The order is preserved for
    /// tie-breaking.
    #[must_use]
    pub fn new(sources: Vec<Arc<dyn SearchSource>>) -> Self {
        Self { sources }
    }

    /// Run `query` against every source (each capped at `limit_per_source`
    /// results) and return a merged, re-ranked list.
    pub async fn query(&self, query: &str, limit_per_source: usize) -> Vec<SearchCandidate> {
        if query.trim().is_empty() || self.sources.is_empty() {
            return Vec::new();
        }

        let futs = self
            .sources
            .iter()
            .map(|s| {
                let s = s.clone();
                let q = query.to_owned();
                async move { s.search(&q, limit_per_source).await }
            })
            .collect::<Vec<_>>();
        let all: Vec<Vec<SearchCandidate>> = join_all(futs).await;

        let mut merged: Vec<SearchCandidate> = all.into_iter().flatten().collect();

        // Re-rank pass — boost exact prefix matches and bias per-source.
        let q_lower = query.to_lowercase();
        let short_q = query.chars().count() <= 3;
        let file_hinted = query.contains('/')
            || query.contains('\\')
            || query.contains('.')
            || query.starts_with('~');

        for c in &mut merged {
            if c.title.to_lowercase().starts_with(&q_lower) {
                c.score = c.score.saturating_add(40);
            }
            match c.source_id {
                "calculator" if query.trim_start().starts_with('=') => {
                    c.score = c.score.saturating_add(50);
                }
                "commands" if short_q => c.score = c.score.saturating_add(30),
                "files" if file_hinted => c.score = c.score.saturating_add(20),
                _ => {}
            }
        }

        merged.sort_by(|a, b| b.score.cmp(&a.score));
        let cap = (limit_per_source * 4).max(4);
        merged.truncate(cap);
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    use super::super::sources::{ActionTarget, SearchSource};

    struct FakeSource {
        id: &'static str,
        hits: Vec<SearchCandidate>,
    }

    #[async_trait]
    impl SearchSource for FakeSource {
        fn id(&self) -> &'static str {
            self.id
        }
        fn name_key(&self) -> &'static str {
            self.id
        }
        fn icon(&self) -> &'static str {
            "x"
        }
        async fn search(&self, _query: &str, limit: usize) -> Vec<SearchCandidate> {
            self.hits.iter().take(limit).cloned().collect()
        }
    }

    fn cand(source_id: &'static str, title: &str, score: i32) -> SearchCandidate {
        SearchCandidate {
            id: format!("{source_id}:{title}"),
            source_id,
            title: title.into(),
            subtitle: None,
            icon: "x",
            score,
            action_hint: None,
            action_target: ActionTarget::RunCommand("noop".into()),
        }
    }

    #[tokio::test]
    async fn short_query_boosts_commands_above_files() {
        let commands = Arc::new(FakeSource {
            id: "commands",
            hits: vec![cand("commands", "widget.create", 10)],
        }) as Arc<dyn SearchSource>;
        let files = Arc::new(FakeSource {
            id: "files",
            hits: vec![cand("files", "widget-thing", 10)],
        }) as Arc<dyn SearchSource>;
        let agg = SearchAggregator::new(vec![commands, files]);
        let results = agg.query("wid", 5).await;
        assert!(results.iter().any(|c| c.source_id == "commands"));
        let top = &results[0];
        assert_eq!(top.source_id, "commands");
    }

    #[tokio::test]
    async fn file_path_query_boosts_files() {
        let commands = Arc::new(FakeSource {
            id: "commands",
            hits: vec![cand("commands", "docs", 30)],
        }) as Arc<dyn SearchSource>;
        let files = Arc::new(FakeSource {
            id: "files",
            hits: vec![cand("files", "docs/README.md", 20)],
        }) as Arc<dyn SearchSource>;
        let agg = SearchAggregator::new(vec![commands, files]);
        let results = agg.query("docs/", 5).await;
        assert_eq!(results[0].source_id, "files");
    }
}
