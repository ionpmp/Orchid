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
                url: "https://news.ycombinator.com/rss".into(),
                enabled: true,
            }],
            max_items_displayed: 20,
            refresh_interval_minutes: 15,
            open_in_browser: true,
        }
    }
}
