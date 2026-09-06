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
}

/// One chrome shortcut for the browser widget (UI-thread drain).
#[derive(Debug, Clone, Copy)]
pub(crate) struct BrowserChromeEvent {
    pub instance_id: Uuid,
    pub action: BrowserChromeAction,
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
                push_nav(&self.nav, key, &webview, Some(false));
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
                }
                {
                    let mut st = host.state.lock();
                    if let Some(slot) = st.slots.get_mut(&key) {
                        slot.creating = false;
                        slot.controller = Some(controller);
                        slot.webview = Some(webview);
                    }
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

        let (controller, webview, bounds, visible, browser, became_visible, loading) = {
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
            if let Some(webview) = webview {
                push_nav(&self.nav, key, &webview, Some(loading));
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
        push_nav(&nav, key, &wv, None);
        Ok(())
    }));
    let mut token = 0_i64;
    if let Err(e) = unsafe { webview.add_SourceChanged(&handler, &mut token) } {
        debug!(?e, instance = %key.instance, "webview2 SourceChanged");
    }

    let nav = host.nav.clone();
    let wv = webview.clone();
    let handler = DocumentTitleChangedEventHandler::create(Box::new(move |_, _| {
        push_nav(&nav, key, &wv, None);
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
        push_nav(&nav, key, &wv, Some(true));
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
        push_nav(&nav, key, &wv, Some(false));
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
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_MENU};

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
        let action = match (ctrl, alt, vk) {
            (true, false, 0x54) => Some(BrowserChromeAction::NewTab),
            (true, false, 0x57) => Some(BrowserChromeAction::CloseTab),
            (true, false, 0x4C) => Some(BrowserChromeAction::FocusAddress),
            (true, false, 0x52) => Some(BrowserChromeAction::Reload),
            (true, false, 0x46) => Some(BrowserChromeAction::Find),
            (true, false, 0x44) => Some(BrowserChromeAction::Bookmark),
            (false, false, 0x74) => Some(BrowserChromeAction::Reload),
            (false, false, 0x1B) => Some(BrowserChromeAction::Stop),
            (false, true, 0x25) => Some(BrowserChromeAction::Back),
            (false, true, 0x27) => Some(BrowserChromeAction::Forward),
            (false, true, 0x24) => Some(BrowserChromeAction::Home),
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
