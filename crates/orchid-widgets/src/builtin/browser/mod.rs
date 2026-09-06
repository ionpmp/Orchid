//! Built-in browser — tabbed WebView2 host with an address bar.

pub mod config;

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{BrowserBookmarkRow, BrowserPayload, BrowserTabRow};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::{BrowserBookmark, BrowserConfig, BrowserTab, MAX_BOOKMARKS, MAX_TABS};

/// Stable type id.
pub const TYPE_ID: &str = "browser";

static BROWSER_LIVE: LazyLock<DashMap<Uuid, Arc<BrowserHandle>>> = LazyLock::new(DashMap::new);

struct BrowserHandle {
    instance_id: Uuid,
    config: Arc<RwLock<BrowserConfig>>,
    bus: Arc<orchid_core::EventBus>,
}

impl BrowserHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }
}

/// Snapshot the live config (tests / diagnostics).
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<BrowserConfig> {
    BROWSER_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Active tab id, if the instance is live.
#[must_use]
pub fn active_tab_id(instance_id: Uuid) -> Option<String> {
    BROWSER_LIVE.get(&instance_id).map(|h| {
        let cfg = h.config.read();
        cfg.active_tab().id.clone()
    })
}

/// Navigate the active tab to `raw` (address bar / search / file path).
pub fn navigate(instance_id: Uuid, raw: &str) {
    let Some(h) = BROWSER_LIVE.get(&instance_id) else {
        return;
    };
    let url = normalize_navigate_url(raw);
    {
        let mut cfg = h.config.write();
        let tab = cfg.active_tab_mut();
        tab.url = url;
        if tab.title.trim().is_empty() {
            tab.title = title_from_url(&tab.url);
        }
    }
    h.publish();
}

/// Switch the active tab.
pub fn select_tab(instance_id: Uuid, index: i32) {
    if index < 0 {
        return;
    }
    let Some(h) = BROWSER_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        if (index as usize) < cfg.tabs.len() {
            cfg.active_index = index as u32;
        }
    }
    h.publish();
}

/// Apply a settings-dialog mutation to the live config.
pub fn update_config(instance_id: Uuid, mutate: impl FnOnce(&mut BrowserConfig)) {
    let Some(h) = BROWSER_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        mutate(&mut cfg);
        cfg.normalize();
    }
    h.publish();
}

/// Navigate the active tab to the configured homepage (or `about:blank`).
pub fn go_home(instance_id: Uuid) {
    let Some(h) = BROWSER_LIVE.get(&instance_id) else {
        return;
    };
    let url = h.config.read().new_tab_url();
    navigate(instance_id, &url);
}

/// Create a new tab (homepage, or blank) and focus it.
pub fn new_tab(instance_id: Uuid) {
    let Some(h) = BROWSER_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        if cfg.tabs.len() >= MAX_TABS {
            return;
        }
        let url = cfg.new_tab_url();
        cfg.tabs.push(BrowserTab::from_url(&url));
        cfg.active_index = (cfg.tabs.len() - 1) as u32;
    }
    h.publish();
}

/// Open `url` in a new tab (target=_blank / window.open).
pub fn new_tab_with_url(instance_id: Uuid, url: &str) {
    let Some(h) = BROWSER_LIVE.get(&instance_id) else {
        return;
    };
    let url = normalize_navigate_url(url);
    {
        let mut cfg = h.config.write();
        cfg.open_in_new_tab(&url);
    }
    h.publish();
}

/// Close a tab by index. Keeps at least one tab.
pub fn close_tab(instance_id: Uuid, index: i32) {
    if index < 0 {
        return;
    }
    let Some(h) = BROWSER_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        let idx = index as usize;
        if cfg.tabs.len() <= 1 || idx >= cfg.tabs.len() {
            if cfg.tabs.len() == 1 {
                let url = cfg.new_tab_url();
                cfg.tabs[0] = BrowserTab::from_url(&url);
                cfg.active_index = 0;
            }
        } else {
            cfg.tabs.remove(idx);
            if cfg.active_index as usize >= cfg.tabs.len() {
                cfg.active_index = (cfg.tabs.len() - 1) as u32;
            } else if (cfg.active_index as usize) > idx {
                cfg.active_index = cfg.active_index.saturating_sub(1);
            }
        }
        cfg.normalize();
    }
    h.publish();
}

/// Apply a WebView2 location / title update for a specific tab.
pub fn tab_navigated(instance_id: Uuid, tab_id: &str, url: &str, title: &str) {
    let Some(h) = BROWSER_LIVE.get(&instance_id) else {
        return;
    };
    let url = url.trim();
    if url.is_empty() {
        return;
    }
    let changed = {
        let mut cfg = h.config.write();
        let Some(tab) = cfg.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };
        let mut changed = false;
        if tab.url != url {
            tab.url = url.to_string();
            changed = true;
        }
        let next_title = title.trim();
        if !next_title.is_empty() && tab.title != next_title {
            tab.title = next_title.to_string();
            changed = true;
        }
        changed
    };
    if changed {
        h.publish();
    }
}

/// Star or unstar the active tab.
pub fn toggle_bookmark(instance_id: Uuid) {
    let Some(h) = BROWSER_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        cfg.toggle_active_bookmark();
    }
    h.publish();
}

/// Open a bookmark in the active tab.
pub fn open_bookmark(instance_id: Uuid, index: i32) {
    if index < 0 {
        return;
    }
    let Some(h) = BROWSER_LIVE.get(&instance_id) else {
        return;
    };
    let url = {
        let cfg = h.config.read();
        cfg.bookmarks.get(index as usize).map(|b| b.url.clone())
    };
    let Some(url) = url else {
        return;
    };
    navigate(instance_id, &url);
}

/// Remove a bookmark by index.
pub fn remove_bookmark(instance_id: Uuid, index: i32) {
    if index < 0 {
        return;
    }
    let Some(h) = BROWSER_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        let idx = index as usize;
        if idx < cfg.bookmarks.len() {
            cfg.bookmarks.remove(idx);
        }
    }
    h.publish();
}

/// Turn an address-bar string into a URL WebView2 can navigate to.
#[must_use]
pub fn normalize_navigate_url(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return "about:blank".to_string();
    }
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("about:") || lower.starts_with("data:") || lower.starts_with("blob:") {
        return s.to_string();
    }
    if has_url_scheme(s) {
        return s.to_string();
    }
    if looks_like_windows_path(s) {
        return windows_path_to_file_url(s);
    }
    let host = s.split(['/', '?', '#']).next().unwrap_or(s);
    let host_name = host.split(':').next().unwrap_or(host);
    if is_local_host(host_name) {
        return format!("http://{s}");
    }
    if !s.contains(char::is_whitespace) && looks_like_host(host_name) {
        return format!("https://{s}");
    }
    duckduckgo_search(s)
}

fn has_url_scheme(s: &str) -> bool {
    let Some(colon) = s.find("://") else {
        return false;
    };
    let scheme = &s[..colon];
    !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-')
}

fn looks_like_windows_path(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
    {
        return true;
    }
    s.starts_with("\\\\")
}

fn windows_path_to_file_url(s: &str) -> String {
    let path = std::path::Path::new(s);
    url::Url::from_file_path(path)
        .map(String::from)
        .unwrap_or_else(|_| {
            let normalized = s.replace('\\', "/");
            if normalized.starts_with("//") {
                format!("file:{normalized}")
            } else {
                format!("file:///{normalized}")
            }
        })
}

fn is_local_host(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    h.eq_ignore_ascii_case("localhost") || h == "::1" || h.starts_with("127.")
}

fn looks_like_host(host: &str) -> bool {
    if host.is_empty() || host.starts_with('.') || host.ends_with('.') {
        return false;
    }
    host.contains('.') && !host.contains(' ')
}

fn duckduckgo_search(query: &str) -> String {
    let mut url = url::Url::parse("https://duckduckgo.com/").expect("static URL");
    url.query_pairs_mut().append_pair("q", query);
    url.to_string()
}

fn title_from_url(url: &str) -> String {
    if url == "about:blank" {
        return String::new();
    }
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default()
}

/// Browser widget implementation.
pub struct BrowserWidget {
    instance_id: Uuid,
    handle: Arc<BrowserHandle>,
}

impl std::fmt::Debug for BrowserWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl BrowserWidget {
    /// Construct with config.
    pub fn new(
        instance_id: Uuid,
        mut config: BrowserConfig,
        bus: Arc<orchid_core::EventBus>,
    ) -> Self {
        config.normalize();
        let handle = Arc::new(BrowserHandle {
            instance_id,
            config: Arc::new(RwLock::new(config)),
            bus,
        });
        BROWSER_LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
        }
    }

    fn build_payload(&self) -> BrowserPayload {
        let cfg = self.handle.config.read();
        let active = cfg.active_tab();
        let tabs = cfg
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| BrowserTabRow {
                id: t.id.clone(),
                title: t.title.clone(),
                url: t.url.clone(),
                is_active: i == cfg.active_index as usize,
            })
            .collect();
        BrowserPayload {
            tabs,
            active_index: cfg.active_index as i32,
            url: active.url.clone(),
            title: active.title.clone(),
            homepage: cfg.homepage.clone(),
            bookmarks: cfg
                .bookmarks
                .iter()
                .map(|b| BrowserBookmarkRow {
                    title: b.title.clone(),
                    url: b.url.clone(),
                })
                .collect(),
            is_bookmarked: cfg.active_is_bookmarked(),
        }
    }
}

#[async_trait]
impl Widget for BrowserWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }

    fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        BROWSER_LIVE.remove(&self.instance_id);
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let payload = self.build_payload();
        let title = if payload.title.trim().is_empty() {
            String::new()
        } else {
            payload.title.clone()
        };
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title,
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Browser(payload),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let mut cfg = BrowserConfig::decode_state(bytes);
        cfg.normalize();
        *self.handle.config.write() = cfg;
        Ok(())
    }

    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Small),
            max_size: None,
            preferred_size: Some(WidgetSize::Large),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: true,
        }
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let mut cfg = match state_bytes {
            Some(bytes) => BrowserConfig::decode_state(bytes),
            None => BrowserConfig::default(),
        };
        cfg.normalize();
        Ok(Box::new(BrowserWidget::new(ctx.instance_id, cfg, ctx.bus.clone())) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-browser-name",
        description_key: "widget-browser-desc",
        icon_name: "browser",
        category: WidgetCategory::Information,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Small),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_empty_is_blank() {
        assert_eq!(normalize_navigate_url(""), "about:blank");
        assert_eq!(normalize_navigate_url("   "), "about:blank");
    }

    #[test]
    fn normalize_keeps_schemes() {
        assert_eq!(
            normalize_navigate_url("https://example.com/x"),
            "https://example.com/x"
        );
        assert_eq!(normalize_navigate_url("about:blank"), "about:blank");
        assert_eq!(
            normalize_navigate_url("file:///C:/tmp/a.html"),
            "file:///C:/tmp/a.html"
        );
    }

    #[test]
    fn normalize_localhost_uses_http() {
        assert_eq!(
            normalize_navigate_url("localhost:3000"),
            "http://localhost:3000"
        );
        assert_eq!(
            normalize_navigate_url("127.0.0.1/status"),
            "http://127.0.0.1/status"
        );
    }

    #[test]
    fn normalize_dotted_host_uses_https() {
        assert_eq!(normalize_navigate_url("example.com"), "https://example.com");
        assert_eq!(
            normalize_navigate_url("example.com/docs"),
            "https://example.com/docs"
        );
    }

    #[test]
    fn normalize_search_query() {
        let url = normalize_navigate_url("rust webview2");
        assert!(url.starts_with("https://duckduckgo.com/?q="), "{url}");
        assert!(url.contains("rust"), "{url}");
    }

    #[test]
    fn normalize_windows_path() {
        let url = normalize_navigate_url(r"C:\temp\page.html");
        assert!(url.starts_with("file:"), "{url}");
        assert!(url.to_ascii_lowercase().contains("page.html"), "{url}");
    }

    #[test]
    fn config_caps_tabs() {
        let mut cfg = BrowserConfig {
            tabs: (0..20).map(|_| BrowserTab::blank()).collect(),
            active_index: 99,
            ..BrowserConfig::default()
        };
        cfg.normalize();
        assert_eq!(cfg.tabs.len(), MAX_TABS);
        assert_eq!(cfg.active_index, (MAX_TABS - 1) as u32);
    }

    #[test]
    fn new_tab_url_uses_homepage() {
        let mut cfg = BrowserConfig::default();
        assert_eq!(cfg.new_tab_url(), "about:blank");
        cfg.homepage = "https://example.com/".to_string();
        assert_eq!(cfg.new_tab_url(), "https://example.com/");
    }

    #[test]
    fn toggle_bookmark_skips_blank() {
        let mut cfg = BrowserConfig::default();
        cfg.toggle_active_bookmark();
        assert!(cfg.bookmarks.is_empty());
        cfg.tabs[0].url = "https://example.com/".to_string();
        cfg.tabs[0].title = "Example".to_string();
        cfg.toggle_active_bookmark();
        assert_eq!(cfg.bookmarks.len(), 1);
        assert!(cfg.active_is_bookmarked());
        cfg.toggle_active_bookmark();
        assert!(cfg.bookmarks.is_empty());
    }

    #[test]
    fn decode_state_accepts_v0() {
        #[derive(serde::Serialize)]
        struct V0 {
            tabs: Vec<BrowserTab>,
            active_index: u32,
        }
        let v0 = V0 {
            tabs: vec![BrowserTab {
                id: "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".to_string(),
                url: "https://example.com/".to_string(),
                title: "Example".to_string(),
            }],
            active_index: 0,
        };
        let bytes = state_codec::save_state(&v0).expect("encode v0");
        let cfg = BrowserConfig::decode_state(&bytes);
        assert_eq!(cfg.tabs.len(), 1);
        assert_eq!(cfg.tabs[0].url, "https://example.com/");
        assert!(cfg.homepage.is_empty());
        assert!(cfg.bookmarks.is_empty());
    }

    #[test]
    fn open_in_new_tab_appends_until_cap() {
        let mut cfg = BrowserConfig::default();
        cfg.open_in_new_tab("https://example.com/a");
        assert_eq!(cfg.tabs.len(), 2);
        assert_eq!(cfg.tabs[1].url, "https://example.com/a");
        assert_eq!(cfg.active_index, 1);
        cfg.tabs = (0..MAX_TABS)
            .map(|_| BrowserTab::from_url("https://kept.example/"))
            .collect();
        cfg.active_index = 0;
        cfg.open_in_new_tab("https://overflow.example/");
        assert_eq!(cfg.tabs.len(), MAX_TABS);
        assert_eq!(cfg.tabs[0].url, "https://overflow.example/");
    }
}
