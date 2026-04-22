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

use async_trait::async_trait;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
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

/// Stable type id.
pub const TYPE_ID: &str = "file-manager";

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
    instance_id: Uuid,
    deps: FileManagerDeps,
    navigator: Arc<Navigator>,
    state: RwLock<FileManagerState>,
    config: RwLock<FileManagerConfig>,
    current_entries: RwLock<Vec<orchid_fs::FsEntry>>,
    bus: Arc<orchid_core::EventBus>,
}

impl std::fmt::Debug for FileManagerWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileManagerWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
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
            instance_id,
            deps,
            navigator,
            state: RwLock::new(state),
            config: RwLock::new(config),
            current_entries: RwLock::new(Vec::new()),
            bus,
        }
    }

    /// Refresh the active tab's entry list.
    pub async fn refresh(&self) {
        let (path, show_hidden) = {
            let state = self.state.read();
            (state.active_tab().path.clone(), self.config.read().show_hidden)
        };
        let result = self.navigator.navigate(&path, show_hidden).await;
        *self.current_entries.write() = result.entries;
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    /// Navigate the active pane's tab to `path`.
    pub async fn navigate(&self, path: orchid_fs::FsPath) {
        {
            let mut state = self.state.write();
            state.active_tab_mut().navigate_to(path);
        }
        self.refresh().await;
    }

    /// Back one step in history.
    pub async fn go_back(&self) {
        let changed = self.state.write().active_tab_mut().back();
        if changed {
            self.refresh().await;
        }
    }

    /// Forward one step in history.
    pub async fn go_forward(&self) {
        let changed = self.state.write().active_tab_mut().forward();
        if changed {
            self.refresh().await;
        }
    }

    /// Change the current tab's view mode.
    pub fn set_view_mode(&self, mode: ViewMode) {
        self.state.write().active_tab_mut().view_mode = mode;
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    /// Shared clipboard accessor.
    #[must_use]
    pub fn clipboard(&self) -> Arc<FileClipboard> {
        self.deps.clipboard.clone()
    }
}

#[async_trait]
impl Widget for FileManagerWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }
    fn instance_id(&self) -> Uuid {
        self.instance_id
    }
    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
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
        Ok(())
    }
    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }
    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let config = self.config.read().clone();
        let state = self.state.read();
        let entries = self.current_entries.read().clone();
        let tab = state.active_tab();
        let tab_payload = build_tab_payload(tab, &entries, &config);
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
                    tabs: vec![build_tab_payload(right_tab, &[], &config)],
                    active_tab: 0,
                });
            }
        }
        let active_pane = match state.active_pane {
            ActivePane::Left => 0,
            ActivePane::Right => 1,
        };
        let clipboard_indicator = match self.deps.clipboard.operation() {
            ClipboardOperation::None => None,
            op => Some(format!(
                "{} {} ready to paste",
                self.deps.clipboard.len(),
                if op == ClipboardOperation::Cut { "entries (cut)" } else { "entries" }
            )),
        };

        Some(WidgetSnapshot {
            instance_id: self.instance_id,
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
        state_codec::save_state(&*self.config.read())
    }
    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let cfg: FileManagerConfig = state_codec::restore_state(bytes)?;
        *self.config.write() = cfg;
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

fn build_tab_payload(
    tab: &TabState,
    entries: &[orchid_fs::FsEntry],
    _config: &FileManagerConfig,
) -> TabPayload {
    let entry_payloads: Vec<EntryPayload> = entries
        .iter()
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
        breadcrumbs: Vec::new(),
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
