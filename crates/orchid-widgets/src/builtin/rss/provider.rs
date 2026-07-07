//! `feed-rs`-backed RSS / Atom / JSON feed provider.

use std::time::Duration;

use chrono::Utc;
use feed_rs::parser;
use futures::future::join_all;

use super::config::FeedSource;
use super::types::{FeedData, FeedItem};

/// Provider that fetches every enabled feed in parallel.
#[derive(Debug, Clone)]
pub struct RssProvider {
    client: reqwest::Client,
}

impl RssProvider {
    /// Construct with a pre-built HTTP client.
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Default HTTP client with 15 s per-request timeout and Orchid
    /// User-Agent.
    ///
    /// # Errors
    ///
    /// Propagates [`reqwest::Error`] on builder failure.
    pub fn default_client() -> std::result::Result<reqwest::Client, reqwest::Error> {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("orchid-widgets/", env!("CARGO_PKG_VERSION")))
            .build()
    }

    /// Fetch every enabled feed; failures are recorded per-feed but never
    /// fail the whole call. Items are merged and sorted newest-first.
    pub async fn fetch_all(&self, feeds: &[FeedSource]) -> FeedData {
        let futs = feeds.iter().filter(|f| f.enabled).map(|feed| {
            let client = self.client.clone();
            let source = feed.clone();
            async move { (source.clone(), fetch_one(&client, &source).await) }
        });
        let results = join_all(futs).await;

        let mut items: Vec<FeedItem> = Vec::new();
        let mut per_feed_errors = std::collections::HashMap::new();
        for (source, res) in results {
            match res {
                Ok(mut fetched) => items.append(&mut fetched),
                Err(e) => {
                    per_feed_errors.insert(source.name.clone(), e);
                }
            }
        }
        items.sort_by(|a, b| {
            b.published
                .unwrap_or_else(chrono::Utc::now)
                .cmp(&a.published.unwrap_or_else(chrono::Utc::now))
        });
        FeedData {
            items,
            fetched_at: Some(Utc::now()),
            per_feed_errors,
        }
    }
}

async fn fetch_one(
    client: &reqwest::Client,
    source: &FeedSource,
) -> std::result::Result<Vec<FeedItem>, String> {
    let resp = client.get(&source.url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let body = resp.bytes().await.map_err(|e| e.to_string())?;
    parse_bytes(&body, &source.name)
}

/// Parse raw feed bytes; factored out for testing against fixtures.
pub fn parse_bytes(
    bytes: &[u8],
    source_name: &str,
) -> std::result::Result<Vec<FeedItem>, String> {
    let feed = parser::parse(bytes).map_err(|e| e.to_string())?;
    let items = feed
        .entries
        .into_iter()
        .map(|entry| {
            let title = entry
                .title
                .as_ref()
                .map(|t| t.content.clone())
                .unwrap_or_else(|| "(untitled)".to_string());
            let link = entry.links.first().map(|l| l.href.clone());
            let summary = entry
                .summary
                .as_ref()
                .map(|s| strip_html(&s.content))
                .filter(|s| !s.is_empty());
            let published = entry.published.or(entry.updated);
            let author = entry.authors.first().map(|a| a.name.clone());
            let id = if entry.id.is_empty() {
                link.clone()
                    .or_else(|| Some(format!("{source_name}:{title}")))
                    .unwrap_or_else(|| title.clone())
            } else {
                entry.id
            };
            FeedItem {
                id,
                title,
                link,
                summary,
                published,
                author,
                source_name: source_name.to_string(),
            }
        })
        .collect();
    Ok(items)
}

fn strip_html(s: &str) -> String {
    // Minimal HTML-tag stripper — not a full parser, good enough for the
    // 2-line summary shown in the widget.
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_RSS: &[u8] = br#"<?xml version="1.0"?>
<rss version="2.0">
  <channel>
    <title>Fixture</title>
    <link>http://example.com</link>
    <description>Test fixture feed.</description>
    <item>
      <title>Hello world</title>
      <link>http://example.com/1</link>
      <description>First item.</description>
      <pubDate>Mon, 20 Apr 2026 12:00:00 +0000</pubDate>
      <guid>example-1</guid>
    </item>
    <item>
      <title>Second story</title>
      <link>http://example.com/2</link>
      <description>&lt;p&gt;With <b>HTML</b>&lt;/p&gt;</description>
      <pubDate>Sun, 19 Apr 2026 12:00:00 +0000</pubDate>
      <guid>example-2</guid>
    </item>
  </channel>
</rss>"#;

    #[test]
    fn parses_rss_fixture() {
        let items = parse_bytes(FIXTURE_RSS, "Fixture").unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Hello world");
        assert_eq!(items[0].source_name, "Fixture");
        assert_eq!(items[0].link.as_deref(), Some("http://example.com/1"));
    }

    #[test]
    fn strip_html_removes_tags_and_collapses_whitespace() {
        assert_eq!(strip_html("<p>Hello  <b>world</b></p>"), "Hello world");
    }

    #[test]
    fn uses_link_as_id_when_entry_id_missing() {
        let items = parse_bytes(FIXTURE_RSS, "Fixture").unwrap();
        assert!(!items[0].id.is_empty());
    }
}
