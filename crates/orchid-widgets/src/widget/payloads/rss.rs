//! Payload for the RSS feed widget.

/// Render-ready RSS payload.
#[derive(Debug, Clone)]
pub struct RssPayload {
    /// Items sorted newest-first.
    pub items: Vec<RssItemView>,
    /// Localised "Updated Xm ago" line.
    pub last_updated_text: String,
    /// Optional localised error summary (e.g. "2 of 5 feeds failed").
    pub error_summary: Option<String>,
}

/// One RSS item as rendered in the UI list.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct RssItemView {
    pub id: String,
    pub title: String,
    pub source_name: String,
    pub published_text: String,
    pub summary_text: Option<String>,
    pub link: Option<String>,
}
