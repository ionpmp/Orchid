//! Search source backed by [`orchid_search::SearchEngine`].

use std::sync::Arc;

use async_trait::async_trait;

use super::{ActionTarget, SearchCandidate, SearchSource};

/// Files source.
pub struct FilesSource {
    engine: Arc<orchid_search::SearchEngine>,
}

impl std::fmt::Debug for FilesSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilesSource").finish_non_exhaustive()
    }
}

impl FilesSource {
    /// Build over an existing engine.
    #[must_use]
    pub fn new(engine: Arc<orchid_search::SearchEngine>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl SearchSource for FilesSource {
    fn id(&self) -> &'static str {
        "files"
    }
    fn name_key(&self) -> &'static str {
        "search-source-files"
    }
    fn icon(&self) -> &'static str {
        "search-files"
    }
    async fn search(&self, query: &str, limit: usize) -> Vec<SearchCandidate> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        let q = orchid_search::QueryBuilder::new()
            .text(query)
            .limit(limit)
            .build();
        let Ok(results) = self.engine.search(q).await else {
            return Vec::new();
        };
        results
            .hits
            .into_iter()
            .map(|h| SearchCandidate {
                id: format!("file:{}", h.path),
                source_id: "files",
                title: h.name,
                subtitle: Some(h.path.clone()),
                icon: "search-files",
                // Tantivy scores are f32 in (~0..=30); keep the relative
                // ordering by scaling into i32.
                score: (h.score * 100.0) as i32,
                action_hint: None,
                action_target: ActionTarget::OpenFile(h.path),
            })
            .collect()
    }
}
