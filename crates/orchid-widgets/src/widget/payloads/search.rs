//! Payload for the universal-search widget.

/// Render-ready universal-search payload.
#[derive(Debug, Clone)]
pub struct UniversalSearchPayload {
    /// Last query the widget ran.
    pub query: String,
    /// Ranked candidates ready for the list.
    pub candidates: Vec<SearchCandidateView>,
    /// Whether a search is still in progress.
    pub is_searching: bool,
    /// Optional error surfaced to the UI (e.g. index unavailable).
    pub error: Option<String>,
}

/// One candidate row.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct SearchCandidateView {
    pub id: String,
    pub source_name: String,
    pub source_icon: &'static str,
    pub title: String,
    pub subtitle: Option<String>,
    pub shortcut_hint: Option<String>,
}
