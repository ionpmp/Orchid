//! Value types for the RSS widget.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

/// Aggregated view over every configured feed.
#[derive(Debug, Clone, Default)]
pub struct FeedData {
    /// Items merged across feeds, sorted newest first.
    pub items: Vec<FeedItem>,
    /// When this aggregate was produced.
    pub fetched_at: Option<DateTime<Utc>>,
    /// Per-feed error message for feeds that failed.
    pub per_feed_errors: HashMap<String, String>,
}

/// One item from a feed.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct FeedItem {
    pub id: String,
    pub title: String,
    pub link: Option<String>,
    pub summary: Option<String>,
    pub published: Option<DateTime<Utc>>,
    pub author: Option<String>,
    pub source_name: String,
}
