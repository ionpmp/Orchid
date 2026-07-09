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
                subtitle: Some(file_hit_subtitle(&h.path, h.snippet.as_ref())),
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

fn file_hit_subtitle(path: &str, snippet: Option<&orchid_search::Snippet>) -> String {
    let Some(snippet) = snippet else {
        return path.to_string();
    };
    let collapsed: String = snippet
        .text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.is_empty() {
        return path.to_string();
    }
    let excerpt: String = if collapsed.chars().count() > 100 {
        let truncated: String = collapsed.chars().take(100).collect();
        format!("{truncated}…")
    } else {
        collapsed
    };
    format!("{path} — {excerpt}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_search::Snippet;

    #[test]
    fn subtitle_falls_back_to_path_without_snippet() {
        assert_eq!(
            file_hit_subtitle("local:/a/b.txt", None),
            "local:/a/b.txt"
        );
    }

    #[test]
    fn subtitle_includes_collapsed_snippet() {
        let sn = Snippet {
            text: "  hello\n  world  ".into(),
            highlights: vec![(0, 5)],
        };
        let out = file_hit_subtitle("local:/note.md", Some(&sn));
        assert!(out.starts_with("local:/note.md — "));
        assert!(out.contains("hello world"));
        assert!(!out.contains('\n'));
    }
}
