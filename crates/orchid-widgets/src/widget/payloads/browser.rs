//! Payload for the built-in browser widget.

#![allow(missing_docs)]

/// One tab row for the browser UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserTabRow {
    pub id: String,
    pub title: String,
    pub url: String,
    pub is_active: bool,
}

/// One bookmark row for the browser UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserBookmarkRow {
    pub title: String,
    pub url: String,
}

/// One download row for the browser UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserDownloadRow {
    pub id: String,
    pub filename: String,
    pub path: String,
    pub progress: i32,
    pub state: i32,
    pub in_progress: bool,
}

/// Render payload for the browser widget.
#[derive(Debug, Clone)]
pub struct BrowserPayload {
    pub tabs: Vec<BrowserTabRow>,
    pub active_index: i32,
    pub url: String,
    pub title: String,
    pub homepage: String,
    pub bookmarks: Vec<BrowserBookmarkRow>,
    pub is_bookmarked: bool,
    pub downloads: Vec<BrowserDownloadRow>,
}
