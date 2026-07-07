//! RSS widget persistent configuration.

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// One feed source entry.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct FeedSource {
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

/// Persistent RSS-widget config.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct RssConfig {
    pub feeds: Vec<FeedSource>,
    pub max_items_displayed: u32,
    pub refresh_interval_minutes: u32,
    pub open_in_browser: bool,
}

impl Default for RssConfig {
    fn default() -> Self {
        Self {
            feeds: vec![FeedSource {
                name: "Hacker News".into(),
                url: "https://hnrss.org/frontpage".into(),
                enabled: true,
            }],
            max_items_displayed: 20,
            refresh_interval_minutes: 15,
            open_in_browser: true,
        }
    }
}

impl RssConfig {
    /// Fill in sane defaults for empty or invalid persisted state.
    pub fn normalize(&mut self) {
        if self.feeds.is_empty() {
            self.feeds = Self::default().feeds;
        }
        if self.max_items_displayed == 0 {
            self.max_items_displayed = 20;
        }
        if self.refresh_interval_minutes == 0 {
            self.refresh_interval_minutes = 15;
        }
    }
}
