//! Result types for [`crate::SearchEngine::search`].

use crate::engine::DocumentKind;

/// A single search hit.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Canonical path.
    pub path: String,
    /// Last segment for UI.
    pub name: String,
    /// Lowercased file extension.
    pub extension: Option<String>,
    /// File size in bytes.
    pub size: u64,
    /// Last-modified Unix seconds.
    pub modified: i64,
    /// MIME type.
    pub mime: Option<String>,
    /// File or directory.
    pub kind: DocumentKind,
    /// Relevance score.
    pub score: f32,
    /// Optional content snippet for UI rendering.
    pub snippet: Option<Snippet>,
}

/// Content snippet with highlight ranges.
#[derive(Debug, Clone)]
pub struct Snippet {
    /// Raw snippet text.
    pub text: String,
    /// `(start, end)` character index pairs into `text`.
    pub highlights: Vec<(u32, u32)>,
}

/// Aggregate search result.
#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    /// Sorted hits.
    pub hits: Vec<SearchHit>,
    /// Best-effort estimate of the total match count.
    pub total_estimated: u64,
    /// Query wall-clock time in milliseconds.
    pub query_time_ms: u64,
}
