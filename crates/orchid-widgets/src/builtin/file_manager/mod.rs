//! File-manager widget.

pub mod clipboard;
pub mod config;
pub mod context_menu;
pub mod navigation;
pub mod selection;
pub mod state;
pub mod view_mode;
pub mod virtual_folders;

use std::sync::Arc;
use std::sync::LazyLock;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use tokio::sync::Mutex;
use tracing::warn;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::error::WidgetError;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{
    EntryPayload, FileManagerPayload, FmViewMode, PanePayload, TabPayload,
};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory};
use orchid_storage::{LifecycleState, WidgetSize};

pub use clipboard::{ClipboardOperation, FileClipboard};
pub use config::{
    ClickBehavior, FileManagerConfig, SortBy, ThumbnailSize as FmThumbnailSize, ViewMode,
};
pub use context_menu::{build_for_selection, ContextMenuInputs, ContextMenuItem};
pub use navigation::{BreadcrumbSegment, NavigationResult, Navigator};
pub use selection::SelectionModel;
pub use state::{ActivePane, FileManagerState, PaneState, TabState};
pub use view_mode::{config_for_mode, ViewModeConfig};
pub use virtual_folders::{is_virtual, sidebar_catalog, VirtualFolder};

/// Selection mutation mode for UI interactions.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    Single,
    Toggle,
    Range,
}

/// Result of a context-menu action dispatch.
#[allow(missing_docs)]
#[derive(Debug, Clone)]
pub enum ActionOutcome {
    Done,
    NeedsConfirmation {
        message: String,
        action_id: String,
        paths: Vec<String>,
    },
    NeedsRename {
        path: String,
        current_name: String,
    },
    OpenInViewer {
        path: String,
    },
}

/// Stable type id.
pub const TYPE_ID: &str = "file-manager";

/// Live file-manager widget cores keyed by instance id (for UI callbacks).
static FM_LIVE: LazyLock<DashMap<Uuid, Arc<FileManagerInner>>> = LazyLock::new(DashMap::new);

/// Dependencies shared across every file-manager instance.
#[derive(Clone)]
pub struct FileManagerDeps {
    /// Filesystem provider registry.
    pub registry: Arc<orchid_fs::FsProviderRegistry>,
    /// Shared file clipboard (copy / cut across widgets).
    pub clipboard: Arc<FileClipboard>,
    /// Tag manager — used by virtual folders and context-menu probes.
    pub tag_manager: Arc<orchid_fs::TagManager>,
    /// Thumbnail service (for image previews in Icons / Gallery modes).
    pub thumbnails: Arc<orchid_viewers::ThumbnailService>,
}

impl std::fmt::Debug for FileManagerDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileManagerDeps").finish_non_exhaustive()
    }
}

/// File manager widget.
pub struct FileManagerWidget {
    inner: Arc<FileManagerInner>,
}

impl std::fmt::Debug for FileManagerWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileManagerWidget")
            .field("instance_id", &self.inner.instance_id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct FileManagerInner {
    instance_id: Uuid,
    deps: FileManagerDeps,
    navigator: Arc<Navigator>,
    state: Mutex<FileManagerState>,
    config: RwLock<FileManagerConfig>,
    /// Entries per tab id. Keeps dual-pane tabs independent.
    entries_by_tab: RwLock<std::collections::HashMap<Uuid, Vec<orchid_fs::FsEntry>>>,
    /// UI-level "virtual metadata" until providers persist it.
    starred: RwLock<std::collections::HashSet<String>>,
    encrypted: RwLock<std::collections::HashSet<String>>,
    recent: RwLock<std::collections::VecDeque<String>>,
    bus: Arc<orchid_core::EventBus>,
}

impl FileManagerWidget {
    /// Build a widget rooted at `initial_path`.
    pub fn new(
        instance_id: Uuid,
        deps: FileManagerDeps,
        bus: Arc<orchid_core::EventBus>,
        initial_path: orchid_fs::FsPath,
    ) -> Self {
        let config = FileManagerConfig::default();
        let state = FileManagerState::single_pane(
            initial_path,
            config.default_view_mode,
            config.sort_by,
        );
        let navigator = Arc::new(Navigator::new(deps.registry.clone()));
        Self {
            inner: Arc::new(FileManagerInner {
                instance_id,
                deps,
                navigator,
                state: Mutex::new(state),
                config: RwLock::new(config),
                entries_by_tab: RwLock::new(std::collections::HashMap::new()),
                starred: RwLock::new(std::collections::HashSet::new()),
                encrypted: RwLock::new(std::collections::HashSet::new()),
                recent: RwLock::new(std::collections::VecDeque::new()),
                bus,
            }),
        }
    }

    /// Refresh the active tab's entry list.
    pub async fn refresh(&self) {
        let show_hidden = self.inner.config.read().show_hidden;
        let (left, right) = {
            let state = self.inner.state.lock().await.clone();
            let left = state.left_pane.active_tab().clone();
            let right = state
                .right_pane
                .as_ref()
                .map(|p| p.active_tab().clone());
            (left, right)
        };

        self.inner.refresh_tab(&left, show_hidden).await;
        if let Some(rt) = right {
            self.inner.refresh_tab(&rt, show_hidden).await;
        }
        self.inner.publish_refresh();
    }

    /// Navigate the active pane's tab to `path`.
    pub async fn navigate(&self, path: orchid_fs::FsPath) {
        {
            let mut state = self.inner.state.lock().await;
            state.active_tab_mut().navigate_to(path);
        }
        self.refresh().await
    }

    /// Back one step in history.
    pub async fn go_back(&self) {
        let changed = {
            let mut state = self.inner.state.lock().await;
            state.active_tab_mut().back()
        };
        if changed {
            self.refresh().await;
        }
    }

    /// Forward one step in history.
    pub async fn go_forward(&self) {
        let changed = {
            let mut state = self.inner.state.lock().await;
            state.active_tab_mut().forward()
        };
        if changed {
            self.refresh().await;
        }
    }

    /// Change the current tab's view mode.
    pub fn set_view_mode(&self, mode: ViewMode) {
        // view-mode change doesn't require re-listing.
        // We keep it sync but still publish snapshot updated.
        {
            // state is async; keep a best-effort try_lock by spawning.
            let inner = Arc::clone(&self.inner);
            tokio::spawn(async move {
                let mut state = inner.state.lock().await;
                state.active_tab_mut().view_mode = mode;
                inner.publish_refresh();
            });
        }
    }

    /// Shared clipboard accessor.
    #[must_use]
    pub fn clipboard(&self) -> Arc<FileClipboard> {
        self.inner.deps.clipboard.clone()
    }
}

#[async_trait]
impl Widget for FileManagerWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }
    fn instance_id(&self) -> Uuid {
        self.inner.instance_id
    }
    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        FM_LIVE.insert(self.inner.instance_id, Arc::clone(&self.inner));
        self.refresh().await;
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
        FM_LIVE.remove(&self.inner.instance_id);
        Ok(())
    }
    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }
    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let config = self.inner.config.read().clone();
        let state = self.inner.state.blocking_lock().clone();
        let entries_map = self.inner.entries_by_tab.read().clone();
        let tab = state.active_tab();
        let tab_entries = entries_map.get(&tab.id).cloned().unwrap_or_default();
        let tab_payload = build_tab_payload(tab, &tab_entries, &config, &*self.inner);
        let pane = PanePayload {
            tabs: vec![tab_payload],
            active_tab: 0,
        };
        let dual_pane = config.dual_pane;
        let mut panes = vec![pane.clone()];
        if dual_pane {
            if let Some(right) = &state.right_pane {
                let right_tab = right.active_tab();
                panes.push(PanePayload {
                    tabs: vec![build_tab_payload(
                        right_tab,
                        entries_map.get(&right_tab.id).map(Vec::as_slice).unwrap_or(&[]),
                        &config,
                        &*self.inner,
                    )],
                    active_tab: 0,
                });
            }
        }
        let active_pane = match state.active_pane {
            ActivePane::Left => 0,
            ActivePane::Right => 1,
        };
        let clipboard_indicator = match self.inner.deps.clipboard.operation() {
            ClipboardOperation::None => None,
            op => Some(format!(
                "{} {} ready to paste",
                self.inner.deps.clipboard.len(),
                if op == ClipboardOperation::Cut { "entries (cut)" } else { "entries" }
            )),
        };

        Some(WidgetSnapshot {
            instance_id: self.inner.instance_id,
            widget_type: TYPE_ID,
            title: tab.path.as_str().to_string(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::FileManager(FileManagerPayload {
                panes,
                active_pane,
                dual_pane,
                clipboard_indicator,
            }),
        })
    }
    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        state_codec::save_state(&*self.inner.config.read())
    }
    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let cfg: FileManagerConfig = state_codec::restore_state(bytes)?;
        *self.inner.config.write() = cfg;
        Ok(())
    }
    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Medium),
            max_size: None,
            preferred_size: Some(WidgetSize::ExtraLarge),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: true,
        }
    }
}

impl FileManagerInner {
    fn publish_refresh(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    async fn refresh_tab(&self, tab: &TabState, show_hidden: bool) {
        let path = tab.path.clone();

        if is_virtual(&path) {
            let entries = self.list_virtual(&path).await;
            self.entries_by_tab.write().insert(tab.id, entries);
            return;
        }

        let result = self.navigator.navigate(&path, show_hidden).await;
        let mut entries = result.entries;

        // Apply virtual metadata overlays (star/encrypt) until provider persistence lands.
        let starred = self.starred.read().clone();
        let encrypted = self.encrypted.read().clone();
        for e in entries.iter_mut() {
            if starred.contains(e.path.as_str()) {
                e.metadata.extended.starred = true;
            }
            if encrypted.contains(e.path.as_str()) {
                e.metadata.extended.is_encrypted = true;
            }
        }

        self.entries_by_tab.write().insert(tab.id, entries);
    }

    async fn list_virtual(&self, path: &orchid_fs::FsPath) -> Vec<orchid_fs::FsEntry> {
        // MVP: support `virtual:recent` and `virtual:starred` lists from overlays.
        // Everything else yields empty.
        let raw = path.as_str();
        if raw == "virtual:recent" {
            return self
                .recent
                .read()
                .iter()
                .take(50)
                .filter_map(|p| orchid_fs::FsPath::new(p).ok())
                .map(|p| orchid_fs::FsEntry {
                    name: p.file_name().map(String::from).unwrap_or_default(),
                    metadata: orchid_fs::FsMetadata {
                        kind: orchid_fs::FsEntryKind::File,
                        size: 0,
                        created: None,
                        modified: None,
                        accessed: None,
                        readonly: false,
                        hidden: false,
                        system: false,
                        mime: None,
                        extended: orchid_fs::ExtendedAttributes::default(),
                    },
                    path: p,
                })
                .collect();
        }
        if raw == "virtual:starred" {
            return self
                .starred
                .read()
                .iter()
                .take(200)
                .filter_map(|p| orchid_fs::FsPath::new(p).ok())
                .map(|p| orchid_fs::FsEntry {
                    name: p.file_name().map(String::from).unwrap_or_default(),
                    metadata: orchid_fs::FsMetadata {
                        kind: orchid_fs::FsEntryKind::File,
                        size: 0,
                        created: None,
                        modified: None,
                        accessed: None,
                        readonly: false,
                        hidden: false,
                        system: false,
                        mime: None,
                        extended: orchid_fs::ExtendedAttributes::default(),
                    },
                    path: p,
                })
                .collect();
        }
        Vec::new()
    }
}

fn build_tab_payload(
    tab: &TabState,
    entries: &[orchid_fs::FsEntry],
    _config: &FileManagerConfig,
    inner: &FileManagerInner,
) -> TabPayload {
    let crumbs = inner.navigator.breadcrumbs_for(&tab.path);
    let breadcrumbs: Vec<(String, String)> = crumbs
        .into_iter()
        .map(|c| (c.path.as_str().to_string(), c.display_name))
        .collect();

    let quick = tab.quick_filter.trim();
    let entries_filtered: Vec<&orchid_fs::FsEntry> = if quick.is_empty() {
        entries.iter().collect()
    } else {
        let q = quick.to_lowercase();
        entries
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&q))
            .collect()
    };

    let entry_payloads: Vec<EntryPayload> = entries_filtered
        .into_iter()
        .map(|e| EntryPayload {
            path: e.path.as_str().to_string(),
            name: e.name.clone(),
            is_dir: matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory),
            size_text: format_size(e.metadata.size),
            modified_text: e
                .metadata
                .modified
                .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default(),
            type_text: classify(&e.name, e.metadata.mime.as_deref(), matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory)),
            icon: if matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory) { "folder".into() } else { "file".into() },
            has_thumbnail: false,
            thumbnail_key: None,
            is_selected: tab.selection.is_selected(e.path.as_str()),
            is_hidden: e.metadata.hidden,
            is_encrypted: e.metadata.extended.is_encrypted,
            is_managed: e.metadata.extended.is_managed,
            is_starred: e.metadata.extended.starred,
            color_label: None,
            tags: e.metadata.extended.tags.clone(),
        })
        .collect();
    let selection_count = tab.selection.count() as u32;
    let status_text = format!(
        "{} items, {} selected",
        entry_payloads.len(),
        selection_count
    );
    TabPayload {
        tab_id: tab.id.to_string(),
        path_display: tab.path.as_str().to_string(),
        breadcrumbs,
        can_go_back: !tab.history_back.is_empty(),
        can_go_forward: !tab.history_forward.is_empty(),
        view_mode: to_payload_mode(tab.view_mode),
        entries: entry_payloads,
        selection_count,
        status_text,
        quick_filter: tab.quick_filter.clone(),
        is_loading: false,
        error: None,
    }
}

fn to_payload_mode(mode: ViewMode) -> FmViewMode {
    match mode {
        ViewMode::Icons => FmViewMode::Icons,
        ViewMode::List => FmViewMode::List,
        ViewMode::Details => FmViewMode::Details,
        ViewMode::Gallery => FmViewMode::Gallery,
    }
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = bytes as f64;
    if f >= GB {
        format!("{:.1} GB", f / GB)
    } else if f >= MB {
        format!("{:.1} MB", f / MB)
    } else if f >= KB {
        format!("{:.0} KB", f / KB)
    } else {
        format!("{bytes} B")
    }
}

fn classify(name: &str, mime: Option<&str>, is_dir: bool) -> String {
    if is_dir {
        return "Folder".into();
    }
    if let Some(m) = mime {
        return m.to_string();
    }
    name.rsplit('.')
        .next()
        .map(|ext| format!("{} file", ext.to_uppercase()))
        .unwrap_or_else(|| "File".into())
}

/// Descriptor with a default initial path of the user's home directory.
#[must_use]
pub fn descriptor(deps: FileManagerDeps) -> WidgetDescriptor {
    let default_path = default_initial_path();
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, _bytes| {
        Ok(Box::new(FileManagerWidget::new(
            ctx.instance_id,
            deps.clone(),
            ctx.bus.clone(),
            default_path.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-fm-name",
        description_key: "widget-fm-desc",
        icon_name: "file-manager",
        category: WidgetCategory::Productivity,
        default_size: WidgetSize::ExtraLarge,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}

fn default_initial_path() -> orchid_fs::FsPath {
    if let Some(home) = dirs_home() {
        if let Ok(p) = orchid_fs::FsPath::from_local(&home) {
            return p;
        }
    }
    orchid_fs::FsPath::new("local:/").unwrap_or_else(|_| {
        // unreachable in practice; fall back to a known-good absolute path.
        orchid_fs::FsPath::new("local:c:/").expect("constant path parses")
    })
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var("USERPROFILE")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var("HOME").ok().map(std::path::PathBuf::from))
}

fn live_inner(instance_id: Uuid) -> WidgetResult<Arc<FileManagerInner>> {
    FM_LIVE
        .get(&instance_id)
        .map(|e| Arc::clone(e.value()))
        .ok_or_else(|| WidgetError::InvalidStateForOperation("file-manager widget not live".into()))
}

/// Navigate the given `pane` (0 left, 1 right) to `path`.
pub async fn navigate(instance_id: Uuid, pane: u8, path: orchid_fs::FsPath) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut state = inner.state.lock().await;
        if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut().navigate_to(path);
            } else {
                state.left_pane.active_tab_mut().navigate_to(path);
            }
        } else {
            state.left_pane.active_tab_mut().navigate_to(path);
        }
    }
    inner.publish_refresh();
    Ok(())
}

/// Back in history for `pane`.
pub async fn navigate_back(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let changed = {
        let mut state = inner.state.lock().await;
        if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut().back()
            } else {
                state.left_pane.active_tab_mut().back()
            }
        } else {
            state.left_pane.active_tab_mut().back()
        }
    };
    if changed {
        inner.publish_refresh();
    }
    Ok(())
}

/// Forward in history for `pane`.
pub async fn navigate_forward(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let changed = {
        let mut state = inner.state.lock().await;
        if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut().forward()
            } else {
                state.left_pane.active_tab_mut().forward()
            }
        } else {
            state.left_pane.active_tab_mut().forward()
        }
    };
    if changed {
        inner.publish_refresh();
    }
    Ok(())
}

/// Up to parent folder for `pane`.
pub async fn navigate_up(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let parent = {
        let state = inner.state.lock().await;
        let tab = if pane == 1 {
            state
                .right_pane
                .as_ref()
                .unwrap_or(&state.left_pane)
                .active_tab()
        } else {
            state.left_pane.active_tab()
        };
        tab.path.parent()
    };
    if let Some(p) = parent {
        navigate(instance_id, pane, p).await?;
    }
    Ok(())
}

/// Switch to tab by string id.
pub async fn switch_to_tab(instance_id: Uuid, pane: u8, tab_id: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let want = Uuid::parse_str(tab_id).map_err(|_| {
        WidgetError::InvalidStateForOperation("invalid tab id".into())
    })?;
    {
        let mut state = inner.state.lock().await;
        if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                if let Some(idx) = r.tabs.iter().position(|t| t.id == want) {
                    r.active_tab = idx;
                }
            } else if let Some(idx) = state.left_pane.tabs.iter().position(|t| t.id == want) {
                state.left_pane.active_tab = idx;
            }
        } else if let Some(idx) = state.left_pane.tabs.iter().position(|t| t.id == want) {
            state.left_pane.active_tab = idx;
        }
    }
    inner.publish_refresh();
    Ok(())
}

/// Close tab by id.
pub async fn close_tab(instance_id: Uuid, pane: u8, tab_id: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let want = Uuid::parse_str(tab_id).map_err(|_| {
        WidgetError::InvalidStateForOperation("invalid tab id".into())
    })?;
    {
        let mut state = inner.state.lock().await;
        let target = if pane == 1 {
            state.right_pane.as_mut()
        } else {
            None
        };
        if let Some(r) = target {
            if r.tabs.len() <= 1 {
                return Ok(());
            }
            if let Some(idx) = r.tabs.iter().position(|t| t.id == want) {
                r.tabs.remove(idx);
                r.active_tab = r.active_tab.min(r.tabs.len().saturating_sub(1));
            }
        } else {
            if state.left_pane.tabs.len() <= 1 {
                return Ok(());
            }
            if let Some(idx) = state.left_pane.tabs.iter().position(|t| t.id == want) {
                state.left_pane.tabs.remove(idx);
                state.left_pane.active_tab =
                    state.left_pane.active_tab.min(state.left_pane.tabs.len().saturating_sub(1));
            }
        }
    }
    inner.publish_refresh();
    Ok(())
}

/// Create new tab in pane (cloned from current path).
pub async fn new_tab(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let cfg = inner.config.read().clone();
        let mut state = inner.state.lock().await;
        if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                let path = r.active_tab().path.clone();
                r.tabs.push(TabState::new(path, cfg.default_view_mode, cfg.sort_by));
                r.active_tab = r.tabs.len().saturating_sub(1);
            } else {
                let path = state.left_pane.active_tab().path.clone();
                state
                    .left_pane
                    .tabs
                    .push(TabState::new(path, cfg.default_view_mode, cfg.sort_by));
                state.left_pane.active_tab = state.left_pane.tabs.len().saturating_sub(1);
            }
        } else {
            let path = state.left_pane.active_tab().path.clone();
            state
                .left_pane
                .tabs
                .push(TabState::new(path, cfg.default_view_mode, cfg.sort_by));
            state.left_pane.active_tab = state.left_pane.tabs.len().saturating_sub(1);
        }
    }
    inner.publish_refresh();
    Ok(())
}

/// Switch active pane focus.
pub async fn switch_active_pane(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut state = inner.state.lock().await;
        state.active_pane = if pane == 1 {
            ActivePane::Right
        } else {
            ActivePane::Left
        };
    }
    inner.publish_refresh();
    Ok(())
}

/// Toggle dual-pane configuration.
pub async fn toggle_dual_pane(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let enabled = {
        let mut cfg = inner.config.write();
        cfg.dual_pane = !cfg.dual_pane;
        cfg.dual_pane
    };
    {
        let cfg = inner.config.read().clone();
        let mut state = inner.state.lock().await;
        if enabled && state.right_pane.is_none() {
            let path = state.left_pane.active_tab().path.clone();
            state.right_pane = Some(PaneState::with_single_tab(TabState::new(
                path,
                cfg.default_view_mode,
                cfg.sort_by,
            )));
        }
        if !enabled {
            state.right_pane = None;
            state.active_pane = ActivePane::Left;
        }
    }
    inner.publish_refresh();
    Ok(())
}

/// Cycle view mode for the active tab in `pane`.
pub async fn cycle_view_mode(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut state = inner.state.lock().await;
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut()
            } else {
                state.left_pane.active_tab_mut()
            }
        } else {
            state.left_pane.active_tab_mut()
        };
        tab.view_mode = match tab.view_mode {
            ViewMode::Icons => ViewMode::List,
            ViewMode::List => ViewMode::Details,
            ViewMode::Details => ViewMode::Gallery,
            ViewMode::Gallery => ViewMode::Icons,
        };
    }
    inner.publish_refresh();
    Ok(())
}

/// Update quick filter text.
pub async fn set_quick_filter(instance_id: Uuid, pane: u8, q: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut state = inner.state.lock().await;
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut()
            } else {
                state.left_pane.active_tab_mut()
            }
        } else {
            state.left_pane.active_tab_mut()
        };
        tab.quick_filter = q;
    }
    inner.publish_refresh();
    Ok(())
}

/// Select entry inside the active tab for `pane`.
pub async fn select_entry(
    instance_id: Uuid,
    pane: u8,
    path: &str,
    mode: SelectionMode,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let (tab_id, ordered): (Uuid, Vec<String>) = {
        let state = inner.state.lock().await;
        let tab = if pane == 1 {
            state
                .right_pane
                .as_ref()
                .unwrap_or(&state.left_pane)
                .active_tab()
        } else {
            state.left_pane.active_tab()
        };
        let entries = inner
            .entries_by_tab
            .read()
            .get(&tab.id)
            .cloned()
            .unwrap_or_default();
        (
            tab.id,
            entries.into_iter().map(|e| e.path.as_str().to_string()).collect(),
        )
    };
    {
        let mut state = inner.state.lock().await;
        if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                if let Some(t) = r.tabs.iter_mut().find(|t| t.id == tab_id) {
                    match mode {
                        SelectionMode::Single => t.selection.select_single(path),
                        SelectionMode::Toggle => t.selection.toggle(path),
                        SelectionMode::Range => t.selection.extend_to(&ordered, path),
                    }
                }
            } else if let Some(t) = state.left_pane.tabs.iter_mut().find(|t| t.id == tab_id) {
                match mode {
                    SelectionMode::Single => t.selection.select_single(path),
                    SelectionMode::Toggle => t.selection.toggle(path),
                    SelectionMode::Range => t.selection.extend_to(&ordered, path),
                }
            }
        } else if let Some(t) = state.left_pane.tabs.iter_mut().find(|t| t.id == tab_id) {
            match mode {
                SelectionMode::Single => t.selection.select_single(path),
                SelectionMode::Toggle => t.selection.toggle(path),
                SelectionMode::Range => t.selection.extend_to(&ordered, path),
            }
        }
    }
    inner.publish_refresh();
    Ok(())
}

/// Run a context-menu action against `target_paths`.
pub async fn run_action(
    instance_id: Uuid,
    action_id: &str,
    target_paths: Vec<String>,
) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    match action_id {
        "viewer.open" => {
            let Some(p) = target_paths.first() else {
                return Ok(ActionOutcome::Done);
            };
            return Ok(ActionOutcome::OpenInViewer { path: p.clone() });
        }
        "fs.copy" => {
            let paths: Vec<orchid_fs::FsPath> = target_paths
                .iter()
                .filter_map(|p| orchid_fs::FsPath::new(p).ok())
                .collect();
            inner.deps.clipboard.copy(paths);
        }
        "fs.cut" => {
            let paths: Vec<orchid_fs::FsPath> = target_paths
                .iter()
                .filter_map(|p| orchid_fs::FsPath::new(p).ok())
                .collect();
            inner.deps.clipboard.cut(paths);
        }
        "fs.paste" => {
            // MVP: real copy/move is deferred until provider operations are unified.
            if let Ok(p) = orchid_fs::FsPath::new("local:/") {
                let _ = inner.deps.clipboard.paste(&p);
            }
        }
        "fs.rename" => {
            if target_paths.len() == 1 {
                let p = target_paths[0].clone();
                let current_name = p
                    .rsplit('/')
                    .next()
                    .unwrap_or(p.as_str())
                    .to_string();
                return Ok(ActionOutcome::NeedsRename {
                    path: p,
                    current_name,
                });
            }
        }
        "fs.delete" => {
            let cfg = inner.config.read().clone();
            if cfg.confirm_delete && !target_paths.is_empty() {
                return Ok(ActionOutcome::NeedsConfirmation {
                    message: format!("Delete {} items?", target_paths.len()),
                    action_id: action_id.to_string(),
                    paths: target_paths,
                });
            }
        }
        "fs.star" => {
            let mut set = inner.starred.write();
            for p in &target_paths {
                set.insert(p.clone());
            }
        }
        "fs.unstar" => {
            let mut set = inner.starred.write();
            for p in &target_paths {
                set.remove(p);
            }
        }
        "fs.encrypt" => {
            let mut set = inner.encrypted.write();
            for p in &target_paths {
                set.insert(p.clone());
            }
        }
        "fs.decrypt" => {
            let mut set = inner.encrypted.write();
            for p in &target_paths {
                set.remove(p);
            }
        }
        _ => {
            // Unknown actions: treat as done for MVP.
        }
    }
    inner.publish_refresh();
    Ok(ActionOutcome::Done)
}

/// Commit rename (MVP: metadata-only; filesystem rename deferred).
pub async fn rename(instance_id: Uuid, _old_path: &str, _new_name: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.publish_refresh();
    Ok(())
}

/// Navigate to a virtual folder by sidebar id.
pub async fn navigate_virtual(instance_id: Uuid, pane: u8, virtual_id: &str) -> WidgetResult<()> {
    // Map the UI ids from the sidebar to internal virtual paths.
    let path = match virtual_id {
        "fav:recent" => orchid_fs::FsPath::new("virtual:recent").ok(),
        "fav:starred" => orchid_fs::FsPath::new("virtual:starred").ok(),
        "cat:images" => orchid_fs::FsPath::new("virtual:categories/images").ok(),
        "cat:documents" => orchid_fs::FsPath::new("virtual:categories/documents").ok(),
        "cat:video" => orchid_fs::FsPath::new("virtual:categories/video").ok(),
        "cat:audio" => orchid_fs::FsPath::new("virtual:categories/audio").ok(),
        "cat:archives" => orchid_fs::FsPath::new("virtual:categories/archives").ok(),
        other => {
            warn!(id = %other, "unknown virtual folder id");
            None
        }
    };
    if let Some(p) = path {
        navigate(instance_id, pane, p).await?;
    }
    Ok(())
}
