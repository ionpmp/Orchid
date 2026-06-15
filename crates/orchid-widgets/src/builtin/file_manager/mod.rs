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
use tracing::warn;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::error::WidgetError;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{
    EntryPayload, FileManagerPayload, FmViewMode, NetworkMountPayload, PanePayload, TabPayload,
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
pub use virtual_folders::{
    category_for_virtual_path, category_search_extensions, entry_matches_category, is_virtual,
    sidebar_catalog, FileCategory, VirtualFolder,
};

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
    /// Open each file path in the viewer (directories are skipped).
    OpenInViewerMany {
        paths: Vec<String>,
    },
    /// Open files with the system "Open with" application picker.
    OpenWithPicker {
        paths: Vec<String>,
    },
    /// Open files with the OS default application.
    OpenExternally {
        paths: Vec<String>,
    },
    /// Read-only info dialog (e.g. file properties).
    ShowInfo {
        title: String,
        message: String,
    },
    /// Prompt for a tag name to apply to `paths`.
    NeedsTag {
        paths: Vec<String>,
    },
    /// Prompt for a folder name under `parent`.
    NeedsCreateFolder {
        parent: String,
    },
    /// Prompt for a passphrase to encrypt or reveal encrypted files.
    NeedsPassphrase {
        paths: Vec<String>,
        purpose: PassphrasePurpose,
    },
}

/// Why the file manager needs a passphrase from the user.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassphrasePurpose {
    Encrypt,
    Decrypt,
    /// Reveal to temp and open with the OS default application.
    Reveal,
    /// Reveal to temp and open in the built-in viewer.
    RevealInViewer,
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
    /// Optional search index for category virtual folders.
    pub search: Option<Arc<orchid_search::SearchEngine>>,
    /// Managed-folder engine (content-addressed backup of on-disk trees).
    pub managed: Option<Arc<orchid_fs::ManagedFolderEngine>>,
    /// Encrypted-folder engine (age encryption + reveal sessions).
    pub encrypted: Option<Arc<orchid_fs::EncryptedFolderEngine>>,
    /// Configured remote mounts from `config.toml` `[file-manager]`.
    pub network_mounts: Arc<RwLock<Vec<orchid_storage::NetworkMountConfig>>>,
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
    state: parking_lot::Mutex<FileManagerState>,
    config: RwLock<FileManagerConfig>,
    /// Entries per tab id. Keeps dual-pane tabs independent.
    entries_by_tab: RwLock<std::collections::HashMap<Uuid, Vec<orchid_fs::FsEntry>>>,
    recent: RwLock<std::collections::VecDeque<String>>,
    /// Decoded thumbnails keyed by entry path (icon / gallery modes).
    thumbnail_rgba: RwLock<std::collections::HashMap<String, orchid_viewers::Thumbnail>>,
    /// Cached managed-folder root paths for [`apply_entry_metadata`].
    managed_roots: RwLock<Vec<String>>,
    /// Cached ingest stats per managed root path.
    managed_stats: RwLock<std::collections::HashMap<String, orchid_fs::ManagedFolderStats>>,
    /// Last ingested file name shown briefly in the status bar.
    ingest_notice: RwLock<Option<(String, std::time::Instant)>>,
    /// Cached encrypted paths for [`apply_entry_metadata`].
    encrypted_paths: RwLock<Vec<String>>,
    /// Last navigation error per tab (shown in the pane error banner).
    tab_errors: RwLock<std::collections::HashMap<Uuid, Option<String>>>,
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
                state: parking_lot::Mutex::new(state),
                config: RwLock::new(config),
                entries_by_tab: RwLock::new(std::collections::HashMap::new()),
                recent: RwLock::new(std::collections::VecDeque::new()),
                thumbnail_rgba: RwLock::new(std::collections::HashMap::new()),
                managed_roots: RwLock::new(Vec::new()),
                managed_stats: RwLock::new(std::collections::HashMap::new()),
                ingest_notice: RwLock::new(None),
                encrypted_paths: RwLock::new(Vec::new()),
                tab_errors: RwLock::new(std::collections::HashMap::new()),
                bus,
            }),
        }
    }

    /// Refresh the active tab's entry list.
    pub async fn refresh(&self) {
        let show_hidden = self.inner.config.read().show_hidden;
        let (left, right) = {
            let state = self.inner.state.lock().clone();
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
            let mut state = self.inner.state.lock();
            state.active_tab_mut().navigate_to(path);
        }
        self.refresh().await
    }

    /// Back one step in history.
    pub async fn go_back(&self) {
        let changed = {
            let mut state = self.inner.state.lock();
            state.active_tab_mut().back()
        };
        if changed {
            self.refresh().await;
        }
    }

    /// Forward one step in history.
    pub async fn go_forward(&self) {
        let changed = {
            let mut state = self.inner.state.lock();
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
                let mut state = inner.state.lock();
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
        let state = self.inner.state.lock().clone();
        let entries_map = self.inner.entries_by_tab.read().clone();
        let left_pane = build_pane_payload(
            &state.left_pane,
            &entries_map,
            &config,
            &*self.inner,
        );
        let dual_pane = config.dual_pane;
        let mut panes = vec![left_pane];
        if dual_pane {
            if let Some(right) = &state.right_pane {
                panes.push(build_pane_payload(
                    right,
                    &entries_map,
                    &config,
                    &*self.inner,
                ));
            }
        }
        let active_pane = match state.active_pane {
            ActivePane::Left => 0,
            ActivePane::Right => 1,
        };
        let tab = state.active_tab();
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
                managed_roots: self.inner.managed_roots.read().clone(),
                network_mounts: self.inner.network_mount_payloads(),
                activity_indicator: self.inner.activity_indicator_label(),
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

    fn activity_indicator_label(&self) -> Option<String> {
        let notice = self.ingest_notice.read();
        if let Some((name, at)) = notice.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(8) {
                return Some(name.clone());
            }
        }
        None
    }

    fn enabled_network_mounts(&self) -> Vec<orchid_storage::NetworkMountConfig> {
        self.deps
            .network_mounts
            .read()
            .iter()
            .filter(|m| m.enabled && !m.uri.trim().is_empty())
            .cloned()
            .collect()
    }

    fn network_mount_payloads(&self) -> Vec<NetworkMountPayload> {
        self.enabled_network_mounts()
            .into_iter()
            .filter_map(|m| {
                let uri = orchid_fs::normalize_mount_uri(&m.uri)?;
                Some(NetworkMountPayload {
                    name: network_mount_display_name(&m, &uri),
                    uri,
                })
            })
            .collect()
    }

    async fn handle_managed_ingest(&self, path: &orchid_fs::FsPath) {
        let label = path
            .file_name()
            .map(String::from)
            .unwrap_or_else(|| path.as_str().to_string());
        *self.ingest_notice.write() = Some((label, std::time::Instant::now()));
        self.publish_refresh();
        self.refresh_managed_roots().await;
        self.publish_refresh();
    }

    async fn refresh_all_tabs(&self) {
        self.refresh_managed_roots().await;
        self.refresh_encrypted_paths().await;
        let show_hidden = self.config.read().show_hidden;
        let (left, right) = {
            let state = self.state.lock().clone();
            let left = state.left_pane.active_tab().clone();
            let right = state
                .right_pane
                .as_ref()
                .map(|p| p.active_tab().clone());
            (left, right)
        };
        self.refresh_tab(&left, show_hidden).await;
        if let Some(rt) = right {
            self.refresh_tab(&rt, show_hidden).await;
        }
        self.publish_refresh();
    }

    async fn paste_clipboard(&self) -> WidgetResult<()> {
        let dest_dir = {
            let state = self.state.lock();
            state.active_tab().path.clone()
        };
        if is_virtual(&dest_dir) {
            return Ok(());
        }
        let (sources, op) = self.deps.clipboard.paste(&dest_dir);
        if sources.is_empty() || op == ClipboardOperation::None {
            return Ok(());
        }
        let registry = &self.deps.registry;
        for src in sources {
            let name = src
                .file_name()
                .map(str::to_string)
                .unwrap_or_else(|| "copy".to_string());
            let dest = dest_dir.join(&name);
            match op {
                ClipboardOperation::Copy => {
                    orchid_fs::operations::copy::copy(
                        registry,
                        &src,
                        &dest,
                        orchid_fs::operations::copy::CopyOptions::default(),
                        None,
                        None,
                    )
                    .await
                    .map_err(map_fs_error)?;
                }
                ClipboardOperation::Cut => {
                    orchid_fs::operations::move_::move_(registry, &src, &dest, None, None)
                        .await
                        .map_err(map_fs_error)?;
                }
                ClipboardOperation::None => break,
            }
        }
        Ok(())
    }

    async fn move_paths_into_directory(
        &self,
        sources: &[String],
        dest_dir: &orchid_fs::FsPath,
    ) -> WidgetResult<()> {
        if is_virtual(dest_dir) {
            return Err(WidgetError::InvalidStateForOperation(
                "cannot drop into virtual folder".into(),
            ));
        }
        let registry = &self.deps.registry;
        let dest_str = dest_dir.as_str();
        for p in sources {
            let src = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            if &src == dest_dir {
                continue;
            }
            let src_str = src.as_str();
            if dest_str.len() > src_str.len() {
                let rest = &dest_str[src_str.len()..];
                if rest.starts_with('/') || rest.starts_with('\\') {
                    continue;
                }
            }
            let name = src
                .file_name()
                .map(str::to_string)
                .unwrap_or_else(|| "moved".to_string());
            let dest = dest_dir.join(&name);
            if src == dest {
                continue;
            }
            orchid_fs::operations::move_::move_(registry, &src, &dest, None, None)
                .await
                .map_err(map_fs_error)?;
        }
        Ok(())
    }

    async fn copy_paths_into_directory(
        &self,
        sources: &[String],
        dest_dir: &orchid_fs::FsPath,
    ) -> WidgetResult<()> {
        if is_virtual(dest_dir) {
            return Err(WidgetError::InvalidStateForOperation(
                "cannot drop into virtual folder".into(),
            ));
        }
        let registry = &self.deps.registry;
        let dest_str = dest_dir.as_str();
        for p in sources {
            let src = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            if &src == dest_dir {
                continue;
            }
            let src_str = src.as_str();
            if dest_str.len() > src_str.len() {
                let rest = &dest_str[src_str.len()..];
                if rest.starts_with('/') || rest.starts_with('\\') {
                    continue;
                }
            }
            let name = src
                .file_name()
                .map(str::to_string)
                .unwrap_or_else(|| "copy".to_string());
            let dest = dest_dir.join(&name);
            if src == dest {
                continue;
            }
            orchid_fs::operations::copy::copy(
                registry,
                &src,
                &dest,
                orchid_fs::operations::copy::CopyOptions::default(),
                None,
                None,
            )
            .await
            .map_err(map_fs_error)?;
        }
        Ok(())
    }

    async fn delete_paths(&self, paths: &[String]) -> WidgetResult<()> {
        let registry = &self.deps.registry;
        let opts = orchid_fs::operations::delete::DeleteOptions::default();
        for p in paths {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            orchid_fs::operations::delete::delete(registry, &fp, opts)
                .await
                .map_err(map_fs_error)?;
        }
        Ok(())
    }

    async fn refresh_tab(&self, tab: &TabState, show_hidden: bool) {
        let path = tab.path.clone();

        if is_virtual(&path) {
            self.tab_errors.write().insert(tab.id, None);
            let mut entries = self.list_virtual(&path).await;
            sort_entries(&mut entries, tab.sort_by, tab.sort_descending);
            self.entries_by_tab.write().insert(tab.id, entries.clone());
            self.ensure_thumbnails(tab, &entries).await;
            return;
        }

        let result = self.navigator.navigate(&path, show_hidden).await;
        self.tab_errors
            .write()
            .insert(tab.id, result.error.clone());
        let mut entries = result.entries;
        self.apply_entry_metadata(&mut entries);
        sort_entries(&mut entries, tab.sort_by, tab.sort_descending);
        self.entries_by_tab.write().insert(tab.id, entries.clone());
        self.ensure_thumbnails(tab, &entries).await;
    }

    fn record_recent(&self, path: &orchid_fs::FsPath) {
        if is_virtual(path) {
            return;
        }
        if path.as_str().ends_with('/') || path.as_str().ends_with('\\') {
            return;
        }
        let s = path.as_str().to_string();
        let mut recent = self.recent.write();
        recent.retain(|p| p != &s);
        recent.push_front(s);
        while recent.len() > 50 {
            recent.pop_back();
        }
    }

    fn collect_catalog_candidates(&self) -> Vec<orchid_fs::FsPath> {
        let mut paths: Vec<orchid_fs::FsPath> = self
            .deps
            .tag_manager
            .starred_paths()
            .unwrap_or_default();
        paths.extend(
            self.recent
                .read()
                .iter()
                .filter_map(|p| orchid_fs::FsPath::new(p).ok()),
        );
        for tag in self.deps.tag_manager.all_tags().unwrap_or_default() {
            paths.extend(
                self.deps
                    .tag_manager
                    .paths_with_tag(&tag)
                    .unwrap_or_default(),
            );
        }
        paths.sort_by_key(|p| p.as_str().to_string());
        paths.dedup();
        paths
    }

    async fn hydrate_entries_metadata(&self, entries: &mut [orchid_fs::FsEntry]) {
        for e in entries.iter_mut() {
            let Some(provider) = self.deps.registry.for_path(&e.path) else {
                continue;
            };
            if let Ok(meta) = provider.metadata(&e.path).await {
                if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
                    continue;
                }
                e.metadata = meta;
            }
        }
    }

    async fn list_category(&self, cat: FileCategory) -> Vec<orchid_fs::FsEntry> {
        let mut path_keys: std::collections::HashSet<String> = self
            .collect_catalog_candidates()
            .into_iter()
            .map(|p| p.as_str().to_string())
            .collect();
        for p in self.search_category_paths(cat).await {
            path_keys.insert(p.as_str().to_string());
        }
        let mut entries = Vec::new();
        for key in path_keys {
            let Ok(p) = orchid_fs::FsPath::new(&key) else {
                continue;
            };
            let Some(provider) = self.deps.registry.for_path(&p) else {
                continue;
            };
            let meta = match provider.metadata(&p).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
                continue;
            }
            let entry = orchid_fs::FsEntry {
                path: p,
                name: key.rsplit('/').next().unwrap_or(&key).to_string(),
                metadata: meta,
            };
            if entry_matches_category(&entry, cat) {
                entries.push(entry);
            }
            if entries.len() >= 200 {
                break;
            }
        }
        self.apply_entry_metadata(&mut entries);
        entries
    }

    async fn search_category_paths(&self, cat: FileCategory) -> Vec<orchid_fs::FsPath> {
        let Some(engine) = self.deps.search.as_ref() else {
            return Vec::new();
        };
        let mut paths = Vec::new();
        for ext in category_search_extensions(cat) {
            let mut q = orchid_search::query::QueryBuilder::new()
                .extension(*ext)
                .limit(50)
                .build();
            q.only_files = true;
            if let Ok(results) = engine.search(q).await {
                for hit in results.hits {
                    if let Ok(p) = orchid_fs::FsPath::new(&hit.path) {
                        paths.push(p);
                    }
                }
            }
        }
        paths.sort_by_key(|p| p.as_str().to_string());
        paths.dedup();
        paths
    }

    fn filtered_paths_for_tab(&self, tab: &TabState) -> Vec<String> {
        let entries = self
            .entries_by_tab
            .read()
            .get(&tab.id)
            .cloned()
            .unwrap_or_default();
        let quick = tab.quick_filter.trim();
        if quick.is_empty() {
            return entries
                .into_iter()
                .map(|e| e.path.as_str().to_string())
                .collect();
        }
        let q = quick.to_lowercase();
        entries
            .into_iter()
            .filter(|e| e.name.to_lowercase().contains(&q))
            .map(|e| e.path.as_str().to_string())
            .collect()
    }

    fn select_all_in_pane(&self, pane: u8) {
        let (tab_id, paths) = {
            let state = self.state.lock();
            let tab = match active_tab_ref(&state, pane) {
                Ok(t) => t,
                Err(_) => return,
            };
            (tab.id, self.filtered_paths_for_tab(tab))
        };
        let mut state = self.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.tabs.iter_mut().find(|t| t.id == tab_id)
            } else {
                state.left_pane.tabs.iter_mut().find(|t| t.id == tab_id)
            }
        } else {
            state.left_pane.tabs.iter_mut().find(|t| t.id == tab_id)
        };
        if let Some(t) = tab {
            t.selection.select_all(&paths);
        }
    }

    fn deselect_all_in_pane(&self, pane: u8) {
        let mut state = self.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut()
            } else {
                state.left_pane.active_tab_mut()
            }
        } else {
            state.left_pane.active_tab_mut()
        };
        tab.selection.clear();
    }

    async fn create_folder_at(&self, parent: &orchid_fs::FsPath, name: &str) -> WidgetResult<()> {
        if name.is_empty()
            || name.contains('/')
            || name.contains('\\')
            || name.contains(':')
        {
            return Err(WidgetError::InvalidStateForOperation(
                "invalid folder name".into(),
            ));
        }
        let new_path = parent.join(name);
        let provider = self
            .deps
            .registry
            .for_path(parent)
            .ok_or_else(|| WidgetError::InvalidStateForOperation("no provider for parent".into()))?;
        provider
            .create_dir(&new_path, false)
            .await
            .map_err(map_fs_error)?;
        Ok(())
    }

    async fn ensure_thumbnails(&self, tab: &TabState, entries: &[orchid_fs::FsEntry]) {
        let mode_cfg = config_for_mode(tab.view_mode, 1.0);
        if !mode_cfg.show_thumbnails {
            return;
        }
        let thumb_size = viewer_thumb_size(self.config.read().thumbnail_size);
        let mut generated = false;
        for e in entries.iter().take(64) {
            if !is_image_entry(e) {
                continue;
            }
            let path_key = e.path.as_str().to_string();
            if self.thumbnail_rgba.read().contains_key(&path_key) {
                continue;
            }
            let modified_ms = e
                .metadata
                .modified
                .map(|t| t.timestamp_millis())
                .unwrap_or(0);
            let key = orchid_viewers::ThumbnailService::cache_key(&e.path, modified_ms);
            if let Ok(Some(thumb)) = self.deps.thumbnails.get_cached(&key, thumb_size).await {
                self.thumbnail_rgba.write().insert(path_key, thumb);
                generated = true;
                continue;
            }
            let Some(provider) = self.deps.registry.for_path(&e.path) else {
                continue;
            };
            let bytes = match provider.read(&e.path).await {
                Ok(b) if b.len() <= 16 * 1024 * 1024 => b,
                _ => continue,
            };
            if let Ok(thumb) = self
                .deps
                .thumbnails
                .generate_from_image_bytes(key, thumb_size, bytes)
                .await
            {
                self.thumbnail_rgba.write().insert(path_key, thumb);
                generated = true;
            }
        }
        if generated {
            self.publish_refresh();
        }
    }

    async fn list_virtual(&self, path: &orchid_fs::FsPath) -> Vec<orchid_fs::FsEntry> {
        let raw = path.as_str();
        if raw == "virtual:recent" {
            let mut entries: Vec<orchid_fs::FsEntry> = self
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
            self.hydrate_entries_metadata(&mut entries).await;
            self.apply_entry_metadata(&mut entries);
            return entries;
        }
        if raw == "virtual:starred" {
            let paths = self
                .deps
                .tag_manager
                .starred_paths()
                .unwrap_or_default();
            let mut entries: Vec<orchid_fs::FsEntry> = paths
                .into_iter()
                .take(200)
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
                        extended: orchid_fs::ExtendedAttributes {
                            starred: true,
                            ..orchid_fs::ExtendedAttributes::default()
                        },
                    },
                    path: p,
                })
                .collect();
            self.hydrate_entries_metadata(&mut entries).await;
            self.apply_entry_metadata(&mut entries);
            return entries;
        }
        if let Some(cat) = category_for_virtual_path(raw) {
            return self.list_category(cat).await;
        }
        if raw == "virtual:tags" {
            return self.list_tagged_paths().await;
        }
        if raw == "virtual:network" {
            return self.list_network_mounts();
        }
        Vec::new()
    }

    fn list_network_mounts(&self) -> Vec<orchid_fs::FsEntry> {
        self.enabled_network_mounts()
            .into_iter()
            .filter_map(|m| {
                let uri = orchid_fs::normalize_mount_uri(&m.uri)?;
                let path = orchid_fs::FsPath::new(&uri).ok()?;
                Some(orchid_fs::FsEntry {
                    name: network_mount_display_name(&m, &uri),
                    metadata: orchid_fs::FsMetadata {
                        kind: orchid_fs::FsEntryKind::Directory,
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
                    path,
                })
            })
            .collect()
    }

    async fn list_tagged_paths(&self) -> Vec<orchid_fs::FsEntry> {
        let mut seen = std::collections::BTreeSet::new();
        let mut paths = Vec::new();
        for tag in self.deps.tag_manager.all_tags().unwrap_or_default() {
            for p in self
                .deps
                .tag_manager
                .paths_with_tag(&tag)
                .unwrap_or_default()
            {
                let key = p.as_str().to_string();
                if seen.insert(key) {
                    paths.push(p);
                }
            }
            if paths.len() >= 200 {
                break;
            }
        }
        let mut entries: Vec<orchid_fs::FsEntry> = paths
            .into_iter()
            .take(200)
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
        self.hydrate_entries_metadata(&mut entries).await;
        self.apply_entry_metadata(&mut entries);
        entries
    }

    fn apply_entry_metadata(&self, entries: &mut [orchid_fs::FsEntry]) {
        let encrypted_paths = self.encrypted_paths.read().clone();
        let managed_roots = self.managed_roots.read().clone();
        let tag_manager = &self.deps.tag_manager;
        for e in entries.iter_mut() {
            if let Ok(Some(tag)) = tag_manager.get(&e.path) {
                e.metadata.extended.starred = tag.starred;
                e.metadata.extended.tags = tag.tags.clone();
                e.metadata.extended.color_label = tag.color_label;
            }
            if managed_roots
                .iter()
                .any(|root| e.path.as_str().starts_with(root))
            {
                e.metadata.extended.is_managed = true;
            }
            if encrypted_paths
                .iter()
                .any(|p| e.path.as_str() == p || e.path.as_str().starts_with(p))
                || orchid_fs::encrypted::marker::looks_encrypted(&e.path)
                || orchid_fs::encrypted::marker::looks_encrypted_directory(&e.path)
            {
                e.metadata.extended.is_encrypted = true;
            }
        }
    }

    fn is_path_encrypted(&self, path: &orchid_fs::FsPath) -> bool {
        if orchid_fs::encrypted::marker::looks_encrypted(path)
            || orchid_fs::encrypted::marker::looks_encrypted_directory(path)
        {
            return true;
        }
        self.encrypted_paths.read().iter().any(|p| {
            path.as_str() == p || path.as_str().starts_with(p)
        })
    }

    async fn refresh_encrypted_paths(&self) {
        let paths = if let Some(engine) = self.deps.encrypted.as_ref() {
            engine
                .list_encrypted()
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|r| r.enabled)
                .map(|r| r.path.as_str().to_string())
                .collect()
        } else {
            Vec::new()
        };
        *self.encrypted_paths.write() = paths;
    }

    async fn encrypt_paths(&self, paths: &[String], passphrase: &str) -> WidgetResult<()> {
        let Some(engine) = self.deps.encrypted.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "encryption unavailable".into(),
            ));
        };
        let identity = orchid_crypto::Identity::passphrase(passphrase);
        for p in paths {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            let is_dir = if let Some(provider) = self.deps.registry.for_path(&fp) {
                provider
                    .metadata(&fp)
                    .await
                    .map(|meta| matches!(meta.kind, orchid_fs::FsEntryKind::Directory))
                    .unwrap_or(false)
            } else {
                false
            };
            if is_dir {
                engine
                    .encrypt_directory_in_place(&fp, identity.clone())
                    .await
                    .map_err(map_fs_error)?;
            } else {
                engine
                    .encrypt_in_place(&fp, identity.clone())
                    .await
                    .map_err(map_fs_error)?;
            }
        }
        Ok(())
    }

    async fn decrypt_paths(&self, paths: &[String], passphrase: &str) -> WidgetResult<()> {
        let Some(engine) = self.deps.encrypted.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "encryption unavailable".into(),
            ));
        };
        let identity = orchid_crypto::Identity::passphrase(passphrase);
        for p in paths {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            engine
                .decrypt_in_place(&fp, identity.clone())
                .await
                .map_err(map_fs_error)?;
        }
        Ok(())
    }

    async fn reveal_paths(&self, paths: &[String], passphrase: &str) -> WidgetResult<Vec<String>> {
        let Some(engine) = self.deps.encrypted.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "encryption unavailable".into(),
            ));
        };
        let identity = orchid_crypto::Identity::passphrase(passphrase);
        let mut revealed = Vec::with_capacity(paths.len());
        for p in paths {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            let session = engine
                .reveal(&fp, identity.clone())
                .await
                .map_err(map_fs_error)?;
            revealed.push(session.revealed_path.to_string_lossy().into_owned());
        }
        Ok(revealed)
    }

    async fn refresh_managed_roots(&self) {
        let mut roots = Vec::new();
        let mut stats = std::collections::HashMap::new();
        if let Some(engine) = self.deps.managed.as_ref() {
            if let Ok(folders) = engine.list_folders().await {
                for f in folders.into_iter().filter(|f| f.enabled) {
                    let key = f.path.as_str().to_string();
                    roots.push(key.clone());
                    if let Ok(st) = engine.folder_stats(&f.path).await {
                        stats.insert(key, st);
                    }
                }
            }
        }
        *self.managed_roots.write() = roots;
        *self.managed_stats.write() = stats;
    }

    async fn register_managed_folder(&self, folder: &orchid_fs::FsPath) -> WidgetResult<()> {
        let Some(engine) = self.deps.managed.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "managed folders unavailable".into(),
            ));
        };
        let cfg = orchid_fs::ManagedFolderConfig {
            path: folder.clone(),
            chunk_size: orchid_crypto::ChunkerConfig::default(),
            enabled: true,
            auto_ingest: true,
        };
        engine.add_folder(cfg).await.map_err(map_fs_error)?;
        Ok(())
    }

    async fn add_selection_to_managed(&self, paths: &[String]) -> WidgetResult<()> {
        let folder = self.resolve_managed_folder_target(paths).await?;
        self.register_managed_folder(&folder).await?;
        if let Some(engine) = self.deps.managed.as_ref() {
            for p in paths {
                let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
                if let Some(provider) = self.deps.registry.for_path(&fp) {
                    if let Ok(meta) = provider.metadata(&fp).await {
                        if matches!(meta.kind, orchid_fs::FsEntryKind::File) {
                            if let Err(e) = engine.ingest(&fp).await {
                                warn!(error = %e, path = %p, "managed ingest failed");
                            }
                        }
                    }
                }
            }
        }
        self.refresh_managed_roots().await;
        Ok(())
    }

    async fn remove_selection_from_managed(&self, paths: &[String]) -> WidgetResult<()> {
        let Some(engine) = self.deps.managed.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "managed folders unavailable".into(),
            ));
        };
        for p in paths {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            engine.remove_folder(&fp).await.map_err(map_fs_error)?;
        }
        self.refresh_managed_roots().await;
        Ok(())
    }

    async fn resolve_managed_folder_target(
        &self,
        paths: &[String],
    ) -> WidgetResult<orchid_fs::FsPath> {
        if paths.is_empty() {
            return Err(WidgetError::InvalidStateForOperation(
                "no selection for managed folder".into(),
            ));
        }
        let mut folder_candidates: Vec<orchid_fs::FsPath> = Vec::new();
        for p in paths {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            if let Some(provider) = self.deps.registry.for_path(&fp) {
                if let Ok(meta) = provider.metadata(&fp).await {
                    if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
                        folder_candidates.push(fp);
                        continue;
                    }
                }
            }
            let parent = fp
                .parent()
                .ok_or_else(|| WidgetError::InvalidStateForOperation("no parent folder".into()))?;
            folder_candidates.push(parent);
        }
        let first = folder_candidates[0].as_str();
        if !folder_candidates.iter().all(|f| f.as_str() == first) {
            return Err(WidgetError::InvalidStateForOperation(
                "selection spans multiple folders".into(),
            ));
        }
        Ok(folder_candidates[0].clone())
    }
}

fn build_pane_payload(
    pane: &PaneState,
    entries_map: &std::collections::HashMap<Uuid, Vec<orchid_fs::FsEntry>>,
    config: &FileManagerConfig,
    inner: &FileManagerInner,
) -> PanePayload {
    let tabs: Vec<TabPayload> = pane
        .tabs
        .iter()
        .map(|tab| {
            let entries = entries_map.get(&tab.id).map(Vec::as_slice).unwrap_or(&[]);
            build_tab_payload(tab, entries, config, inner)
        })
        .collect();
    PanePayload {
        tabs,
        active_tab: pane.active_tab as u32,
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

    let thumb_cache = inner.thumbnail_rgba.read();
    let entry_payloads: Vec<EntryPayload> = entries_filtered
        .into_iter()
        .map(|e| {
            let path_key = e.path.as_str();
            let (has_thumbnail, thumbnail_rgba, thumbnail_width, thumbnail_height) =
                if let Some(t) = thumb_cache.get(path_key) {
                    (
                        true,
                        Some(t.rgba.as_ref().clone()),
                        t.width,
                        t.height,
                    )
                } else {
                    (false, None, 0, 0)
                };
            EntryPayload {
                path: path_key.to_string(),
                name: e.name.clone(),
                is_dir: matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory),
                size_text: format_size(e.metadata.size),
                modified_text: e
                    .metadata
                    .modified
                    .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_default(),
                type_text: classify(
                    &e.name,
                    e.metadata.mime.as_deref(),
                    matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory),
                ),
                icon: if matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory) {
                    "folder".into()
                } else {
                    "file".into()
                },
                has_thumbnail,
                thumbnail_key: None,
                thumbnail_rgba,
                thumbnail_width,
                thumbnail_height,
                is_selected: tab.selection.is_selected(path_key),
                is_hidden: e.metadata.hidden,
                is_encrypted: e.metadata.extended.is_encrypted,
                is_managed: e.metadata.extended.is_managed,
                is_starred: e.metadata.extended.starred,
                color_label: e
                    .metadata
                    .extended
                    .color_label
                    .map(color_label_to_str),
                tags: e.metadata.extended.tags.clone(),
            }
        })
        .collect();
    let selection_count = tab.selection.count() as u32;
    let status_text = if let Some(st) = inner.managed_stats.read().get(tab.path.as_str()) {
        let saved = st.logical_bytes.saturating_sub(st.physical_bytes);
        format!(
            "{} items, {} selected · {} ingested, {} deduped",
            entry_payloads.len(),
            selection_count,
            st.files_tracked,
            format_size(saved),
        )
    } else {
        format!(
            "{} items, {} selected",
            entry_payloads.len(),
            selection_count,
        )
    };
    let error = if tab.path.as_str() == "virtual:network" && entry_payloads.is_empty() {
        Some("network-placeholder".into())
    } else {
        inner
            .tab_errors
            .read()
            .get(&tab.id)
            .cloned()
            .flatten()
    };
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
        error,
        sort_by: sort_by_to_u8(tab.sort_by),
        sort_descending: tab.sort_descending,
    }
}

fn sort_by_to_u8(sort_by: SortBy) -> u8 {
    match sort_by {
        SortBy::Name => 0,
        SortBy::Size => 1,
        SortBy::Modified => 2,
        SortBy::Type => 3,
    }
}

fn sort_by_from_u8(column: u8) -> Option<SortBy> {
    match column {
        0 => Some(SortBy::Name),
        1 => Some(SortBy::Size),
        2 => Some(SortBy::Modified),
        3 => Some(SortBy::Type),
        _ => None,
    }
}

fn next_sort_by(current: SortBy) -> SortBy {
    match current {
        SortBy::Name => SortBy::Size,
        SortBy::Size => SortBy::Modified,
        SortBy::Modified => SortBy::Type,
        SortBy::Type => SortBy::Name,
    }
}

fn sort_entries(entries: &mut [orchid_fs::FsEntry], sort_by: SortBy, descending: bool) {
    use std::cmp::Ordering;
    entries.sort_by(|a, b| {
        let ad = matches!(a.metadata.kind, orchid_fs::FsEntryKind::Directory);
        let bd = matches!(b.metadata.kind, orchid_fs::FsEntryKind::Directory);
        let dir_ord = match (ad, bd) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => Ordering::Equal,
        };
        if dir_ord != Ordering::Equal {
            return if descending {
                dir_ord.reverse()
            } else {
                dir_ord
            };
        }
        let field = match sort_by {
            SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortBy::Size => a.metadata.size.cmp(&b.metadata.size),
            SortBy::Modified => {
                let am = a
                    .metadata
                    .modified
                    .map(|t| t.timestamp())
                    .unwrap_or(0);
                let bm = b
                    .metadata
                    .modified
                    .map(|t| t.timestamp())
                    .unwrap_or(0);
                am.cmp(&bm)
            }
            SortBy::Type => {
                let ae = a
                    .path
                    .extension()
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();
                let be = b
                    .path
                    .extension()
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();
                ae.cmp(&be).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            }
        };
        if descending {
            field.reverse()
        } else {
            field
        }
    });
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

fn is_image_entry(e: &orchid_fs::FsEntry) -> bool {
    if e.metadata.mime.as_deref().map(|m| m.starts_with("image/")).unwrap_or(false) {
        return true;
    }
    let ext = e
        .path
        .extension()
        .map(|x| x.to_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "avif"
    )
}

fn viewer_thumb_size(size: config::ThumbnailSize) -> orchid_viewers::ThumbnailSize {
    match size {
        config::ThumbnailSize::Small => orchid_viewers::ThumbnailSize::Small,
        config::ThumbnailSize::Medium => orchid_viewers::ThumbnailSize::Medium,
        config::ThumbnailSize::Large => orchid_viewers::ThumbnailSize::Large,
    }
}

fn color_label_to_str(label: orchid_storage::ColorLabel) -> String {
    match label {
        orchid_storage::ColorLabel::Red => "red",
        orchid_storage::ColorLabel::Orange => "orange",
        orchid_storage::ColorLabel::Yellow => "yellow",
        orchid_storage::ColorLabel::Green => "green",
        orchid_storage::ColorLabel::Blue => "blue",
        orchid_storage::ColorLabel::Purple => "purple",
        orchid_storage::ColorLabel::Gray => "gray",
    }
    .to_string()
}

fn color_label_from_action_id(action_id: &str) -> Option<orchid_storage::ColorLabel> {
    use orchid_storage::ColorLabel;
    match action_id.strip_prefix("fs.color-label:") {
        Some("red") => Some(ColorLabel::Red),
        Some("orange") => Some(ColorLabel::Orange),
        Some("yellow") => Some(ColorLabel::Yellow),
        Some("green") => Some(ColorLabel::Green),
        Some("blue") => Some(ColorLabel::Blue),
        Some("purple") => Some(ColorLabel::Purple),
        Some("gray") => Some(ColorLabel::Gray),
        Some("none") | Some("clear") => None,
        _ => None,
    }
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

fn map_fs_error(e: orchid_fs::FsError) -> WidgetError {
    WidgetError::InvalidStateForOperation(e.to_string())
}

/// Options for [`run_action`].
#[derive(Debug, Clone, Copy, Default)]
pub struct RunActionOpts {
    /// When true, destructive actions skip confirmation (user already confirmed).
    pub skip_confirm: bool,
}

/// Build context-menu descriptors for a right-click on `context_path` in `pane`.
pub fn context_menu_for(
    instance_id: Uuid,
    pane: u8,
    context_path: &str,
) -> WidgetResult<(Vec<ContextMenuItem>, Vec<String>)> {
    let inner = live_inner(instance_id)?;
    if context_path.is_empty() {
        inner.deselect_all_in_pane(pane);
    }
    let (entries, target_paths, entry_count, selection_count) = {
        let state = inner.state.lock();
        let tab = active_tab_ref(&state, pane)?;
        let tab_id = tab.id;
        let selection = tab.selection.selected_paths();
        let entries = inner
            .entries_by_tab
            .read()
            .get(&tab_id)
            .cloned()
            .unwrap_or_default();
        let entry_count = inner.filtered_paths_for_tab(tab).len();
        let selection_count = tab.selection.count();
        let target_paths = if context_path.is_empty() {
            Vec::new()
        } else if selection.iter().any(|p| p == context_path) {
            selection
        } else {
            vec![context_path.to_string()]
        };
        (entries, target_paths, entry_count, selection_count)
    };
    let selected_entries: Vec<orchid_fs::FsEntry> = target_paths
        .iter()
        .filter_map(|p| entries.iter().find(|e| e.path.as_str() == p))
        .cloned()
        .collect();
    let mut tag_union = std::collections::BTreeSet::new();
    for e in &selected_entries {
        for t in &e.metadata.extended.tags {
            tag_union.insert(t.clone());
        }
    }
    let inputs = ContextMenuInputs {
        clipboard_has_contents: !inner.deps.clipboard.is_empty(),
        all_encrypted: selected_entries
            .iter()
            .all(|e| e.metadata.extended.is_encrypted),
        any_encrypted: selected_entries
            .iter()
            .any(|e| e.metadata.extended.is_encrypted),
        all_managed: selected_entries
            .iter()
            .all(|e| e.metadata.extended.is_managed),
        all_starred: selected_entries
            .iter()
            .all(|e| e.metadata.extended.starred),
        any_starred: selected_entries
            .iter()
            .any(|e| e.metadata.extended.starred),
        known_tags: inner
            .deps
            .tag_manager
            .all_tags()
            .unwrap_or_default()
            .into_iter()
            .take(12)
            .collect(),
        tags_on_selection: tag_union.into_iter().take(12).collect(),
        entry_count,
        selection_count,
    };
    Ok((build_for_selection(&selected_entries, inputs), target_paths))
}

/// Select `context_path` when it is not already part of the current selection.
pub async fn focus_context_target(
    instance_id: Uuid,
    pane: u8,
    context_path: &str,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let needs_select = {
        let state = inner.state.lock();
        let tab = active_tab_ref(&state, pane)?;
        !tab.selection.is_selected(context_path)
    };
    if needs_select {
        select_entry(instance_id, pane, context_path, SelectionMode::Single).await?;
    }
    Ok(())
}

fn active_tab_ref(
    state: &FileManagerState,
    pane: u8,
) -> WidgetResult<&TabState> {
    if pane == 1 {
        if let Some(r) = state.right_pane.as_ref() {
            return Ok(r.active_tab());
        }
    }
    Ok(state.left_pane.active_tab())
}

/// Navigate the given `pane` (0 left, 1 right) to `path`.
pub async fn navigate(instance_id: Uuid, pane: u8, path: orchid_fs::FsPath) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut state = inner.state.lock();
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
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Back in history for `pane`.
pub async fn navigate_back(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let changed = {
        let mut state = inner.state.lock();
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
        inner.refresh_all_tabs().await;
    }
    Ok(())
}

/// Forward in history for `pane`.
pub async fn navigate_forward(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let changed = {
        let mut state = inner.state.lock();
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
        inner.refresh_all_tabs().await;
    }
    Ok(())
}

/// Up to parent folder for `pane`.
pub async fn navigate_up(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let parent = {
        let state = inner.state.lock();
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
        let mut state = inner.state.lock();
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
        let mut state = inner.state.lock();
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
        let mut state = inner.state.lock();
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
        let mut state = inner.state.lock();
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
        let mut state = inner.state.lock();
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

/// Whether hidden entries are listed in navigation results.
pub fn show_hidden(instance_id: Uuid) -> WidgetResult<bool> {
    Ok(live_inner(instance_id)?.config.read().show_hidden)
}

/// Toggle whether hidden entries are shown in listings.
pub async fn toggle_show_hidden(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut cfg = inner.config.write();
        cfg.show_hidden = !cfg.show_hidden;
    }
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Toggle single-click vs double-click to open files.
pub async fn toggle_click_behavior(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut cfg = inner.config.write();
        cfg.click_behavior = match cfg.click_behavior {
            ClickBehavior::DoubleToOpen => ClickBehavior::SingleToOpen,
            ClickBehavior::SingleToOpen => ClickBehavior::DoubleToOpen,
        };
    }
    inner.publish_refresh();
    Ok(())
}

/// Cycle view mode for the active tab in `pane`.
pub async fn cycle_view_mode(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut state = inner.state.lock();
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

/// Cycle the sort column for the active tab in `pane` (folders stay grouped first).
pub async fn cycle_sort(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut state = inner.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut()
            } else {
                state.left_pane.active_tab_mut()
            }
        } else {
            state.left_pane.active_tab_mut()
        };
        tab.sort_by = next_sort_by(tab.sort_by);
        tab.sort_descending = false;
    }
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Set sort column for the active tab in `pane`; toggles direction when the column is unchanged.
pub async fn set_sort_column(instance_id: Uuid, pane: u8, column: u8) -> WidgetResult<()> {
    let sort_by = sort_by_from_u8(column).ok_or_else(|| {
        WidgetError::InvalidStateForOperation("invalid sort column".into())
    })?;
    let inner = live_inner(instance_id)?;
    {
        let mut state = inner.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut()
            } else {
                state.left_pane.active_tab_mut()
            }
        } else {
            state.left_pane.active_tab_mut()
        };
        if tab.sort_by == sort_by {
            tab.sort_descending = !tab.sort_descending;
        } else {
            tab.sort_by = sort_by;
            tab.sort_descending = false;
        }
    }
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Update quick filter text.
pub async fn set_quick_filter(instance_id: Uuid, pane: u8, q: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut state = inner.state.lock();
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
        let state = inner.state.lock();
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
        let mut state = inner.state.lock();
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
    run_action_with_opts(instance_id, action_id, target_paths, RunActionOpts::default()).await
}

/// Like [`run_action`] with extra flags (e.g. skip delete confirmation).
pub async fn run_action_with_opts(
    instance_id: Uuid,
    action_id: &str,
    target_paths: Vec<String>,
    opts: RunActionOpts,
) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    if let Some(tag) = action_id.strip_prefix("fs.tag-remove:") {
        if !tag.is_empty() {
            for p in &target_paths {
                let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
                inner
                    .deps
                    .tag_manager
                    .remove_tag(&fp, tag)
                    .map_err(map_fs_error)?;
            }
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
    }
    if let Some(tag) = action_id.strip_prefix("fs.tag:") {
        if !tag.is_empty() {
            for p in &target_paths {
                let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
                inner
                    .deps
                    .tag_manager
                    .add_tag(&fp, tag)
                    .map_err(map_fs_error)?;
            }
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
    }
    match action_id {
        "fs.open" => {
            let Some(p) = target_paths.first() else {
                return Ok(ActionOutcome::Done);
            };
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            if inner.is_path_encrypted(&fp) {
                return Ok(ActionOutcome::NeedsPassphrase {
                    paths: vec![p.clone()],
                    purpose: PassphrasePurpose::RevealInViewer,
                });
            }
            inner.record_recent(&fp);
            return Ok(ActionOutcome::OpenInViewer { path: p.clone() });
        }
        "fs.open-all" => {
            let mut files = Vec::new();
            for p in target_paths {
                let fp = orchid_fs::FsPath::new(&p).map_err(map_fs_error)?;
                if let Some(provider) = inner.deps.registry.for_path(&fp) {
                    if let Ok(meta) = provider.metadata(&fp).await {
                        if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
                            continue;
                        }
                    }
                }
                files.push(p);
            }
            if files.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::OpenInViewerMany { paths: files });
        }
        "viewer.open" => {
            let Some(p) = target_paths.first() else {
                return Ok(ActionOutcome::Done);
            };
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            if inner.is_path_encrypted(&fp) {
                return Ok(ActionOutcome::NeedsPassphrase {
                    paths: vec![p.clone()],
                    purpose: PassphrasePurpose::RevealInViewer,
                });
            }
            return Ok(ActionOutcome::OpenInViewer { path: p.clone() });
        }
        "fs.open-external" => {
            let mut files = Vec::new();
            for p in &target_paths {
                let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
                if let Some(provider) = inner.deps.registry.for_path(&fp) {
                    if let Ok(meta) = provider.metadata(&fp).await {
                        if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
                            if inner.is_path_encrypted(&fp) {
                                return Ok(ActionOutcome::NeedsPassphrase {
                                    paths: vec![p.clone()],
                                    purpose: PassphrasePurpose::Reveal,
                                });
                            }
                            continue;
                        }
                    }
                }
                files.push(p.clone());
            }
            if files.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            if files.iter().all(|p| {
                orchid_fs::FsPath::new(p)
                    .map(|fp| inner.is_path_encrypted(&fp))
                    .unwrap_or(false)
            }) {
                return Ok(ActionOutcome::NeedsPassphrase {
                    paths: files,
                    purpose: PassphrasePurpose::Reveal,
                });
            }
            return Ok(ActionOutcome::OpenExternally { paths: files });
        }
        "fs.open-with" => {
            let mut files = Vec::new();
            for p in &target_paths {
                let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
                if let Some(provider) = inner.deps.registry.for_path(&fp) {
                    if let Ok(meta) = provider.metadata(&fp).await {
                        if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
                            continue;
                        }
                    }
                }
                if inner.is_path_encrypted(&fp) {
                    return Ok(ActionOutcome::NeedsPassphrase {
                        paths: vec![p.clone()],
                        purpose: PassphrasePurpose::Reveal,
                    });
                }
                files.push(p.clone());
            }
            if files.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::OpenWithPicker { paths: files });
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
            inner.paste_clipboard().await?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
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
            if cfg.confirm_delete && !target_paths.is_empty() && !opts.skip_confirm {
                return Ok(ActionOutcome::NeedsConfirmation {
                    message: format!("Delete {} items?", target_paths.len()),
                    action_id: action_id.to_string(),
                    paths: target_paths,
                });
            }
            inner.delete_paths(&target_paths).await?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.star" => {
            for p in &target_paths {
                let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
                inner
                    .deps
                    .tag_manager
                    .set_starred(&fp, true)
                    .map_err(map_fs_error)?;
            }
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.unstar" => {
            for p in &target_paths {
                let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
                inner
                    .deps
                    .tag_manager
                    .set_starred(&fp, false)
                    .map_err(map_fs_error)?;
            }
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.tag-add" => {
            if target_paths.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::NeedsTag {
                paths: target_paths,
            });
        }
        "fs.select-all" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.select_all_in_pane(pane);
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.deselect-all" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.deselect_all_in_pane(pane);
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.new-folder" => {
            let parent = {
                let state = inner.state.lock();
                let pane = match state.active_pane {
                    ActivePane::Left => 0,
                    ActivePane::Right => 1,
                };
                active_tab_ref(&state, pane)
                    .map(|t| t.path.clone())
                    .ok()
            };
            if let Some(parent) = parent {
                if !is_virtual(&parent) {
                    return Ok(ActionOutcome::NeedsCreateFolder {
                        parent: parent.as_str().to_string(),
                    });
                }
            }
            return Ok(ActionOutcome::Done);
        }
        "fs.color-label" => {
            // Parent row only opens the flyout submenu.
            return Ok(ActionOutcome::Done);
        }
        action_id if action_id.starts_with("fs.color-label:") => {
            let color = color_label_from_action_id(action_id);
            for p in &target_paths {
                let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
                inner
                    .deps
                    .tag_manager
                    .set_color(&fp, color)
                    .map_err(map_fs_error)?;
            }
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.properties" => {
            let mut lines = Vec::new();
            for p in &target_paths {
                let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
                let name = fp.file_name().unwrap_or(p.as_str()).to_string();
                if let Some(provider) = inner.deps.registry.for_path(&fp) {
                    if let Ok(meta) = provider.metadata(&fp).await {
                        let kind = if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
                            "Folder"
                        } else {
                            "File"
                        };
                        let modified = meta
                            .modified
                            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_else(|| "—".into());
                        let mime = meta.mime.unwrap_or_else(|| "—".into());
                        lines.push(format!(
                            "{name}\n  Type: {kind}\n  Size: {}\n  Modified: {modified}\n  MIME: {mime}",
                            format_size(meta.size)
                        ));
                        continue;
                    }
                }
                lines.push(name);
            }
            return Ok(ActionOutcome::ShowInfo {
                title: "fm-properties-title".to_string(),
                message: lines.join("\n\n"),
            });
        }
        "fs.encrypt" => {
            if target_paths.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::NeedsPassphrase {
                paths: target_paths,
                purpose: PassphrasePurpose::Encrypt,
            });
        }
        "fs.add-to-managed" => {
            inner.add_selection_to_managed(&target_paths).await?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.remove-from-managed" => {
            inner.remove_selection_from_managed(&target_paths).await?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.decrypt" => {
            if target_paths.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::NeedsPassphrase {
                paths: target_paths,
                purpose: PassphrasePurpose::Decrypt,
            });
        }
        "fs.reveal" => {
            if target_paths.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::NeedsPassphrase {
                paths: target_paths,
                purpose: PassphrasePurpose::Reveal,
            });
        }
        _ => {
            // Unknown actions: treat as done for MVP.
        }
    }
    inner.publish_refresh();
    Ok(ActionOutcome::Done)
}

/// Commit rename on the backing filesystem.
pub async fn rename(instance_id: Uuid, old_path: &str, new_name: &str) -> WidgetResult<()> {
    if new_name.is_empty()
        || new_name.contains('/')
        || new_name.contains('\\')
        || new_name.contains(':')
    {
        return Err(WidgetError::InvalidStateForOperation(
            "invalid rename target".into(),
        ));
    }
    let inner = live_inner(instance_id)?;
    let old = orchid_fs::FsPath::new(old_path).map_err(map_fs_error)?;
    let parent = old
        .parent()
        .ok_or_else(|| WidgetError::InvalidStateForOperation("cannot rename root".into()))?;
    let new_path = parent.join(new_name);
    let provider = inner
        .deps
        .registry
        .for_path(&old)
        .ok_or_else(|| WidgetError::InvalidStateForOperation(format!("no provider for {old_path}")))?;
    provider
        .rename(&old, &new_path)
        .await
        .map_err(map_fs_error)?;
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Create a subfolder under `parent_path`.
pub async fn create_folder(instance_id: Uuid, parent_path: &str, name: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let parent = orchid_fs::FsPath::new(parent_path).map_err(map_fs_error)?;
    if is_virtual(&parent) {
        return Err(WidgetError::InvalidStateForOperation(
            "cannot create folder in virtual location".into(),
        ));
    }
    inner.create_folder_at(&parent, name).await?;
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Open the new-folder dialog for `pane`'s current directory.
pub async fn request_new_folder(instance_id: Uuid, pane: u8) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    let parent = {
        let state = inner.state.lock();
        active_tab_ref(&state, pane)?.path.clone()
    };
    if is_virtual(&parent) {
        return Ok(ActionOutcome::Done);
    }
    Ok(ActionOutcome::NeedsCreateFolder {
        parent: parent.as_str().to_string(),
    })
}

/// Apply `tag` to every path in `paths`.
pub async fn add_tag_to_paths(
    instance_id: Uuid,
    paths: Vec<String>,
    tag: &str,
) -> WidgetResult<()> {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return Err(WidgetError::InvalidStateForOperation("empty tag".into()));
    }
    let inner = live_inner(instance_id)?;
    for p in paths {
        let fp = orchid_fs::FsPath::new(&p).map_err(map_fs_error)?;
        inner
            .deps
            .tag_manager
            .add_tag(&fp, trimmed)
            .map_err(map_fs_error)?;
    }
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Select every visible entry in `pane`'s active tab.
pub async fn select_all_in_pane(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.select_all_in_pane(pane);
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Clear selection in `pane`'s active tab.
pub async fn deselect_all_in_pane(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.deselect_all_in_pane(pane);
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Apply a passphrase for encrypt or reveal after [`ActionOutcome::NeedsPassphrase`].
pub async fn apply_passphrase(
    instance_id: Uuid,
    paths: Vec<String>,
    passphrase: String,
    purpose: PassphrasePurpose,
) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    match purpose {
        PassphrasePurpose::Encrypt => {
            inner.encrypt_paths(&paths, &passphrase).await?;
            inner.refresh_all_tabs().await;
            Ok(ActionOutcome::Done)
        }
        PassphrasePurpose::Decrypt => {
            inner.decrypt_paths(&paths, &passphrase).await?;
            inner.refresh_all_tabs().await;
            Ok(ActionOutcome::Done)
        }
        PassphrasePurpose::Reveal => {
            let revealed = inner.reveal_paths(&paths, &passphrase).await?;
            Ok(ActionOutcome::OpenExternally { paths: revealed })
        }
        PassphrasePurpose::RevealInViewer => {
            let revealed = inner.reveal_paths(&paths, &passphrase).await?;
            if let Some(path) = revealed.first() {
                Ok(ActionOutcome::OpenInViewer {
                    path: path.clone(),
                })
            } else {
                Ok(ActionOutcome::Done)
            }
        }
    }
}

/// Current single- vs double-click open behaviour.
pub fn click_behavior(instance_id: Uuid) -> WidgetResult<ClickBehavior> {
    Ok(live_inner(instance_id)?.config.read().click_behavior)
}

/// Open a path from the listing (navigate directories, reveal or view files).
pub async fn open_path(instance_id: Uuid, pane: u8, path: &str) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    let fp = orchid_fs::FsPath::new(path).map_err(map_fs_error)?;

    let is_dir = if let Some(provider) = inner.deps.registry.for_path(&fp) {
        provider
            .metadata(&fp)
            .await
            .map(|meta| matches!(meta.kind, orchid_fs::FsEntryKind::Directory))
            .unwrap_or(false)
    } else {
        false
    };

    if is_dir {
        if inner.is_path_encrypted(&fp) {
            return Ok(ActionOutcome::NeedsPassphrase {
                paths: vec![path.to_string()],
                purpose: PassphrasePurpose::Reveal,
            });
        }
        navigate(instance_id, pane, fp).await?;
        return Ok(ActionOutcome::Done);
    }

    if inner.is_path_encrypted(&fp) {
        return Ok(ActionOutcome::NeedsPassphrase {
            paths: vec![path.to_string()],
            purpose: PassphrasePurpose::RevealInViewer,
        });
    }

    inner.record_recent(&fp);
    Ok(ActionOutcome::OpenInViewer {
        path: path.to_string(),
    })
}

/// Refresh every tab in a live file-manager instance.
pub async fn refresh_instance(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.refresh_all_tabs().await;
    inner.publish_refresh();
    Ok(())
}

/// Move `sources` into directory `dest_dir` (drag-and-drop target).
pub async fn move_paths_to_directory(
    instance_id: Uuid,
    sources: Vec<String>,
    dest_dir: &str,
) -> WidgetResult<()> {
    if sources.is_empty() {
        return Ok(());
    }
    let inner = live_inner(instance_id)?;
    let dest = orchid_fs::FsPath::new(dest_dir).map_err(map_fs_error)?;
    if let Some(provider) = inner.deps.registry.for_path(&dest) {
        let meta = provider.metadata(&dest).await.map_err(map_fs_error)?;
        if !matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
            return Err(WidgetError::InvalidStateForOperation(
                "drop target is not a directory".into(),
            ));
        }
    } else {
        return Err(WidgetError::InvalidStateForOperation(
            "drop target unavailable".into(),
        ));
    }
    inner.move_paths_into_directory(&sources, &dest).await?;
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Copy `sources` into directory `dest_dir` (Ctrl+drag or Ctrl+OS drop).
pub async fn copy_paths_to_directory(
    instance_id: Uuid,
    sources: Vec<String>,
    dest_dir: &str,
) -> WidgetResult<()> {
    if sources.is_empty() {
        return Ok(());
    }
    let inner = live_inner(instance_id)?;
    let dest = orchid_fs::FsPath::new(dest_dir).map_err(map_fs_error)?;
    if let Some(provider) = inner.deps.registry.for_path(&dest) {
        let meta = provider.metadata(&dest).await.map_err(map_fs_error)?;
        if !matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
            return Err(WidgetError::InvalidStateForOperation(
                "drop target is not a directory".into(),
            ));
        }
    } else {
        return Err(WidgetError::InvalidStateForOperation(
            "drop target unavailable".into(),
        ));
    }
    inner.copy_paths_into_directory(&sources, &dest).await?;
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Record a path in the recent-files list (files only).
pub async fn touch_recent(instance_id: Uuid, path: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let fp = orchid_fs::FsPath::new(path).map_err(map_fs_error)?;
    inner.record_recent(&fp);
    Ok(())
}

/// Navigate to a virtual folder by sidebar id.
pub async fn navigate_virtual(instance_id: Uuid, pane: u8, virtual_id: &str) -> WidgetResult<()> {
    // Map the UI ids from the sidebar to internal virtual paths.
    let path = match virtual_id {
        "fav:recent" => orchid_fs::FsPath::new("virtual:recent").ok(),
        "fav:starred" => orchid_fs::FsPath::new("virtual:starred").ok(),
        "fav:tags" => orchid_fs::FsPath::new("virtual:tags").ok(),
        "cat:images" => orchid_fs::FsPath::new("virtual:categories/images").ok(),
        "cat:documents" => orchid_fs::FsPath::new("virtual:categories/documents").ok(),
        "cat:video" => orchid_fs::FsPath::new("virtual:categories/video").ok(),
        "cat:audio" => orchid_fs::FsPath::new("virtual:categories/audio").ok(),
        "cat:archives" => orchid_fs::FsPath::new("virtual:categories/archives").ok(),
        "net:places" => orchid_fs::FsPath::new("virtual:network").ok(),
        other if other.starts_with("net:") && other != "net:places" => {
            let idx = other
                .strip_prefix("net:")
                .and_then(|s| s.parse::<usize>().ok());
            let inner = live_inner(instance_id)?;
            idx.and_then(|i| {
                inner
                    .enabled_network_mounts()
                    .into_iter()
                    .nth(i)
                    .and_then(|m| orchid_fs::normalize_mount_uri(&m.uri))
                    .and_then(|uri| orchid_fs::FsPath::new(&uri).ok())
            })
        }
        other if other.starts_with("managed:") => {
            let idx = other
                .strip_prefix("managed:")
                .and_then(|s| s.parse::<usize>().ok());
            let inner = live_inner(instance_id)?;
            let roots = inner.managed_roots.read();
            idx.and_then(|i| roots.get(i))
                .and_then(|p| orchid_fs::FsPath::new(p).ok())
        }
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

/// Refresh every live file-manager instance (e.g. after config hot-reload).
pub async fn refresh_all_instances() {
    for entry in FM_LIVE.iter() {
        let inner = Arc::clone(entry.value());
        tokio::spawn(async move {
            inner.refresh_all_tabs().await;
        });
    }
}

/// Notify every live file-manager instance that a managed file was ingested.
pub fn notify_managed_ingest(path: &orchid_fs::FsPath) {
    for entry in FM_LIVE.iter() {
        let inner = Arc::clone(entry.value());
        let path = path.clone();
        tokio::spawn(async move {
            inner.handle_managed_ingest(&path).await;
        });
    }
}

fn network_mount_display_name(m: &orchid_storage::NetworkMountConfig, uri: &str) -> String {
    if !m.name.trim().is_empty() {
        return m.name.trim().to_string();
    }
    orchid_fs::FsPath::new(uri)
        .ok()
        .and_then(|p| p.file_name().map(String::from))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| uri.to_string())
}
