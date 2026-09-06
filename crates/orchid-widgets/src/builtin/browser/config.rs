//! Persistent config for the built-in browser widget.

#![allow(missing_docs)]

use bincode_reloaded::{Decode, Encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Maximum number of tabs kept in one browser instance.
pub const MAX_TABS: usize = 16;

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
        Self {
            id: Uuid::new_v4().to_string(),
            url: "about:blank".to_string(),
            title: String::new(),
        }
    }
}

/// Persisted browser widget state.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct BrowserConfig {
    pub tabs: Vec<BrowserTab>,
    pub active_index: u32,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            tabs: vec![BrowserTab::blank()],
            active_index: 0,
        }
    }
}

impl BrowserConfig {
    /// Ensure at least one tab, a valid active index, and a tab cap.
    pub fn normalize(&mut self) {
        if self.tabs.is_empty() {
            self.tabs.push(BrowserTab::blank());
        }
        if self.tabs.len() > MAX_TABS {
            self.tabs.truncate(MAX_TABS);
        }
        for tab in &mut self.tabs {
            if tab.id.trim().is_empty() {
                tab.id = Uuid::new_v4().to_string();
            }
            if tab.url.trim().is_empty() {
                tab.url = "about:blank".to_string();
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
