//! HWND-hosted WebView2 overlay used by the HTML viewer and the browser widget.
//!
//! Slint has no native web control. On Windows the host creates a WebView2
//! controller as a child of the main window and positions it over the embed
//! rectangle. Missing runtime falls back to the source preview (HTML viewer)
//! or an unavailable hint (browser widget).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use tracing::{debug, warn};
use uuid::Uuid;

/// Navigation chrome update produced by WebView2 history / source events.
#[derive(Debug, Clone)]
pub(crate) struct HtmlNavState {
    pub instance_id: Uuid,
    pub surface_id: Uuid,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub url: Option<String>,
    pub title: Option<String>,
    pub is_loading: Option<bool>,
    pub zoom: Option<f64>,
}

/// Keyboard shortcut captured from a focused WebView2 controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserChromeAction {
    NewTab,
    CloseTab,
    FocusAddress,
    Reload,
    Stop,
    Find,
    Bookmark,
    Home,
    Back,
    Forward,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ReopenClosed,
    NextTab,
    PrevTab,
    SelectTab(u8),
    Downloads,
}

/// One chrome shortcut for the browser widget (UI-thread drain).
#[derive(Debug, Clone, Copy)]
pub(crate) struct BrowserChromeEvent {
    pub instance_id: Uuid,
    pub action: BrowserChromeAction,
}

/// PNG bytes for a browser tab favicon.
#[derive(Debug, Clone)]
pub(crate) struct HtmlFaviconUpdate {
    pub instance_id: Uuid,
    pub surface_id: Uuid,
    pub png: Vec<u8>,
}

/// `target=_blank` / `window.open` URL to open as a tab.
#[derive(Debug, Clone)]
pub(crate) struct BrowserOpenRequest {
    pub instance_id: Uuid,
    pub url: String,
}

/// Progress for one WebView2 download.
#[derive(Debug, Clone)]
pub(crate) struct BrowserDownloadEvent {
    pub instance_id: Uuid,
    pub id: Uuid,
    pub filename: String,
    pub path: String,
    pub bytes: u64,
    pub total: u64,
    pub state: u8,
}

/// Target document for a viewer / browser surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HtmlDocument {
    /// Nothing to show.
    None,
    /// Local (or remote) URL, typically `file://`.
    Url(String),
    /// In-memory HTML when the file is not on the local filesystem.
    Html(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct SlotKey {
    instance: Uuid,
    surface: Uuid,
}

impl SlotKey {
    fn html(instance: Uuid) -> Self {
        Self {
            instance,
            surface: Uuid::nil(),
        }
    }
}

/// Shared WebView2 host (cheap to clone; UI-thread affinity).
#[derive(Clone)]
pub(crate) struct HtmlWebViewHost {
    state: Arc<Mutex<HostState>>,
    nav: Arc<Mutex<Vec<HtmlNavState>>>,
    chrome: Arc<Mutex<Vec<BrowserChromeEvent>>>,
    opens: Arc<Mutex<Vec<BrowserOpenRequest>>>,
    favicons: Arc<Mutex<Vec<HtmlFaviconUpdate>>>,
    downloads: Arc<Mutex<Vec<BrowserDownloadEvent>>>,
    #[cfg(windows)]
    download_ops: Arc<
        Mutex<
            HashMap<
                Uuid,
                webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2DownloadOperation,
            >,
        >,
    >,
}

#[derive(Default)]
struct HostState {
    user_data_dir: PathBuf,
    env_pending: bool,
    env_failed: bool,
    #[cfg(windows)]
    env: Option<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment>,
    #[cfg(windows)]
    parent: windows::Win32::Foundation::HWND,
    slots: HashMap<SlotKey, Slot>,
}

struct Slot {
    document: HtmlDocument,
    last_applied: HtmlDocument,
    visible: bool,
    applied_visible: bool,
    loading: bool,
    zoom: f64,
    bounds: OverlayBounds,
    creating: bool,
    browser: bool,
    #[cfg(windows)]
    controller: Option<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller>,
    #[cfg(windows)]
    webview: Option<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2>,
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            document: HtmlDocument::None,
            last_applied: HtmlDocument::None,
            visible: false,
            applied_visible: false,
            loading: false,
            zoom: 1.0,
            bounds: OverlayBounds::default(),
            creating: false,
            browser: false,
            #[cfg(windows)]
            controller: None,
            #[cfg(windows)]
            webview: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct OverlayBounds {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl HtmlWebViewHost {
    /// Resolve the user-data folder and probe the runtime.
    pub(crate) fn new() -> Self {
        let user_data_dir = orchid_storage::OrchidPaths::resolve()
            .map(|p| p.cache_dir.join("webview2"))
            .unwrap_or_else(|_| std::env::temp_dir().join("orchid-webview2"));
        if let Err(e) = std::fs::create_dir_all(&user_data_dir) {
            warn!(?e, path = %user_data_dir.display(), "webview2 user-data dir");
        }
        Self {
            state: Arc::new(Mutex::new(HostState {
                user_data_dir,
                ..HostState::default()
            })),
            nav: Arc::new(Mutex::new(Vec::new())),
            chrome: Arc::new(Mutex::new(Vec::new())),
            opens: Arc::new(Mutex::new(Vec::new())),
            favicons: Arc::new(Mutex::new(Vec::new())),
            downloads: Arc::new(Mutex::new(Vec::new())),
            #[cfg(windows)]
            download_ops: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Evergreen WebView2 Runtime present on this machine.
    #[must_use]
    pub(crate) fn runtime_available() -> bool {
        probe_runtime()
    }

    /// Drain history-changed updates for the Slint chrome.
    pub(crate) fn take_nav_updates(&self) -> Vec<HtmlNavState> {
        let mut g = self.nav.lock();
        std::mem::take(&mut *g)
    }

    /// Drain in-page accelerator shortcuts for the browser widget.
    pub(crate) fn take_chrome_events(&self) -> Vec<BrowserChromeEvent> {
        let mut g = self.chrome.lock();
        std::mem::take(&mut *g)
    }

    /// Drain `NewWindowRequested` URLs for the browser widget.
    pub(crate) fn take_open_requests(&self) -> Vec<BrowserOpenRequest> {
        let mut g = self.opens.lock();
        std::mem::take(&mut *g)
    }

    /// Drain favicon PNG blobs for browser tabs.
    pub(crate) fn take_favicon_updates(&self) -> Vec<HtmlFaviconUpdate> {
        let mut g = self.favicons.lock();
        std::mem::take(&mut *g)
    }

    /// Drain download progress events.
    pub(crate) fn take_download_events(&self) -> Vec<BrowserDownloadEvent> {
        let mut g = self.downloads.lock();
        std::mem::take(&mut *g)
    }

    /// Cancel an in-flight WebView2 download.
    pub(crate) fn cancel_download(&self, id: Uuid) {
        #[cfg(windows)]
        {
            let op = self.download_ops.lock().get(&id).cloned();
            if let Some(op) = op {
                let _ = unsafe { op.Cancel() };
            }
        }
        #[cfg(not(windows))]
        {
            let _ = id;
        }
    }

    /// Remember the document to show for the HTML viewer (`surface = nil`).
    pub(crate) fn set_document(&self, id: Uuid, document: HtmlDocument) {
        self.set_document_key(SlotKey::html(id), document, false);
    }

    /// Remember the document for a browser tab surface.
    pub(crate) fn set_document_surface(
        &self,
        instance: Uuid,
        surface: Uuid,
        document: HtmlDocument,
    ) {
        self.set_document_key(SlotKey { instance, surface }, document, true);
    }

    fn set_document_key(&self, key: SlotKey, document: HtmlDocument, browser: bool) {
        {
            let mut st = self.state.lock();
            let slot = st.slots.entry(key).or_default();
            // Viewer content ticks call this on every patch with an unchanged
            // body; skipping the kick avoids waking the host thread.
            if slot.document == document && slot.browser == browser {
                return;
            }
            slot.document = document;
            slot.browser = browser;
        }
        self.kick();
    }

    /// Position the overlay in parent-client physical pixels (HTML viewer).
    pub(crate) fn set_bounds(
        &self,
        id: Uuid,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        visible: bool,
        parent_hwnd: isize,
    ) {
        self.set_bounds_key(SlotKey::html(id), x, y, w, h, visible, parent_hwnd);
    }

    /// Position a browser-tab overlay.
    pub(crate) fn set_bounds_surface(
        &self,
        instance: Uuid,
        surface: Uuid,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        visible: bool,
        parent_hwnd: isize,
    ) {
        self.set_bounds_key(
            SlotKey { instance, surface },
            x,
            y,
            w,
            h,
            visible,
            parent_hwnd,
        );
    }

    fn set_bounds_key(
        &self,
        key: SlotKey,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        visible: bool,
        #[cfg_attr(not(windows), allow(unused_variables))] parent_hwnd: isize,
    ) {
        {
            let mut st = self.state.lock();
            #[cfg(windows)]
            {
                st.parent = windows::Win32::Foundation::HWND(parent_hwnd as *mut _);
            }
            let slot = st.slots.entry(key).or_default();
            slot.bounds = OverlayBounds { x, y, w, h };
            slot.visible = visible && w > 8 && h > 8;
        }
        self.kick();
    }

    /// Back / forward / reload for the HTML viewer.
    pub(crate) fn command(&self, id: Uuid, command: &str) {
        self.command_key(SlotKey::html(id), command);
    }

    /// Back / forward / reload for a browser tab.
    pub(crate) fn command_surface(&self, instance: Uuid, surface: Uuid, command: &str) {
        self.command_key(SlotKey { instance, surface }, command);
    }

    /// Find in the current page via `window.find` (WebView2 0.39 has no Find API).
    pub(crate) fn find_in_page(&self, instance: Uuid, surface: Uuid, query: &str, forward: bool) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }
        #[cfg(windows)]
        {
            use webview2_com::ExecuteScriptCompletedHandler;
            use windows::core::HSTRING;

            let webview = {
                let st = self.state.lock();
                st.slots
                    .get(&SlotKey { instance, surface })
                    .and_then(|s| s.webview.clone())
            };
            let Some(webview) = webview else {
                return;
            };
            let literal = serde_json::to_string(query).unwrap_or_else(|_| "\"\"".to_string());
            let backwards = if forward { "false" } else { "true" };
            let js = format!(
                "(function(){{ try {{ window.find({literal}, false, {backwards}, true, false, true, false); }} catch (e) {{}} }})()"
            );
            let handler = ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(())));
            if let Err(e) = unsafe { webview.ExecuteScript(&HSTRING::from(js.as_str()), &handler) }
            {
                warn!(?e, instance = %instance, "webview2 find");
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (instance, surface, query, forward);
        }
    }

    fn command_key(&self, key: SlotKey, command: &str) {
        #[cfg(windows)]
        {
            if matches!(command, "zoom-in" | "zoom-out" | "zoom-reset") {
                self.apply_zoom(key, command);
                return;
            }
            let webview = {
                let mut st = self.state.lock();
                let Some(slot) = st.slots.get_mut(&key) else {
                    return;
                };
                if command == "stop" {
                    slot.loading = false;
                }
                slot.webview.clone()
            };
            let Some(webview) = webview else {
                return;
            };
            let result = unsafe {
                match command {
                    "back" => webview.GoBack(),
                    "forward" => webview.GoForward(),
                    "reload" => webview.Reload(),
                    "stop" => webview.Stop(),
                    _ => return,
                }
            };
            if command == "stop" {
                push_nav(&self.nav, key, &webview, Some(false), None);
            }
            if let Err(e) = result {
                warn!(?e, command, instance = %key.instance, surface = %key.surface, "webview2 command");
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (key, command);
        }
    }

    #[cfg(windows)]
    fn apply_zoom(&self, key: SlotKey, command: &str) {
        let (controller, webview, zoom) = {
            let mut st = self.state.lock();
            let Some(slot) = st.slots.get_mut(&key) else {
                return;
            };
            let next = match command {
                "zoom-in" => ((slot.zoom * 100.0).round() + 10.0) / 100.0,
                "zoom-out" => ((slot.zoom * 100.0).round() - 10.0) / 100.0,
                _ => 1.0,
            };
            slot.zoom = next.clamp(0.25, 3.0);
            (slot.controller.clone(), slot.webview.clone(), slot.zoom)
        };
        if let Some(controller) = controller {
            if let Err(e) = unsafe { controller.SetZoomFactor(zoom) } {
                warn!(?e, instance = %key.instance, "webview2 SetZoomFactor");
            }
        }
        if let Some(webview) = webview {
            push_nav(&self.nav, key, &webview, None, Some(zoom));
        }
    }

    /// Close every controller for a widget instance (HTML viewer or all browser tabs).
    pub(crate) fn destroy(&self, id: Uuid) {
        let keys: Vec<SlotKey> = {
            let st = self.state.lock();
            st.slots
                .keys()
                .copied()
                .filter(|k| k.instance == id)
                .collect()
        };
        for key in keys {
            self.destroy_key(key);
        }
    }

    /// Close one browser-tab surface.
    pub(crate) fn destroy_surface(&self, instance: Uuid, surface: Uuid) {
        self.destroy_key(SlotKey { instance, surface });
    }

    fn destroy_key(&self, key: SlotKey) {
        let slot = {
            let mut st = self.state.lock();
            st.slots.remove(&key)
        };
        #[cfg(windows)]
        if let Some(slot) = slot {
            if let Some(controller) = slot.controller {
                if let Err(e) = unsafe { controller.Close() } {
                    debug!(?e, instance = %key.instance, surface = %key.surface, "webview2 close");
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = slot;
        }
    }

    /// Hide overlays whose widgets are not currently painted.
    pub(crate) fn hide_except(&self, visible: &[Uuid]) {
        let to_hide: Vec<SlotKey> = {
            let st = self.state.lock();
            st.slots
                .iter()
                .filter(|(key, slot)| slot.visible && !visible.contains(&key.instance))
                .map(|(key, _)| *key)
                .collect()
        };
        for key in to_hide {
            {
                let mut st = self.state.lock();
                if let Some(slot) = st.slots.get_mut(&key) {
                    slot.visible = false;
                }
            }
            self.apply_bounds(key);
        }
    }
}

fn probe_runtime() -> bool {
    #[cfg(windows)]
    {
        use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
        use windows::core::{PCWSTR, PWSTR};
        use windows::Win32::System::Com::CoTaskMemFree;

        let mut version = PWSTR::null();
        let hr =
            unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version) };
        let present = hr.is_ok() && !version.is_null();
        if !version.is_null() {
            unsafe { CoTaskMemFree(Some(version.0.cast())) };
        }
        present
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Build a `file://` URL for an OS path.
#[must_use]
pub(crate) fn file_url_from_path(path: &Path) -> Option<String> {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    url::Url::from_file_path(&abs).ok().map(String::from)
}

impl HtmlWebViewHost {
    fn kick(&self) {
        #[cfg(windows)]
        {
            self.ensure_environment();
            let keys: Vec<SlotKey> = {
                let st = self.state.lock();
                st.slots.keys().copied().collect()
            };
            for key in keys {
                self.ensure_controller(key);
                self.apply_document(key);
                self.apply_bounds(key);
            }
        }
    }

    #[cfg(windows)]
    fn ensure_environment(&self) {
        use webview2_com::CreateCoreWebView2EnvironmentCompletedHandler;
        use webview2_com::Microsoft::Web::WebView2::Win32::CreateCoreWebView2EnvironmentWithOptions;
        use windows::core::PCWSTR;

        let user_data = {
            let mut st = self.state.lock();
            if st.env.is_some() || st.env_pending || st.env_failed {
                return;
            }
            if !probe_runtime() {
                st.env_failed = true;
                return;
            }
            st.env_pending = true;
            st.user_data_dir.clone()
        };

        let mut wide: Vec<u16> = user_data
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let host = self.clone();
        let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
            move |error_code, env| {
                if let Err(e) = error_code {
                    warn!(?e, "webview2 environment");
                    let mut st = host.state.lock();
                    st.env_pending = false;
                    st.env_failed = true;
                    return Ok(());
                }
                {
                    let mut st = host.state.lock();
                    st.env_pending = false;
                    st.env = env;
                }
                host.kick();
                Ok(())
            },
        ));
        let hr = unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                PCWSTR(wide.as_mut_ptr()),
                None,
                &handler,
            )
        };
        if let Err(e) = hr {
            warn!(?e, "CreateCoreWebView2EnvironmentWithOptions");
            let mut st = self.state.lock();
            st.env_pending = false;
            st.env_failed = true;
        }
    }

    #[cfg(windows)]
    fn ensure_controller(&self, key: SlotKey) {
        use webview2_com::CreateCoreWebView2ControllerCompletedHandler;

        let (env, parent, browser) = {
            let mut st = self.state.lock();
            let Some(env) = st.env.clone() else {
                return;
            };
            if st.parent.0.is_null() {
                return;
            }
            let parent = st.parent;
            let Some(slot) = st.slots.get_mut(&key) else {
                return;
            };
            if slot.controller.is_some() || slot.creating {
                return;
            }
            if !slot.visible {
                return;
            }
            slot.creating = true;
            (env, parent, slot.browser)
        };

        let host = self.clone();
        let handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
            move |error_code, controller| {
                if let Err(e) = error_code {
                    warn!(?e, instance = %key.instance, "webview2 controller");
                    if let Some(slot) = host.state.lock().slots.get_mut(&key) {
                        slot.creating = false;
                    }
                    return Ok(());
                }
                let Some(controller) = controller else {
                    if let Some(slot) = host.state.lock().slots.get_mut(&key) {
                        slot.creating = false;
                    }
                    return Ok(());
                };
                let webview = match unsafe { controller.CoreWebView2() } {
                    Ok(w) => w,
                    Err(e) => {
                        warn!(?e, instance = %key.instance, "CoreWebView2");
                        if let Some(slot) = host.state.lock().slots.get_mut(&key) {
                            slot.creating = false;
                        }
                        return Ok(());
                    }
                };
                if let Ok(settings) = unsafe { webview.Settings() } {
                    let _ = unsafe { settings.SetAreDefaultContextMenusEnabled(true) };
                    let _ = unsafe { settings.SetAreDevToolsEnabled(browser) };
                    let _ = unsafe { settings.SetIsStatusBarEnabled(false) };
                }
                attach_history(&host, key, &webview);
                if browser {
                    attach_location(&host, key, &webview);
                    attach_loading(&host, key, &webview);
                    attach_accel(&host, key, &controller);
                    attach_new_window(&host, key, &webview);
                    attach_zoom(&host, key, &controller);
                    attach_favicon(&host, key, &webview);
                    attach_downloads(&host, key, &webview);
                }
                let zoom = {
                    let mut st = host.state.lock();
                    if let Some(slot) = st.slots.get_mut(&key) {
                        slot.creating = false;
                        slot.controller = Some(controller.clone());
                        slot.webview = Some(webview);
                        slot.zoom
                    } else {
                        1.0
                    }
                };
                if browser {
                    let _ = unsafe { controller.SetZoomFactor(zoom) };
                }
                host.apply_document(key);
                host.apply_bounds(key);
                Ok(())
            },
        ));
        if let Err(e) = unsafe { env.CreateCoreWebView2Controller(parent, &handler) } {
            warn!(?e, instance = %key.instance, "CreateCoreWebView2Controller");
            if let Some(slot) = self.state.lock().slots.get_mut(&key) {
                slot.creating = false;
            }
        }
    }

    #[cfg(windows)]
    fn apply_document(&self, key: SlotKey) {
        use windows::core::HSTRING;

        let (webview, document) = {
            let mut st = self.state.lock();
            let Some(slot) = st.slots.get_mut(&key) else {
                return;
            };
            if slot.document == slot.last_applied {
                return;
            }
            let Some(webview) = slot.webview.clone() else {
                return;
            };
            let document = slot.document.clone();
            slot.last_applied = document.clone();
            (webview, document)
        };
        let result = unsafe {
            match &document {
                HtmlDocument::None => webview.Navigate(&HSTRING::from("about:blank")),
                HtmlDocument::Url(url) => webview.Navigate(&HSTRING::from(url.as_str())),
                HtmlDocument::Html(html) => webview.NavigateToString(&HSTRING::from(html.as_str())),
            }
        };
        if let Err(e) = result {
            warn!(?e, instance = %key.instance, "webview2 navigate");
            let mut st = self.state.lock();
            if let Some(slot) = st.slots.get_mut(&key) {
                slot.last_applied = HtmlDocument::None;
            }
        }
    }

    #[cfg(windows)]
    fn apply_bounds(&self, key: SlotKey) {
        use windows::Win32::Foundation::RECT;

        let (controller, webview, bounds, visible, browser, became_visible, loading, zoom) = {
            let mut st = self.state.lock();
            let Some(slot) = st.slots.get_mut(&key) else {
                return;
            };
            let Some(controller) = slot.controller.clone() else {
                return;
            };
            let became_visible = slot.visible && !slot.applied_visible;
            slot.applied_visible = slot.visible;
            (
                controller,
                slot.webview.clone(),
                slot.bounds,
                slot.visible,
                slot.browser,
                became_visible,
                slot.loading,
                slot.zoom,
            )
        };
        let rect = RECT {
            left: bounds.x,
            top: bounds.y,
            right: bounds.x + bounds.w.max(0),
            bottom: bounds.y + bounds.h.max(0),
        };
        if let Err(e) = unsafe { controller.SetBounds(rect) } {
            debug!(?e, instance = %key.instance, "webview2 SetBounds");
        }
        if let Err(e) = unsafe { controller.SetIsVisible(visible) } {
            debug!(?e, instance = %key.instance, "webview2 SetIsVisible");
        }
        let _ = unsafe { controller.NotifyParentWindowPositionChanged() };
        if browser && became_visible {
            let _ = unsafe { controller.SetZoomFactor(zoom) };
            if let Some(webview) = webview {
                push_nav(&self.nav, key, &webview, Some(loading), Some(zoom));
            }
        }
    }

    #[cfg(not(windows))]
    fn apply_bounds(&self, _key: SlotKey) {}
}

#[cfg(windows)]
fn attach_history(
    host: &HtmlWebViewHost,
    key: SlotKey,
    webview: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
) {
    use webview2_com::HistoryChangedEventHandler;

    let nav = host.nav.clone();
    let wv = webview.clone();
    let handler = HistoryChangedEventHandler::create(Box::new(move |_, _| {
        let mut can_go_back = windows::core::BOOL::from(false);
        let mut can_go_forward = windows::core::BOOL::from(false);
        let _ = unsafe { wv.CanGoBack(&mut can_go_back) };
        let _ = unsafe { wv.CanGoForward(&mut can_go_forward) };
        nav.lock().push(HtmlNavState {
            instance_id: key.instance,
            surface_id: key.surface,
            can_go_back: can_go_back.as_bool(),
            can_go_forward: can_go_forward.as_bool(),
            url: None,
            title: None,
            is_loading: None,
            zoom: None,
        });
        Ok(())
    }));
    let mut token = 0_i64;
    if let Err(e) = unsafe { webview.add_HistoryChanged(&handler, &mut token) } {
        debug!(?e, instance = %key.instance, "webview2 HistoryChanged");
    }
}

#[cfg(windows)]
fn attach_location(
    host: &HtmlWebViewHost,
    key: SlotKey,
    webview: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
) {
    use webview2_com::{DocumentTitleChangedEventHandler, SourceChangedEventHandler};

    let nav = host.nav.clone();
    let wv = webview.clone();
    let handler = SourceChangedEventHandler::create(Box::new(move |_, _| {
        push_nav(&nav, key, &wv, None, None);
        Ok(())
    }));
    let mut token = 0_i64;
    if let Err(e) = unsafe { webview.add_SourceChanged(&handler, &mut token) } {
        debug!(?e, instance = %key.instance, "webview2 SourceChanged");
    }

    let nav = host.nav.clone();
    let wv = webview.clone();
    let handler = DocumentTitleChangedEventHandler::create(Box::new(move |_, _| {
        push_nav(&nav, key, &wv, None, None);
        Ok(())
    }));
    let mut token = 0_i64;
    if let Err(e) = unsafe { webview.add_DocumentTitleChanged(&handler, &mut token) } {
        debug!(?e, instance = %key.instance, "webview2 DocumentTitleChanged");
    }
}

#[cfg(windows)]
fn attach_loading(
    host: &HtmlWebViewHost,
    key: SlotKey,
    webview: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
) {
    use webview2_com::{NavigationCompletedEventHandler, NavigationStartingEventHandler};

    let nav = host.nav.clone();
    let state = host.state.clone();
    let wv = webview.clone();
    let handler = NavigationStartingEventHandler::create(Box::new(move |_, _| {
        if let Some(slot) = state.lock().slots.get_mut(&key) {
            slot.loading = true;
        }
        push_nav(&nav, key, &wv, Some(true), None);
        Ok(())
    }));
    let mut token = 0_i64;
    if let Err(e) = unsafe { webview.add_NavigationStarting(&handler, &mut token) } {
        debug!(?e, instance = %key.instance, "webview2 NavigationStarting");
    }

    let nav = host.nav.clone();
    let state = host.state.clone();
    let wv = webview.clone();
    let handler = NavigationCompletedEventHandler::create(Box::new(move |_, _| {
        if let Some(slot) = state.lock().slots.get_mut(&key) {
            slot.loading = false;
        }
        push_nav(&nav, key, &wv, Some(false), None);
        Ok(())
    }));
    let mut token = 0_i64;
    if let Err(e) = unsafe { webview.add_NavigationCompleted(&handler, &mut token) } {
        debug!(?e, instance = %key.instance, "webview2 NavigationCompleted");
    }
}

#[cfg(windows)]
fn attach_accel(
    host: &HtmlWebViewHost,
    key: SlotKey,
    controller: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller,
) {
    use webview2_com::AcceleratorKeyPressedEventHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
        COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_MENU, VK_SHIFT};

    let chrome = host.chrome.clone();
    let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut kind = COREWEBVIEW2_KEY_EVENT_KIND(0);
        let _ = unsafe { args.KeyEventKind(&mut kind) };
        if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
            && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
        {
            return Ok(());
        }
        let mut vk = 0u32;
        let _ = unsafe { args.VirtualKey(&mut vk) };
        let ctrl = key_down(VK_CONTROL);
        let alt = key_down(VK_MENU);
        let shift = key_down(VK_SHIFT);
        let action = match (ctrl, alt, shift, vk) {
            (true, false, true, 0x54) => Some(BrowserChromeAction::ReopenClosed),
            (true, false, false, 0x54) => Some(BrowserChromeAction::NewTab),
            (true, false, false, 0x57) => Some(BrowserChromeAction::CloseTab),
            (true, false, false, 0x4C) => Some(BrowserChromeAction::FocusAddress),
            (true, false, false, 0x52) => Some(BrowserChromeAction::Reload),
            (true, false, false, 0x46) => Some(BrowserChromeAction::Find),
            (true, false, false, 0x44) => Some(BrowserChromeAction::Bookmark),
            (true, false, false, 0x4A) => Some(BrowserChromeAction::Downloads),
            (true, false, true, 0x09) => Some(BrowserChromeAction::PrevTab),
            (true, false, false, 0x09) => Some(BrowserChromeAction::NextTab),
            (true, false, false, 0x22) => Some(BrowserChromeAction::NextTab),
            (true, false, false, 0x21) => Some(BrowserChromeAction::PrevTab),
            (true, false, _, 0xBB) | (true, false, _, 0x6B) => Some(BrowserChromeAction::ZoomIn),
            (true, false, _, 0xBD) | (true, false, _, 0x6D) => Some(BrowserChromeAction::ZoomOut),
            (true, false, _, 0x30) | (true, false, _, 0x60) => Some(BrowserChromeAction::ZoomReset),
            (false, false, false, 0x74) => Some(BrowserChromeAction::Reload),
            (false, false, false, 0x1B) => Some(BrowserChromeAction::Stop),
            (false, true, false, 0x25) => Some(BrowserChromeAction::Back),
            (false, true, false, 0x27) => Some(BrowserChromeAction::Forward),
            (false, true, false, 0x24) => Some(BrowserChromeAction::Home),
            (true, false, false, vk) if (0x31..=0x39).contains(&vk) => {
                Some(BrowserChromeAction::SelectTab((vk - 0x30) as u8))
            }
            (true, false, false, vk) if (0x61..=0x69).contains(&vk) => {
                Some(BrowserChromeAction::SelectTab((vk - 0x60) as u8))
            }
            _ => None,
        };
        let Some(action) = action else {
            return Ok(());
        };
        let _ = unsafe { args.SetHandled(true) };
        chrome.lock().push(BrowserChromeEvent {
            instance_id: key.instance,
            action,
        });
        Ok(())
    }));
    let mut token = 0_i64;
    if let Err(e) = unsafe { controller.add_AcceleratorKeyPressed(&handler, &mut token) } {
        debug!(?e, instance = %key.instance, "webview2 AcceleratorKeyPressed");
    }
}

#[cfg(windows)]
fn attach_new_window(
    host: &HtmlWebViewHost,
    key: SlotKey,
    webview: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
) {
    use webview2_com::NewWindowRequestedEventHandler;

    let opens = host.opens.clone();
    let handler = NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let _ = unsafe { args.SetHandled(true) };
        let mut uri = windows::core::PWSTR::null();
        let _ = unsafe { args.Uri(&mut uri) };
        if let Some(url) = pwstr_to_string(uri) {
            opens.lock().push(BrowserOpenRequest {
                instance_id: key.instance,
                url,
            });
        }
        Ok(())
    }));
    let mut token = 0_i64;
    if let Err(e) = unsafe { webview.add_NewWindowRequested(&handler, &mut token) } {
        debug!(?e, instance = %key.instance, "webview2 NewWindowRequested");
    }
}

#[cfg(windows)]
fn attach_zoom(
    host: &HtmlWebViewHost,
    key: SlotKey,
    controller: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller,
) {
    use webview2_com::ZoomFactorChangedEventHandler;

    let nav = host.nav.clone();
    let state = host.state.clone();
    let ctl = controller.clone();
    let handler = ZoomFactorChangedEventHandler::create(Box::new(move |_, _| {
        let mut zoom = 1.0_f64;
        let _ = unsafe { ctl.ZoomFactor(&mut zoom) };
        let webview = {
            let mut st = state.lock();
            let Some(slot) = st.slots.get_mut(&key) else {
                return Ok(());
            };
            slot.zoom = zoom;
            slot.webview.clone()
        };
        if let Some(webview) = webview {
            push_nav(&nav, key, &webview, None, Some(zoom));
        }
        Ok(())
    }));
    let mut token = 0_i64;
    if let Err(e) = unsafe { controller.add_ZoomFactorChanged(&handler, &mut token) } {
        debug!(?e, instance = %key.instance, "webview2 ZoomFactorChanged");
    }
}

#[cfg(windows)]
fn attach_favicon(
    host: &HtmlWebViewHost,
    key: SlotKey,
    webview: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
) {
    use webview2_com::FaviconChangedEventHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_15;
    use windows::core::Interface;

    let Ok(wv15) = webview.cast::<ICoreWebView2_15>() else {
        return;
    };
    let favicons = host.favicons.clone();
    let wv_for_get = wv15.clone();
    request_favicon(wv15.clone(), favicons.clone(), key);
    let handler = FaviconChangedEventHandler::create(Box::new(move |_, _| {
        request_favicon(wv_for_get.clone(), favicons.clone(), key);
        Ok(())
    }));
    let mut token = 0_i64;
    if let Err(e) = unsafe { wv15.add_FaviconChanged(&handler, &mut token) } {
        debug!(?e, instance = %key.instance, "webview2 FaviconChanged");
    }
}

#[cfg(windows)]
fn request_favicon(
    wv15: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_15,
    favicons: Arc<Mutex<Vec<HtmlFaviconUpdate>>>,
    key: SlotKey,
) {
    use webview2_com::GetFaviconCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG;

    let handler = GetFaviconCompletedHandler::create(Box::new(move |error, stream| {
        if error.is_err() {
            return Ok(());
        }
        let Some(stream) = stream else {
            return Ok(());
        };
        if let Some(png) = read_istream(&stream) {
            if !png.is_empty() {
                favicons.lock().push(HtmlFaviconUpdate {
                    instance_id: key.instance,
                    surface_id: key.surface,
                    png,
                });
            }
        }
        Ok(())
    }));
    if let Err(e) = unsafe { wv15.GetFavicon(COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG, &handler) } {
        debug!(?e, instance = %key.instance, "webview2 GetFavicon");
    }
}

#[cfg(windows)]
fn read_istream(stream: &windows::Win32::System::Com::IStream) -> Option<Vec<u8>> {
    use std::ffi::c_void;

    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let mut read = 0u32;
        let hr = unsafe {
            stream.Read(
                buf.as_mut_ptr().cast::<c_void>(),
                buf.len() as u32,
                Some(&mut read as *mut u32),
            )
        };
        if !hr.is_ok() || read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
        if (read as usize) < buf.len() {
            break;
        }
        if out.len() > 256 * 1024 {
            break;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(windows)]
fn attach_downloads(
    host: &HtmlWebViewHost,
    key: SlotKey,
    webview: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
) {
    use orchid_widgets::builtin::browser::{
        unique_download_path, DOWNLOAD_COMPLETED, DOWNLOAD_FAILED, DOWNLOAD_IN_PROGRESS,
    };
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_4, COREWEBVIEW2_DOWNLOAD_STATE, COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED,
        COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED,
    };
    use webview2_com::{
        BytesReceivedChangedEventHandler, DownloadStartingEventHandler, StateChangedEventHandler,
    };
    use windows::core::{Interface, HSTRING};

    let Ok(wv4) = webview.cast::<ICoreWebView2_4>() else {
        return;
    };
    let events = host.downloads.clone();
    let ops = host.download_ops.clone();
    let handler = DownloadStartingEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let Ok(op) = (unsafe { args.DownloadOperation() }) else {
            return Ok(());
        };
        let mut suggested = windows::core::PWSTR::null();
        let _ = unsafe { args.ResultFilePath(&mut suggested) };
        let suggested = pwstr_to_string(suggested).unwrap_or_else(|| "download".to_string());
        let dir = default_download_dir();
        let _ = std::fs::create_dir_all(&dir);
        let dest = unique_download_path(&dir, &suggested);
        let path_s = dest.to_string_lossy().into_owned();
        let filename = dest
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("download")
            .to_string();
        let _ = unsafe { args.SetResultFilePath(&HSTRING::from(path_s.as_str())) };
        let _ = unsafe { args.SetHandled(true) };

        let dl_id = Uuid::new_v4();
        ops.lock().insert(dl_id, op.clone());
        push_download(
            &events,
            key.instance,
            dl_id,
            &filename,
            &path_s,
            &op,
            DOWNLOAD_IN_PROGRESS,
        );

        let events_b = events.clone();
        let filename_b = filename.clone();
        let path_b = path_s.clone();
        let op_b = op.clone();
        let bytes_handler = BytesReceivedChangedEventHandler::create(Box::new(move |_, _| {
            push_download(
                &events_b,
                key.instance,
                dl_id,
                &filename_b,
                &path_b,
                &op_b,
                DOWNLOAD_IN_PROGRESS,
            );
            Ok(())
        }));
        let mut token = 0_i64;
        let _ = unsafe { op.add_BytesReceivedChanged(&bytes_handler, &mut token) };

        let events_s = events.clone();
        let ops_s = ops.clone();
        let filename_s = filename;
        let path_st = path_s;
        let op_s = op.clone();
        let state_handler = StateChangedEventHandler::create(Box::new(move |_, _| {
            let mut st = COREWEBVIEW2_DOWNLOAD_STATE(0);
            let _ = unsafe { op_s.State(&mut st) };
            let mapped = if st == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED {
                DOWNLOAD_COMPLETED
            } else if st == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
                DOWNLOAD_FAILED
            } else {
                DOWNLOAD_IN_PROGRESS
            };
            push_download(
                &events_s,
                key.instance,
                dl_id,
                &filename_s,
                &path_st,
                &op_s,
                mapped,
            );
            if mapped != DOWNLOAD_IN_PROGRESS {
                ops_s.lock().remove(&dl_id);
            }
            Ok(())
        }));
        let mut token = 0_i64;
        let _ = unsafe { op.add_StateChanged(&state_handler, &mut token) };
        Ok(())
    }));
    let mut token = 0_i64;
    if let Err(e) = unsafe { wv4.add_DownloadStarting(&handler, &mut token) } {
        debug!(?e, instance = %key.instance, "webview2 DownloadStarting");
    }
}

#[cfg(windows)]
fn push_download(
    events: &Arc<Mutex<Vec<BrowserDownloadEvent>>>,
    instance_id: Uuid,
    id: Uuid,
    filename: &str,
    path: &str,
    op: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2DownloadOperation,
    state: u8,
) {
    let mut bytes = 0i64;
    let mut total = 0i64;
    let _ = unsafe { op.BytesReceived(&mut bytes) };
    let _ = unsafe { op.TotalBytesToReceive(&mut total) };
    events.lock().push(BrowserDownloadEvent {
        instance_id,
        id,
        filename: filename.to_string(),
        path: path.to_string(),
        bytes: bytes.max(0) as u64,
        total: total.max(0) as u64,
        state,
    });
}

#[cfg(windows)]
fn default_download_dir() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|u| u.download_dir().map(Path::to_path_buf))
        .unwrap_or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Downloads")
        })
}

#[cfg(windows)]
fn key_down(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    unsafe { GetAsyncKeyState(i32::from(vk.0)) as u16 & 0x8000 != 0 }
}

#[cfg(windows)]
fn push_nav(
    nav: &Arc<Mutex<Vec<HtmlNavState>>>,
    key: SlotKey,
    wv: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
    is_loading: Option<bool>,
    zoom: Option<f64>,
) {
    let mut can_go_back = windows::core::BOOL::from(false);
    let mut can_go_forward = windows::core::BOOL::from(false);
    let _ = unsafe { wv.CanGoBack(&mut can_go_back) };
    let _ = unsafe { wv.CanGoForward(&mut can_go_forward) };
    let mut uri = windows::core::PWSTR::null();
    let _ = unsafe { wv.Source(&mut uri) };
    let mut title_raw = windows::core::PWSTR::null();
    let _ = unsafe { wv.DocumentTitle(&mut title_raw) };
    nav.lock().push(HtmlNavState {
        instance_id: key.instance,
        surface_id: key.surface,
        can_go_back: can_go_back.as_bool(),
        can_go_forward: can_go_forward.as_bool(),
        url: pwstr_to_string(uri),
        title: pwstr_to_string(title_raw),
        is_loading,
        zoom,
    });
}

#[cfg(windows)]
fn pwstr_to_string(p: windows::core::PWSTR) -> Option<String> {
    use windows::Win32::System::Com::CoTaskMemFree;

    if p.is_null() {
        return None;
    }
    let s = unsafe { p.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(p.0.cast())) };
    s.filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_url_from_drive_path() {
        let url = file_url_from_path(Path::new(r"C:\temp\page.html")).expect("url");
        assert!(url.starts_with("file:"), "{url}");
        assert!(url.contains("page.html"), "{url}");
    }
}
