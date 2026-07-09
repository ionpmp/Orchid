//! Config / pure-logic smoke tests for builtin widgets.

use orchid_widgets::builtin::moon::config::MoonConfig;
use orchid_widgets::builtin::rss::config::{FeedSource, RssConfig};
use orchid_widgets::builtin::rss::provider::parse_bytes;
use orchid_widgets::builtin::system::config::SystemConfig;

#[test]
fn moon_config_defaults_to_semarang() {
    let cfg = MoonConfig::default();
    assert!((cfg.latitude - (-6.9667)).abs() < 0.001);
    assert!((cfg.longitude - 110.4167).abs() < 0.001);
    assert_eq!(cfg.location_name, "Semarang");
    assert!(cfg.show_sunrise_sunset);
    assert!(!cfg.show_libration);
}

#[test]
fn system_config_defaults_enable_core_indicators() {
    let cfg = SystemConfig::default();
    assert!(cfg.show_cpu);
    assert!(cfg.show_memory);
    assert!(cfg.show_disks);
    assert!(cfg.show_network);
    assert!(cfg.show_battery);
    assert!(cfg.show_uptime);
    assert_eq!(cfg.refresh_interval_seconds, 2);
    assert!(cfg.network_interfaces.is_empty());
    assert!(cfg.disks.is_empty());
}

#[test]
fn rss_config_normalize_restores_empty_feeds() {
    let mut cfg = RssConfig {
        feeds: vec![],
        max_items_displayed: 0,
        refresh_interval_minutes: 0,
        open_in_browser: false,
    };
    cfg.normalize();
    assert!(!cfg.feeds.is_empty());
    assert_eq!(cfg.max_items_displayed, 20);
    assert_eq!(cfg.refresh_interval_minutes, 15);
    assert!(cfg.feeds.iter().any(|f| f.url.contains("hnrss")));
}

#[test]
fn rss_parse_bytes_fixture() {
    const FIXTURE: &[u8] = br#"<?xml version="1.0"?>
<rss version="2.0">
  <channel>
    <title>Fixture</title>
    <link>http://example.com</link>
    <item>
      <title>Alpha</title>
      <link>http://example.com/a</link>
      <guid>a-1</guid>
    </item>
  </channel>
</rss>"#;
    let items = parse_bytes(FIXTURE, "Fixture").unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Alpha");
    assert_eq!(items[0].source_name, "Fixture");
}

#[test]
fn rss_feed_source_roundtrip_fields() {
    let feed = FeedSource {
        name: "Local".into(),
        url: "https://example.com/feed.xml".into(),
        enabled: false,
    };
    assert!(!feed.enabled);
    assert!(feed.url.starts_with("https://"));
}
