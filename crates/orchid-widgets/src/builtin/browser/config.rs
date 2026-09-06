//! Persistent config for the built-in browser widget.

#![allow(missing_docs)]

use bincode_reloaded::{Decode, Encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::widget::config as state_codec;

/// Maximum number of tabs kept in one browser instance.
pub const MAX_TABS: usize = 16;

/// Maximum number of bookmarks kept in one browser instance.
pub const MAX_BOOKMARKS: usize = 50;

/// Session-only recently-closed stack (Ctrl+Shift+T).
pub const MAX_CLOSED: usize = 10;

/// One browser tab.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct BrowserTab {
    pub id: String,
    pub url: String,
    pub title: String,
}

impl BrowserTab {
    #[must_use]
    pub fn blank() -> Self {
        Self::from_url("about:blank")
    }

    #[must_use]
    pub fn from_url(url: &str) -> Self {
        let url = url.trim();
        Self {
            id: Uuid::new_v4().to_string(),
            url: if url.is_empty() {
                "about:blank".to_string()
            } else {
                url.to_string()
            },
            title: String::new(),
        }
    }
}

/// One saved bookmark.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, PartialEq, Eq)]
pub struct BrowserBookmark {
    pub title: String,
    pub url: String,
}

/// Tabs + index as persisted by the first Browser widget revision.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
struct BrowserConfigV0 {
    tabs: Vec<BrowserTab>,
    active_index: u32,
}

/// Persisted browser widget state.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct BrowserConfig {
    pub tabs: Vec<BrowserTab>,
    pub active_index: u32,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub bookmarks: Vec<BrowserBookmark>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            tabs: vec![BrowserTab::blank()],
            active_index: 0,
            homepage: String::new(),
            bookmarks: Vec::new(),
        }
    }
}

impl BrowserConfig {
    /// Decode widget state, accepting the original tabs-only layout.
    #[must_use]
    pub fn decode_state(bytes: &[u8]) -> Self {
        if let Ok(cfg) = state_codec::restore_state::<Self>(bytes) {
            return cfg;
        }
        if let Ok(v0) = state_codec::restore_state::<BrowserConfigV0>(bytes) {
            return Self {
                tabs: v0.tabs,
                active_index: v0.active_index,
                homepage: String::new(),
                bookmarks: Vec::new(),
            };
        }
        Self::default()
    }

    /// URL opened by New tab / last-tab reset / Home (empty homepage → blank).
    #[must_use]
    pub fn new_tab_url(&self) -> String {
        let home = self.homepage.trim();
        if home.is_empty() {
            "about:blank".to_string()
        } else {
            home.to_string()
        }
    }

    /// Open `url` in a new tab (or the active tab if the cap is reached).
    pub fn open_in_new_tab(&mut self, url: &str) {
        let url = url.trim();
        let url = if url.is_empty() {
            "about:blank".to_string()
        } else {
            url.to_string()
        };
        if self.tabs.len() >= MAX_TABS {
            let tab = self.active_tab_mut();
            tab.url = url;
            tab.title.clear();
            return;
        }
        self.tabs.push(BrowserTab::from_url(&url));
        self.active_index = (self.tabs.len() - 1) as u32;
    }

    /// Restore a closed tab with a fresh id. No-op at the tab cap.
    pub fn restore_tab(&mut self, tab: BrowserTab) -> bool {
        if self.tabs.len() >= MAX_TABS {
            return false;
        }
        let mut next = BrowserTab::from_url(&tab.url);
        next.title = tab.title;
        self.tabs.push(next);
        self.active_index = (self.tabs.len() - 1) as u32;
        true
    }

    /// Remember `tab` for Ctrl+Shift+T. Skips blank pages.
    pub fn remember_closed(stack: &mut Vec<BrowserTab>, tab: BrowserTab) {
        let url = tab.url.trim();
        if url.is_empty() || url.eq_ignore_ascii_case("about:blank") {
            return;
        }
        stack.push(tab);
        if stack.len() > MAX_CLOSED {
            stack.remove(0);
        }
    }

    /// Whether the active tab's URL is in the bookmark list.
    #[must_use]
    pub fn active_is_bookmarked(&self) -> bool {
        let url = self.active_tab().url.trim();
        bookmarkable(url) && self.bookmarks.iter().any(|b| b.url == url)
    }

    /// Star / unstar the active tab. No-ops for blank pages.
    pub fn toggle_active_bookmark(&mut self) {
        let tab = self.active_tab().clone();
        let url = tab.url.trim();
        if !bookmarkable(url) {
            return;
        }
        if let Some(i) = self.bookmarks.iter().position(|b| b.url == url) {
            self.bookmarks.remove(i);
            return;
        }
        if self.bookmarks.len() >= MAX_BOOKMARKS {
            self.bookmarks.remove(0);
        }
        let title = if tab.title.trim().is_empty() {
            url.to_string()
        } else {
            tab.title.clone()
        };
        self.bookmarks.push(BrowserBookmark {
            title,
            url: url.to_string(),
        });
    }

    /// Move the active tab by `delta` (wraps around).
    pub fn cycle_active(&mut self, delta: i32) {
        let n = self.tabs.len() as i32;
        if n <= 0 {
            return;
        }
        self.active_index = (self.active_index as i32 + delta).rem_euclid(n) as u32;
    }

    /// Ctrl+1..8 select that tab if it exists; Ctrl+9 selects the last tab.
    pub fn select_numbered(&mut self, n: i32) {
        if n < 1 || self.tabs.is_empty() {
            return;
        }
        if n >= 9 {
            self.active_index = (self.tabs.len() - 1) as u32;
            return;
        }
        let idx = (n as usize).saturating_sub(1);
        if idx < self.tabs.len() {
            self.active_index = idx as u32;
        }
    }

    /// Ensure at least one tab, a valid active index, and caps.
    pub fn normalize(&mut self) {
        if self.tabs.is_empty() {
            self.tabs.push(BrowserTab::from_url(&self.new_tab_url()));
        }
        if self.tabs.len() > MAX_TABS {
            self.tabs.truncate(MAX_TABS);
        }
        if self.bookmarks.len() > MAX_BOOKMARKS {
            self.bookmarks.truncate(MAX_BOOKMARKS);
        }
        self.homepage = self.homepage.trim().to_string();
        self.bookmarks.retain(|b| bookmarkable(b.url.trim()));
        for tab in &mut self.tabs {
            if tab.id.trim().is_empty() {
                tab.id = Uuid::new_v4().to_string();
            }
            if tab.url.trim().is_empty() {
                tab.url = "about:blank".to_string();
            }
        }
        for bm in &mut self.bookmarks {
            if bm.title.trim().is_empty() {
                bm.title = bm.url.clone();
            }
        }
        let max = (self.tabs.len().saturating_sub(1)) as u32;
        if self.active_index > max {
            self.active_index = max;
        }
    }

    #[must_use]
    pub fn active_tab(&self) -> &BrowserTab {
        self.tabs
            .get(self.active_index as usize)
            .unwrap_or(&self.tabs[0])
    }

    pub fn active_tab_mut(&mut self) -> &mut BrowserTab {
        self.normalize();
        let idx = self.active_index as usize;
        &mut self.tabs[idx]
    }
}

fn bookmarkable(url: &str) -> bool {
    !url.is_empty() && url != "about:blank"
}
