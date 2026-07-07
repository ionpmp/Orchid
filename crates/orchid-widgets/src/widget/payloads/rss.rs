//! Payload for the RSS feed widget.

/// Render-ready RSS payload.
#[derive(Debug, Clone)]
pub struct RssPayload {
    /// Items sorted newest-first.
    pub items: Vec<RssItemView>,
    /// Localised "Updated Xm ago" line.
    pub last_updated_text: String,
    /// `true` until the first fetch attempt completes.
    pub is_loading: bool,
    /// Number of enabled feeds in the widget config.
    pub enabled_feed_count: u32,
    /// Number of enabled feeds that failed on the last refresh.
    pub failed_feed_count: u32,
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
