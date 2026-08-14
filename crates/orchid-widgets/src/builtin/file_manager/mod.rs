//! File-manager widget.

pub mod archive;
pub mod batch_rename;
pub mod clipboard;
pub mod config;
pub mod context_menu;
pub mod find;
pub mod navigation;
pub mod selection;
pub mod state;
pub mod tools;
pub mod transfer;
pub mod view_mode;
pub mod virtual_folders;
pub mod visit_log;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::error::WidgetError;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{
    EntryPayload, FileManagerPayload, FmViewMode, ManagedFolderSidebarPayload, NetworkMountPayload,
    PanePayload, TabPayload, VisitHistoryItemPayload,
};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use clipboard::{ClipboardOperation, FileClipboard};
pub use config::{
    decode_persisted, ClickBehavior, FileManagerConfig, FileManagerPersisted, FileManagerSession,
    PersistedActivePane, PersistedPane, PersistedTab, SortBy, ThumbnailSize as FmThumbnailSize,
    ViewMode,
};
pub use context_menu::{
    build_for_selection, info_for_selection, ContextMenuInfo, ContextMenuInputs, ContextMenuItem,
};
pub use find::{is_search_virtual, search_session_id, FindSpec, SearchSession};
pub use navigation::{
    coerce_typed_path, complete_parent_and_prefix, drive_root, list_local_drives,
    list_mapped_network_shares, parse_net_use, BreadcrumbSegment, DriveItem, NavigationResult,
    Navigator, PathCompleteItem,
};
pub use selection::{parse_byte_size, MaskOp, SelectFilter, SelectionModel};
pub use state::{ActivePane, FileManagerState, PaneState, TabState};
pub use transfer::{
    apply_conflict, cancel_transfer, copy_to_other_pane, move_to_other_pane, pause_transfer,
    resume_transfer, ConflictChoice, TransferOptions,
};
pub use view_mode::{config_for_mode, ViewModeConfig};
pub use virtual_folders::{
    category_for_virtual_path, category_search_extensions, empty_placeholder_for_path,
    entry_matches_category, is_virtual, label_key_for_virtual_path, sidebar_catalog, FileCategory,
    VirtualFolder,
};
pub use visit_log::{PathVisit, VisitLog, VisitMenuItem};

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
    /// Open in the built-in viewer already in edit mode (F4).
    OpenInEditor {
        path: String,
    },
    /// Open the OS file-association / default-app settings for `path`.
    OpenFileAssociations {
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
    /// Prompt for a tag name to apply to `paths`.
    NeedsTag {
        paths: Vec<String>,
    },
    /// Prompt for a selection mask / filter (`+`, `-`, or Select by filter).
    NeedsSelectMask {
        op: MaskOp,
        filter: bool,
    },
    /// Prompt for a folder name under `parent`.
    NeedsCreateFolder {
        parent: String,
    },
    /// Prompt for a new file name under `parent`.
    NeedsCreateFile {
        parent: String,
    },
    /// Prompt for a passphrase to encrypt or reveal encrypted files.
    NeedsPassphrase {
        paths: Vec<String>,
        purpose: PassphrasePurpose,
    },
    /// Prompt for overwrite / skip / rename when a destination exists.
    NeedsConflict {
        source: String,
        dest: String,
        dest_name: String,
        can_resume: bool,
        is_copy: bool,
    },
    /// Prompt for a batch-rename pattern applied to `paths`.
    NeedsBatchRename {
        paths: Vec<String>,
    },
    /// Show a read-only report (checksums, compare, ACL).
    NeedsReport {
        title: String,
        body: String,
    },
    /// Prompt for a tool parameter (split size, chmod, ACL grant, …).
    NeedsToolPrompt {
        action_id: String,
        paths: Vec<String>,
        proposed: String,
        title: String,
        hint: String,
    },
    /// Open the Find Files dialog for the current folder.
    NeedsFindDialog {
        root: String,
    },
    /// Navigate the active pane to a find-results virtual folder.
    NavigateSearch {
        path: String,
    },
    /// Show read-only managed-folder policy for `path`.
    NeedsManagedPolicy {
        path: String,
        policy: Option<orchid_fs::ManagedFolderPolicy>,
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
    /// Password for creating an encrypted archive.
    ArchiveCreate,
    /// Password to open / extract / test an archive.
    ArchiveOpen,
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
    /// Sidecar for runtime network-place bookmarks (`network-bookmarks.toml`).
    pub network_bookmarks_file: Option<std::path::PathBuf>,
    /// Application-wide recent-files list.
    pub recent_files: Arc<crate::recent_files::RecentFilesStore>,
    /// DPAPI-backed passphrase for Windows Hello unlock of encrypted files.
    pub fm_passphrase_vault: Arc<orchid_crypto::FmPassphraseVault>,
    /// Application-wide config (locale formatting, etc.).
    pub orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    /// Fluent locale for UI strings built inside the widget (e.g. context-menu info).
    pub locale: Arc<orchid_i18n::LocaleManager>,
    /// Optional directory watcher used to auto-refresh open folders.
    pub file_watcher: Option<Arc<orchid_fs::FileWatcher>>,
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
    ///
    /// Stored behind [`Arc`] so selection/filter snapshots clone the map of
    /// arcs instead of deep-cloning every [`orchid_fs::FsEntry`].
    entries_by_tab: RwLock<HashMap<Uuid, Arc<Vec<orchid_fs::FsEntry>>>>,
    /// Decoded image thumbnails keyed by entry path (icon / gallery modes).
    thumbnail_rgba: RwLock<HashMap<String, orchid_viewers::Thumbnail>>,
    /// Insertion order for thumbnail cache eviction.
    thumbnail_order: RwLock<VecDeque<String>>,
    /// OS shell icons keyed by entry path (list / details / icons when no image preview).
    shell_icon_rgba: RwLock<HashMap<String, orchid_viewers::Thumbnail>>,
    /// Insertion order for shell-icon cache eviction.
    shell_icon_order: RwLock<VecDeque<String>>,
    /// Cached managed-folder root paths for [`apply_entry_metadata`].
    managed_roots: RwLock<Vec<String>>,
    /// Cached ingest stats per managed root path.
    managed_stats: RwLock<std::collections::HashMap<String, orchid_fs::ManagedFolderStats>>,
    /// Cached policy per managed root path.
    managed_policies:
        RwLock<std::collections::HashMap<String, Option<orchid_fs::ManagedFolderPolicy>>>,
    /// Last ingested file name shown briefly in the status bar.
    ingest_notice: RwLock<Option<(String, std::time::Instant)>>,
    /// Managed ingest operations in progress (across all instances).
    ingest_in_flight: AtomicU32,
    /// File name currently being ingested (best-effort label).
    ingest_current: RwLock<Option<String>>,
    /// Cached encrypted paths for [`apply_entry_metadata`].
    encrypted_paths: RwLock<Vec<String>>,
    /// Last navigation error per tab (shown in the pane error banner).
    tab_errors: RwLock<std::collections::HashMap<Uuid, Option<String>>>,
    /// Copy/move progress for drag-and-drop and OS file drops.
    transfer: RwLock<TransferState>,
    /// Pause / cancel / queue for the active transfer.
    xfer: transfer::TransferCtl,
    /// Last failed transfer message (brief status-bar toast).
    transfer_notice: RwLock<Option<(String, std::time::Instant)>>,
    /// Tab ids currently loading a directory listing (navigation in flight).
    loading_tabs: RwLock<HashSet<Uuid>>,
    /// Visible filtered-entry window per pane: (first, end).
    viewport_by_pane: RwLock<HashMap<u8, (usize, usize)>>,
    /// Last passphrase failure (brief status-bar toast while dialog is open).
    passphrase_error: RwLock<Option<(String, std::time::Instant)>>,
    /// Last managed ingest failure (file name for status-bar toast).
    ingest_error: RwLock<Option<(String, std::time::Instant)>>,
    /// Brief success notice (`i18n` key + optional name argument).
    activity_notice_key: RwLock<Option<String>>,
    activity_notice_name: RwLock<Option<String>>,
    activity_notice_at: RwLock<Option<std::time::Instant>>,
    /// Active directory watches keyed by tab id (drop handle to unsubscribe).
    watch_handles: parking_lot::Mutex<HashMap<Uuid, orchid_fs::WatchHandle>>,
    /// Watched directory path per tab (for filtering bus FS events).
    watch_paths: RwLock<HashMap<Uuid, String>>,
    /// Bus subscriptions that drive external directory refresh.
    dir_watch_subs: parking_lot::Mutex<Vec<orchid_core::SubscriptionHandle>>,
    /// Generation counter for coalescing external refresh tasks.
    external_refresh_gen: AtomicU64,
    /// Folder visit counts for the history dropdown.
    visit_log: parking_lot::Mutex<VisitLog>,
    /// Runtime find / duplicate / large-file result sets (`virtual:search/<id>`).
    search_sessions: RwLock<HashMap<Uuid, find::SearchSession>>,
    bus: Arc<orchid_core::EventBus>,
}

#[derive(Debug, Clone, Default)]
struct TransferState {
    active: bool,
    is_copy: bool,
    current_name: String,
    processed_bytes: u64,
    total_bytes: u64,
    last_publish: Option<std::time::Instant>,
    paused: bool,
    queue_len: u32,
}

/// Options for [`FileManagerInner::refresh_all_tabs_with_opts`].
#[derive(Debug, Clone, Copy)]
struct RefreshOpts {
    publish: bool,
    indicate_loading: bool,
}

const SHELL_ICON_CACHE_CAP: usize = 1024;
const THUMBNAIL_CACHE_CAP: usize = 256;

fn shell_icon_size_for_mode(mode: ViewMode) -> orchid_fs::ShellIconSize {
    match mode {
        ViewMode::Gallery => orchid_fs::ShellIconSize::Jumbo,
        ViewMode::Icons => orchid_fs::ShellIconSize::ExtraLarge,
        ViewMode::List | ViewMode::Details => orchid_fs::ShellIconSize::Small,
    }
}

fn shell_icon_cache_key(path: &str, size: orchid_fs::ShellIconSize) -> String {
    format!("{path}\x1e{}", size.pixels())
}

fn insert_capped_thumbnail(
    map: &mut HashMap<String, orchid_viewers::Thumbnail>,
    order: &mut VecDeque<String>,
    key: String,
    value: orchid_viewers::Thumbnail,
    cap: usize,
) {
    if !map.contains_key(&key) {
        while map.len() >= cap {
            if let Some(old) = order.pop_front() {
                map.remove(&old);
            } else {
                break;
            }
        }
        order.push_back(key.clone());
    }
    map.insert(key, value);
}

impl FileManagerWidget {
    /// Build a widget rooted at `initial_path`.
    pub fn new(
        instance_id: Uuid,
        deps: FileManagerDeps,
        bus: Arc<orchid_core::EventBus>,
        initial_path: orchid_fs::FsPath,
    ) -> Self {
        Self::from_persisted(
            instance_id,
            deps,
            bus,
            FileManagerPersisted {
                config: FileManagerConfig::default(),
                session: None,
                path_visits: Vec::new(),
            },
            initial_path,
        )
    }

    /// Build from decoded persisted config/session.
    pub fn from_persisted(
        instance_id: Uuid,
        deps: FileManagerDeps,
        bus: Arc<orchid_core::EventBus>,
        persisted: FileManagerPersisted,
        fallback_path: orchid_fs::FsPath,
    ) -> Self {
        let config = persisted.config;
        let state = state_from_persisted(&config, persisted.session.as_ref(), fallback_path);
        let navigator = Arc::new(Navigator::new(deps.registry.clone()));
        Self {
            inner: Arc::new(FileManagerInner {
                instance_id,
                deps,
                navigator,
                state: parking_lot::Mutex::new(state),
                config: RwLock::new(config),
                entries_by_tab: RwLock::new(HashMap::new()),
                thumbnail_rgba: RwLock::new(HashMap::new()),
                thumbnail_order: RwLock::new(VecDeque::new()),
                shell_icon_rgba: RwLock::new(HashMap::new()),
                shell_icon_order: RwLock::new(VecDeque::new()),
                managed_roots: RwLock::new(Vec::new()),
                managed_stats: RwLock::new(HashMap::new()),
                managed_policies: RwLock::new(HashMap::new()),
                ingest_notice: RwLock::new(None),
                ingest_in_flight: AtomicU32::new(0),
                ingest_current: RwLock::new(None),
                encrypted_paths: RwLock::new(Vec::new()),
                tab_errors: RwLock::new(HashMap::new()),
                transfer: RwLock::new(TransferState::default()),
                xfer: transfer::TransferCtl::default(),
                transfer_notice: RwLock::new(None),
                loading_tabs: RwLock::new(HashSet::new()),
                viewport_by_pane: RwLock::new(HashMap::new()),
                passphrase_error: RwLock::new(None),
                ingest_error: RwLock::new(None),
                activity_notice_key: RwLock::new(None),
                activity_notice_name: RwLock::new(None),
                activity_notice_at: RwLock::new(None),
                watch_handles: parking_lot::Mutex::new(HashMap::new()),
                watch_paths: RwLock::new(HashMap::new()),
                dir_watch_subs: parking_lot::Mutex::new(Vec::new()),
                external_refresh_gen: AtomicU64::new(0),
                visit_log: parking_lot::Mutex::new(VisitLog::from_entries(persisted.path_visits)),
                search_sessions: RwLock::new(HashMap::new()),
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
            let right = state.right_pane.as_ref().map(|p| p.active_tab().clone());
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
        let tab = {
            let mut state = self.inner.state.lock();
            let pane = match state.active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            let changed = state.active_tab_mut().navigate_to(path);
            self.inner.reset_pane_viewport(pane);
            let tab = state.active_tab().clone();
            if changed {
                self.inner.record_visit(&tab.path);
            }
            tab
        };
        self.inner
            .refresh_tabs_with_opts(
                &[tab],
                RefreshOpts {
                    publish: true,
                    indicate_loading: true,
                },
            )
            .await
    }

    /// Back one step in history.
    pub async fn go_back(&self) {
        let tab = {
            let mut state = self.inner.state.lock();
            let pane = match state.active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            if !state.active_tab_mut().back() {
                return;
            }
            self.inner.reset_pane_viewport(pane);
            let tab = state.active_tab().clone();
            self.inner.record_visit(&tab.path);
            tab
        };
        self.inner
            .refresh_tabs_with_opts(
                &[tab],
                RefreshOpts {
                    publish: true,
                    indicate_loading: true,
                },
            )
            .await;
    }

    /// Forward one step in history.
    pub async fn go_forward(&self) {
        let tab = {
            let mut state = self.inner.state.lock();
            let pane = match state.active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            if !state.active_tab_mut().forward() {
                return;
            }
            self.inner.reset_pane_viewport(pane);
            let tab = state.active_tab().clone();
            self.inner.record_visit(&tab.path);
            tab
        };
        self.inner
            .refresh_tabs_with_opts(
                &[tab],
                RefreshOpts {
                    publish: true,
                    indicate_loading: true,
                },
            )
            .await;
    }

    /// Change the current tab's view mode.
    pub fn set_view_mode(&self, mode: ViewMode) {
        // view-mode change doesn't require re-listing.
        // We keep it sync but still publish snapshot updated.
        {
            // state is async; keep a best-effort try_lock by spawning.
            let inner = Arc::clone(&self.inner);
            tokio::spawn(async move {
                let tab = {
                    let mut state = inner.state.lock();
                    state.active_tab_mut().view_mode = mode;
                    state.active_tab().clone()
                };
                inner.publish_refresh();
                inner.spawn_view_decorations(tab);
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
        self.inner.install_dir_watch_handlers();
        // Directory listing deferred to on_activate (visibility-gated).
        Ok(())
    }
    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh().await;
        Ok(())
    }
    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }
    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }
    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.inner.clear_dir_watches();
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
        let left_pane = build_pane_payload(&state.left_pane, 0, &entries_map, &config, &self.inner);
        let dual_pane = config.dual_pane;
        let mut panes = vec![left_pane];
        if dual_pane {
            if let Some(right) = &state.right_pane {
                panes.push(build_pane_payload(
                    right,
                    1,
                    &entries_map,
                    &config,
                    &self.inner,
                ));
            }
        }
        let active_pane = match state.active_pane {
            ActivePane::Left => 0,
            ActivePane::Right => 1,
        };
        let tab = state.active_tab();
        let (clipboard_count, clipboard_is_cut) = self.inner.deps.clipboard.display_state();

        Some(WidgetSnapshot {
            instance_id: self.inner.instance_id,
            widget_type: TYPE_ID,
            title: tab.path.as_str().to_string(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::FileManager({
                let transfer = self.inner.transfer.read().clone();
                FileManagerPayload {
                    panes,
                    active_pane,
                    dual_pane,
                    clipboard_count,
                    clipboard_is_cut,
                    managed_folders: self.inner.managed_folder_payloads(),
                    network_mounts: self.inner.network_mount_payloads(),
                    activity_indicator: self.inner.activity_indicator_label(),
                    ingest_in_flight: self.inner.ingest_in_flight.load(Ordering::Relaxed),
                    transfer_active: transfer.active,
                    transfer_progress: if transfer.active && transfer.total_bytes > 0 {
                        (transfer.processed_bytes as f32 / transfer.total_bytes as f32).min(1.0)
                    } else {
                        0.0
                    },
                    transfer_is_copy: transfer.is_copy,
                    transfer_paused: transfer.paused,
                    transfer_queue: transfer.queue_len,
                    transfer_current: if transfer.active {
                        Some(transfer.current_name.clone())
                    } else {
                        None
                    },
                    transfer_error: self.inner.transfer_error_label(),
                    passphrase_error: self.inner.passphrase_error_label(),
                    ingest_error: self.inner.ingest_error_label(),
                    activity_notice_key: self.inner.activity_notice_key(),
                    activity_notice_name: self.inner.activity_notice_name(),
                    visit_history: self.inner.visit_history_payload(),
                }
            }),
        })
    }
    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let persisted = FileManagerPersisted {
            config: self.inner.config.read().clone(),
            session: Some(session_from_state(&self.inner.state.lock())),
            path_visits: self.inner.visit_log.lock().entries().to_vec(),
        };
        state_codec::save_state(&persisted)
    }
    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let persisted = decode_persisted(bytes)?;
        *self.inner.config.write() = persisted.config.clone();
        *self.inner.visit_log.lock() = VisitLog::from_entries(persisted.path_visits);
        *self.inner.state.lock() = state_from_persisted(
            &persisted.config,
            persisted.session.as_ref(),
            default_initial_path(),
        );
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

    fn reset_pane_viewport(&self, pane: u8) {
        self.viewport_by_pane.write().remove(&pane);
    }

    fn record_visit(&self, path: &orchid_fs::FsPath) {
        self.visit_log.lock().record(path.as_str());
    }

    fn visit_history_payload(&self) -> Vec<VisitHistoryItemPayload> {
        self.visit_log
            .lock()
            .menu_items()
            .into_iter()
            .map(|item| VisitHistoryItemPayload {
                path: item.path,
                frequent: item.frequent,
                is_header: item.is_header,
            })
            .collect()
    }

    fn install_dir_watch_handlers(self: &Arc<Self>) {
        if self.deps.file_watcher.is_none() {
            return;
        }
        use orchid_core::{Event, EventFilter, HandlerPriority};
        use orchid_fs::{FsCreatedEvent, FsDeletedEvent, FsModifiedEvent, FsRenamedEvent};

        let filter = EventFilter::default()
            .add_type(FsCreatedEvent::event_type())
            .add_type(FsModifiedEvent::event_type())
            .add_type(FsDeletedEvent::event_type())
            .add_type(FsRenamedEvent::event_type());
        let this = Arc::downgrade(self);
        match self
            .bus
            .subscribe_async(filter, HandlerPriority::Normal, move |env| {
                let this = this.clone();
                async move {
                    let Some(inner) = this.upgrade() else {
                        return;
                    };
                    for path in fs_event_paths(&env) {
                        inner.schedule_external_refresh(&path);
                    }
                }
            }) {
            Ok(handle) => {
                self.dir_watch_subs.lock().push(handle);
            }
            Err(e) => {
                warn!(error = %e, "fm: failed to subscribe to directory watch events");
            }
        }
    }

    fn clear_dir_watches(&self) {
        self.watch_handles.lock().clear();
        self.watch_paths.write().clear();
        self.dir_watch_subs.lock().clear();
    }

    fn drop_tab_watch(&self, tab_id: Uuid) {
        self.watch_handles.lock().remove(&tab_id);
        self.watch_paths.write().remove(&tab_id);
    }

    async fn rewatch_tab(self: &Arc<Self>, tab: &TabState) {
        self.drop_tab_watch(tab.id);
        if is_virtual(&tab.path) {
            return;
        }
        let Some(watcher) = self.deps.file_watcher.as_ref() else {
            return;
        };
        // Non-recursive: the listing only needs sibling-entry events. Recursive
        // watches on home/OneDrive trees block startup for minutes.
        match watcher.watch(tab.path.clone(), false).await {
            Ok(handle) => {
                self.watch_paths
                    .write()
                    .insert(tab.id, tab.path.as_str().to_string());
                self.watch_handles.lock().insert(tab.id, handle);
            }
            Err(e) => {
                debug!(
                    error = %e,
                    path = %tab.path.as_str(),
                    "fm: directory watch unavailable"
                );
            }
        }
    }

    fn schedule_external_refresh(self: &Arc<Self>, path: &orchid_fs::FsPath) {
        let path_str = path.as_str();
        let affected: Vec<TabState> = {
            let watch_paths = self.watch_paths.read();
            let state = self.state.lock();
            let mut out = Vec::new();
            for (tab_id, root) in watch_paths.iter() {
                if !fs_event_affects_listing(path_str, root) {
                    continue;
                }
                if let Some(tab) = find_tab_by_id(&state, *tab_id) {
                    out.push(tab.clone());
                }
            }
            out
        };
        if affected.is_empty() {
            return;
        }
        let gen = self.external_refresh_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let this = Arc::clone(self);
        let changed_path = path.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            if this.external_refresh_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            let show_hidden = this.config.read().show_hidden;
            let mut need_publish = false;
            for tab in affected {
                // Prefer the live tab state in case the user navigated away.
                let live = {
                    let state = this.state.lock();
                    find_tab_by_id(&state, tab.id).cloned()
                };
                let Some(live) = live else {
                    continue;
                };
                if live.path != tab.path {
                    continue;
                }
                // Fast path: patch a single direct child without re-listing.
                if this
                    .try_incremental_entry_update(&live, &changed_path, show_hidden)
                    .await
                {
                    need_publish = true;
                    continue;
                }
                this.refresh_tab(&live, show_hidden).await;
                need_publish = true;
            }
            if need_publish {
                this.publish_refresh();
            }
        });
    }

    /// Patch a single listing entry for `changed` when it is a direct child of `tab.path`.
    /// Returns `true` when the listing was updated without a full re-list.
    async fn try_incremental_entry_update(
        self: &Arc<Self>,
        tab: &TabState,
        changed: &orchid_fs::FsPath,
        show_hidden: bool,
    ) -> bool {
        if is_virtual(&tab.path) || tab.branch_view {
            return false;
        }
        let Some(parent) = changed.parent() else {
            return false;
        };
        if parent.as_str() != tab.path.as_str() {
            // Nested noise under the open folder — keep coalesced full refresh.
            return false;
        }
        let Some(provider) = self.deps.registry.for_path(changed) else {
            return false;
        };
        let exists = match provider.exists(changed).await {
            Ok(v) => v,
            Err(_) => return false,
        };
        let mut entries = {
            let guard = self.entries_by_tab.read();
            guard
                .get(&tab.id)
                .map(|v| v.as_ref().clone())
                .unwrap_or_default()
        };
        let key = changed.as_str();
        if !exists {
            let before = entries.len();
            entries.retain(|e| e.path.as_str() != key);
            if entries.len() == before {
                return false;
            }
        } else {
            let Ok(meta) = provider.metadata(changed).await else {
                return false;
            };
            if !show_hidden && meta.hidden {
                entries.retain(|e| e.path.as_str() != key);
            } else {
                let name = changed
                    .file_name()
                    .map(String::from)
                    .unwrap_or_else(|| key.to_string());
                let mut entry = orchid_fs::FsEntry {
                    path: changed.clone(),
                    name,
                    metadata: meta,
                };
                let mut batch = vec![entry];
                self.apply_entry_metadata(&mut batch);
                entry = batch.remove(0);
                if let Some(slot) = entries.iter_mut().find(|e| e.path.as_str() == key) {
                    *slot = entry;
                } else {
                    entries.push(entry);
                }
                sort_entries(&mut entries, tab.sort_by, tab.sort_descending);
            }
        }
        self.entries_by_tab
            .write()
            .insert(tab.id, Arc::new(entries));
        true
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

    fn managed_folder_payloads(&self) -> Vec<ManagedFolderSidebarPayload> {
        let roots = self.managed_roots.read().clone();
        let stats = self.managed_stats.read();
        let policies = self.managed_policies.read();
        roots
            .into_iter()
            .map(|path| {
                let st = stats.get(&path);
                let policy = policies.get(&path).and_then(|p| p.as_ref());
                let files_tracked = st.map(|s| s.files_tracked as u32).unwrap_or(0);
                let dedup_bytes = st
                    .map(|s| s.logical_bytes.saturating_sub(s.physical_bytes))
                    .unwrap_or(0);
                ManagedFolderSidebarPayload {
                    path,
                    files_tracked,
                    dedup_bytes,
                    policy_max_bytes: policy.and_then(|p| p.max_size_bytes),
                    policy_retention_days: policy.and_then(|p| p.retention_days),
                    policy_exclude_count: policy
                        .map(|p| p.exclude_patterns.len() as u32)
                        .unwrap_or(0),
                }
            })
            .collect()
    }

    fn set_activity_notice(&self, key: &str, name: Option<String>) {
        *self.activity_notice_key.write() = Some(key.to_string());
        *self.activity_notice_name.write() = name;
        *self.activity_notice_at.write() = Some(std::time::Instant::now());
        self.publish_refresh();
    }

    fn activity_notice_key(&self) -> Option<String> {
        let at = *self.activity_notice_at.read();
        if at
            .map(|t| t.elapsed() < std::time::Duration::from_secs(8))
            .unwrap_or(false)
        {
            self.activity_notice_key.read().clone()
        } else {
            None
        }
    }

    fn activity_notice_name(&self) -> Option<String> {
        let at = *self.activity_notice_at.read();
        if at
            .map(|t| t.elapsed() < std::time::Duration::from_secs(8))
            .unwrap_or(false)
        {
            self.activity_notice_name.read().clone()
        } else {
            None
        }
    }

    fn transfer_error_label(&self) -> Option<String> {
        let notice = self.transfer_notice.read();
        if let Some((msg, at)) = notice.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(8) {
                return Some(msg.clone());
            }
        }
        None
    }

    fn set_transfer_notice(&self, message: String) {
        *self.transfer_notice.write() = Some((message, std::time::Instant::now()));
        self.publish_refresh();
    }

    fn passphrase_error_label(&self) -> Option<String> {
        let notice = self.passphrase_error.read();
        if let Some((msg, at)) = notice.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(8) {
                return Some(msg.clone());
            }
        }
        None
    }

    fn set_passphrase_error(&self, message: String) {
        *self.passphrase_error.write() = Some((message, std::time::Instant::now()));
        self.publish_refresh();
    }

    fn clear_passphrase_error(&self) {
        *self.passphrase_error.write() = None;
    }

    fn ingest_error_label(&self) -> Option<String> {
        let notice = self.ingest_error.read();
        if let Some((name, at)) = notice.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(8) {
                return Some(name.clone());
            }
        }
        None
    }

    fn set_ingest_error(&self, name: String) {
        *self.ingest_error.write() = Some((name, std::time::Instant::now()));
        self.publish_refresh();
    }

    fn clear_ingest_error(&self) {
        *self.ingest_error.write() = None;
    }

    fn activity_indicator_label(&self) -> Option<String> {
        if self.ingest_in_flight.load(Ordering::Relaxed) > 0 {
            return self.ingest_current.read().clone();
        }
        let notice = self.ingest_notice.read();
        if let Some((name, at)) = notice.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(8) {
                return Some(name.clone());
            }
        }
        None
    }

    fn handle_managed_ingest_started(&self, path: &orchid_fs::FsPath) {
        self.ingest_in_flight.fetch_add(1, Ordering::Relaxed);
        let label = path
            .file_name()
            .map(String::from)
            .unwrap_or_else(|| path.as_str().to_string());
        *self.ingest_current.write() = Some(label);
        self.publish_refresh();
    }

    fn handle_managed_ingest_finished(&self) {
        let prev = self.ingest_in_flight.fetch_sub(1, Ordering::Relaxed);
        if prev <= 1 {
            *self.ingest_current.write() = None;
        }
        self.publish_refresh();
    }

    fn handle_managed_ingest_failed(&self, path: &orchid_fs::FsPath) {
        self.handle_managed_ingest_finished();
        let name = path
            .file_name()
            .map(String::from)
            .unwrap_or_else(|| path.as_str().to_string());
        self.set_ingest_error(name);
    }

    async fn handle_managed_ingest(&self, path: &orchid_fs::FsPath) {
        self.handle_managed_ingest_finished();
        self.clear_ingest_error();
        let label = path
            .file_name()
            .map(String::from)
            .unwrap_or_else(|| path.as_str().to_string());
        *self.ingest_notice.write() = Some((label, std::time::Instant::now()));
        self.publish_refresh();
        self.refresh_managed_roots().await;
        self.publish_refresh();
    }

    fn begin_transfer(&self, is_copy: bool) {
        let queue_len = self.xfer.queue.lock().len() as u32;
        *self.transfer.write() = TransferState {
            active: true,
            is_copy,
            queue_len,
            ..TransferState::default()
        };
        self.publish_refresh();
    }

    fn apply_transfer_progress(&self, p: &orchid_fs::OperationProgress) {
        let name = p
            .current_path
            .file_name()
            .map(str::to_string)
            .unwrap_or_default();
        let should_publish = {
            let mut st = self.transfer.write();
            st.active = true;
            st.current_name = name;
            st.processed_bytes = p.processed_bytes;
            st.total_bytes = p.total_bytes;
            st.last_publish
                .map(|t| t.elapsed() >= std::time::Duration::from_millis(100))
                .unwrap_or(true)
        };
        if should_publish {
            self.transfer.write().last_publish = Some(std::time::Instant::now());
            self.publish_refresh();
        }
    }

    fn end_transfer(&self) {
        let queue_len = self.xfer.queue.lock().len() as u32;
        *self.transfer.write() = TransferState {
            queue_len,
            ..TransferState::default()
        };
        self.publish_refresh();
    }

    async fn refresh_all_tabs(self: &Arc<Self>) {
        self.refresh_all_tabs_with_opts(RefreshOpts {
            publish: true,
            indicate_loading: false,
        })
        .await;
    }

    async fn refresh_all_tabs_with_opts(self: &Arc<Self>, opts: RefreshOpts) {
        let tabs = {
            let state = self.state.lock();
            let mut tabs = vec![state.left_pane.active_tab().clone()];
            if let Some(right) = state.right_pane.as_ref() {
                tabs.push(right.active_tab().clone());
            }
            tabs
        };
        self.refresh_tabs_with_opts(&tabs, opts).await;
    }

    /// Re-list only the given tabs (e.g. the pane that just navigated).
    async fn refresh_tabs_with_opts(self: &Arc<Self>, tabs: &[TabState], opts: RefreshOpts) {
        if tabs.is_empty() {
            return;
        }
        self.refresh_managed_roots().await;
        self.refresh_encrypted_paths().await;
        let show_hidden = self.config.read().show_hidden;

        let loading_ids: Vec<Uuid> = tabs.iter().map(|t| t.id).collect();

        if opts.indicate_loading {
            {
                let mut loading = self.loading_tabs.write();
                for id in &loading_ids {
                    loading.insert(*id);
                }
            }
            {
                let mut entries = self.entries_by_tab.write();
                for id in &loading_ids {
                    entries.remove(id);
                }
            }
            self.publish_refresh();
        }

        for tab in tabs {
            self.refresh_tab(tab, show_hidden).await;
        }

        if opts.indicate_loading {
            let mut loading = self.loading_tabs.write();
            for id in &loading_ids {
                loading.remove(id);
            }
        }

        if opts.publish {
            self.publish_refresh();
        }
    }

    /// Re-sort a tab's already-loaded entries without touching the filesystem.
    fn resort_tab_in_memory(&self, tab_id: Uuid, sort_by: SortBy, descending: bool) {
        let mut entries = self.entries_by_tab.write();
        if let Some(list) = entries.get_mut(&tab_id) {
            sort_entries(Arc::make_mut(list), sort_by, descending);
        }
    }

    async fn delete_paths(&self, paths: &[String], to_recycle: Option<bool>) -> WidgetResult<()> {
        let recycle_paths: Vec<String> = paths
            .iter()
            .filter(|p| orchid_fs::is_recycle_item(p))
            .cloned()
            .collect();
        if !recycle_paths.is_empty() {
            orchid_fs::purge_recycle(&recycle_paths)
                .await
                .map_err(map_fs_error)?;
        }
        let rest: Vec<&String> = paths
            .iter()
            .filter(|p| !orchid_fs::is_recycle_item(p))
            .collect();
        if rest.is_empty() {
            return Ok(());
        }
        let registry = &self.deps.registry;
        let to_recycle_bin = to_recycle.unwrap_or_else(|| self.config.read().delete_to_recycle);
        let opts = orchid_fs::operations::delete::DeleteOptions {
            to_recycle_bin,
            recursive: !to_recycle_bin, // permanent deletes need recurse for folders
        };
        for p in rest {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            orchid_fs::operations::delete::delete(registry, &fp, opts)
                .await
                .map_err(map_fs_error)?;
        }
        Ok(())
    }

    async fn refresh_tab(self: &Arc<Self>, tab: &TabState, show_hidden: bool) {
        let path = tab.path.clone();
        let t0 = std::time::Instant::now();
        debug!(path = %path.as_str(), "fm refresh_tab start");

        let entries = if is_virtual(&path) {
            self.tab_errors.write().insert(tab.id, None);
            let mut entries = self.list_virtual(&path).await;
            sort_entries(&mut entries, tab.sort_by, tab.sort_descending);
            let entries = Arc::new(entries);
            self.entries_by_tab
                .write()
                .insert(tab.id, Arc::clone(&entries));
            entries
        } else if tab.branch_view {
            let result = self.navigator.list_branch(&path, show_hidden).await;
            self.tab_errors.write().insert(tab.id, result.error.clone());
            let mut entries = result.entries;
            self.apply_entry_metadata(&mut entries);
            sort_entries(&mut entries, tab.sort_by, tab.sort_descending);
            let entries = Arc::new(entries);
            self.entries_by_tab
                .write()
                .insert(tab.id, Arc::clone(&entries));
            entries
        } else {
            let result = self.navigator.navigate(&path, show_hidden).await;
            self.tab_errors.write().insert(tab.id, result.error.clone());
            let mut entries = result.entries;
            self.apply_entry_metadata(&mut entries);
            sort_entries(&mut entries, tab.sort_by, tab.sort_descending);
            let entries = Arc::new(entries);
            self.entries_by_tab
                .write()
                .insert(tab.id, Arc::clone(&entries));
            entries
        };

        debug!(
            path = %path.as_str(),
            entries = entries.len(),
            elapsed_ms = t0.elapsed().as_millis(),
            "fm refresh_tab listed"
        );

        // Watch + icons/thumbs off the critical path so restore/bootstrap can
        // finish and the main window can open without waiting on notify.
        let this = Arc::clone(self);
        let tab = tab.clone();
        tokio::spawn(async move {
            this.rewatch_tab(&tab).await;
            this.ensure_shell_icons(&tab, entries.as_slice()).await;
            let mode_cfg = config_for_mode(tab.view_mode, 1.0);
            if mode_cfg.show_thumbnails {
                this.ensure_thumbnails(&tab, entries.as_slice()).await;
            }
        });
    }

    fn record_recent(&self, path: &orchid_fs::FsPath) {
        self.deps.recent_files.touch(path, Some(&self.bus));
    }

    fn collect_catalog_candidates(&self) -> Vec<orchid_fs::FsPath> {
        let mut paths: Vec<orchid_fs::FsPath> =
            self.deps.tag_manager.starred_paths().unwrap_or_default();
        paths.extend(
            self.deps
                .recent_files
                .paths()
                .into_iter()
                .filter_map(|p| orchid_fs::FsPath::new(&p).ok()),
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
        let guard = self.entries_by_tab.read();
        let Some(entries) = guard.get(&tab.id) else {
            return Vec::new();
        };
        let quick = tab.quick_filter.trim();
        if quick.is_empty() {
            return entries
                .iter()
                .map(|e| e.path.as_str().to_string())
                .collect();
        }
        let q = quick.to_lowercase();
        entries
            .iter()
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

    fn invert_selection_in_pane(&self, pane: u8) {
        self.mutate_selection_in_pane(pane, |sel, paths, _| sel.invert(paths));
    }

    fn apply_filter_in_pane(&self, pane: u8, op: MaskOp, filter: &SelectFilter) {
        self.mutate_selection_in_pane(pane, |sel, paths, entries| {
            sel.apply_matching(paths, op, |p| {
                entries
                    .iter()
                    .find(|e| e.path.as_str() == p)
                    .is_some_and(|e| selection::entry_matches_filter(e, filter))
            });
        });
    }

    fn select_index_range_in_pane(
        &self,
        pane: u8,
        from: usize,
        to: usize,
        additive: bool,
        columns: usize,
    ) {
        self.mutate_selection_in_pane(pane, |sel, paths, _| {
            sel.select_index_rect(paths, from, to, columns, additive);
        });
    }

    fn mutate_selection_in_pane(
        &self,
        pane: u8,
        f: impl FnOnce(&mut SelectionModel, &[String], &[orchid_fs::FsEntry]),
    ) {
        let (tab_id, paths, entries) = {
            let state = self.state.lock();
            let tab = match active_tab_ref(&state, pane) {
                Ok(t) => t,
                Err(_) => return,
            };
            let entries = self
                .entries_by_tab
                .read()
                .get(&tab.id)
                .cloned()
                .unwrap_or_else(|| Arc::new(Vec::new()));
            (tab.id, self.filtered_paths_for_tab(tab), entries)
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
            f(&mut t.selection, &paths, entries.as_slice());
        }
    }

    fn selection_bytes_in_tab(&self, tab: &TabState) -> u64 {
        let Some(entries) = self.entries_by_tab.read().get(&tab.id).cloned() else {
            return 0;
        };
        entries
            .iter()
            .filter(|e| tab.selection.is_selected(e.path.as_str()))
            .map(|e| e.metadata.size)
            .sum()
    }

    fn move_selection_in_pane(&self, pane: u8, delta: i32, extend: bool) {
        let (tab_id, ordered) = {
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
            t.selection.select_relative(&ordered, delta, extend);
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
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains(':') {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-invalid-folder-name".into(),
            ));
        }
        let new_path = parent.join(name);
        let provider =
            self.deps.registry.for_path(parent).ok_or_else(|| {
                WidgetError::InvalidStateForOperation("fm-no-provider-parent".into())
            })?;
        provider
            .create_dir(&new_path, false)
            .await
            .map_err(map_fs_error)?;
        Ok(())
    }

    async fn create_file_at(&self, parent: &orchid_fs::FsPath, name: &str) -> WidgetResult<()> {
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains(':') {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-invalid-folder-name".into(),
            ));
        }
        let new_path = parent.join(name);
        let provider =
            self.deps.registry.for_path(parent).ok_or_else(|| {
                WidgetError::InvalidStateForOperation("fm-no-provider-parent".into())
            })?;
        if provider.metadata(&new_path).await.is_ok() {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-invalid-folder-name".into(),
            ));
        }
        provider.write(&new_path, &[]).await.map_err(map_fs_error)?;
        Ok(())
    }

    fn spawn_view_decorations(self: &Arc<Self>, tab: TabState) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let Some(entries) = this.entries_by_tab.read().get(&tab.id).cloned() else {
                return;
            };
            this.ensure_shell_icons(&tab, entries.as_slice()).await;
            if config_for_mode(tab.view_mode, 1.0).show_thumbnails {
                this.ensure_thumbnails(&tab, entries.as_slice()).await;
            }
        });
    }

    async fn ensure_shell_icons(&self, tab: &TabState, entries: &[orchid_fs::FsEntry]) {
        let size = shell_icon_size_for_mode(tab.view_mode);

        let mut pending = Vec::new();
        {
            let cache = self.shell_icon_rgba.read();
            for e in entries.iter().take(256) {
                let path_key = e.path.as_str().to_string();
                let cache_key = shell_icon_cache_key(&path_key, size);
                if cache.contains_key(&cache_key) {
                    continue;
                }
                let path = e.path.clone();
                let is_dir = matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory);
                pending.push((path_key, path, is_dir));
            }
        }
        if pending.is_empty() {
            return;
        }

        // Shell extraction serializes on a process-wide lock, but scheduling
        // many spawn_blocking tasks still overlaps with cache hits / scheduling.
        // Publish once after the full icon pass so the UI is not rebuilt per batch.
        const BATCH: usize = 24;
        let mut any_hit = false;
        for chunk in pending.chunks(BATCH) {
            let futs = chunk.iter().map(|(path_key, path, is_dir)| {
                let path_key = path_key.clone();
                let path = path.clone();
                let is_dir = *is_dir;
                async move {
                    let icon = tokio::task::spawn_blocking(move || {
                        orchid_fs::shell_icon(&path, is_dir, size)
                    })
                    .await
                    .ok()
                    .flatten();
                    (path_key, icon)
                }
            });
            let results = futures::future::join_all(futs).await;
            {
                let mut cache = self.shell_icon_rgba.write();
                let mut order = self.shell_icon_order.write();
                for (path_key, icon) in results {
                    let Some(icon) = icon else {
                        continue;
                    };
                    insert_capped_thumbnail(
                        &mut cache,
                        &mut order,
                        shell_icon_cache_key(&path_key, size),
                        orchid_viewers::Thumbnail {
                            rgba: icon.rgba,
                            width: icon.width,
                            height: icon.height,
                        },
                        SHELL_ICON_CACHE_CAP,
                    );
                    any_hit = true;
                }
            }
        }
        if any_hit {
            self.publish_refresh();
        }
    }

    async fn ensure_thumbnails(&self, tab: &TabState, entries: &[orchid_fs::FsEntry]) {
        let mode_cfg = config_for_mode(tab.view_mode, 1.0);
        if !mode_cfg.show_thumbnails {
            return;
        }
        let thumb_size = viewer_thumb_size(self.config.read().thumbnail_size);

        let mut pending = Vec::new();
        {
            let cache = self.thumbnail_rgba.read();
            for e in entries.iter().take(64) {
                if !is_image_entry(e) {
                    continue;
                }
                let path_key = e.path.as_str().to_string();
                if cache.contains_key(&path_key) {
                    continue;
                }
                let modified_ms = e
                    .metadata
                    .modified
                    .map(|t| t.timestamp_millis())
                    .unwrap_or(0);
                let key = orchid_viewers::ThumbnailService::cache_key(&e.path, modified_ms);
                pending.push((path_key, e.path.clone(), key));
            }
        }
        if pending.is_empty() {
            return;
        }

        const CONCURRENCY: usize = 4;
        let mut any_thumb = false;
        for chunk in pending.chunks(CONCURRENCY) {
            let futs = chunk.iter().map(|(path_key, path, key)| {
                let path_key = path_key.clone();
                let path = path.clone();
                let key = *key;
                let thumbs = Arc::clone(&self.deps.thumbnails);
                let registry = Arc::clone(&self.deps.registry);
                async move {
                    if let Ok(Some(thumb)) = thumbs.get_cached(&key, thumb_size).await {
                        return Some((path_key, thumb));
                    }
                    // Local files: mmap decode avoids a full Vec copy.
                    if path.scheme() == "local" {
                        if let Ok(os_path) = path.to_local() {
                            match thumbs
                                .generate_from_local_path(key, thumb_size, os_path)
                                .await
                            {
                                Ok(thumb) => return Some((path_key, thumb)),
                                Err(_) => return None,
                            }
                        }
                    }
                    let provider = registry.for_path(&path)?;
                    let bytes = match provider.read(&path).await {
                        Ok(b) if b.len() <= 16 * 1024 * 1024 => b,
                        _ => return None,
                    };
                    match thumbs
                        .generate_from_image_bytes(key, thumb_size, bytes)
                        .await
                    {
                        Ok(thumb) => Some((path_key, thumb)),
                        Err(_) => None,
                    }
                }
            });
            let results = futures::future::join_all(futs).await;
            {
                let mut cache = self.thumbnail_rgba.write();
                let mut order = self.thumbnail_order.write();
                for (path_key, thumb) in results.into_iter().flatten() {
                    insert_capped_thumbnail(
                        &mut cache,
                        &mut order,
                        path_key,
                        thumb,
                        THUMBNAIL_CACHE_CAP,
                    );
                    any_thumb = true;
                }
            }
        }
        if any_thumb {
            self.publish_refresh();
        }
    }

    async fn list_virtual(&self, path: &orchid_fs::FsPath) -> Vec<orchid_fs::FsEntry> {
        let raw = path.as_str();
        if raw == "virtual:recent" {
            let mut entries: Vec<orchid_fs::FsEntry> = self
                .deps
                .recent_files
                .paths()
                .into_iter()
                .take(50)
                .filter_map(|p| orchid_fs::FsPath::new(&p).ok())
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
            let paths = self.deps.tag_manager.starred_paths().unwrap_or_default();
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
        if raw == "virtual:search" {
            return self.list_search_sessions();
        }
        if orchid_fs::is_recycle_listing(raw) {
            return self.list_recycle_bin().await;
        }
        if let Some(id) = find::search_session_id(raw) {
            return self
                .search_sessions
                .read()
                .get(&id)
                .map(|s| s.entries.clone())
                .unwrap_or_default();
        }
        Vec::new()
    }

    async fn list_recycle_bin(&self) -> Vec<orchid_fs::FsEntry> {
        match orchid_fs::list_recycle().await {
            Ok(items) => orchid_fs::recycle_entries(&items),
            Err(e) => {
                warn!(error = %e, "recycle bin list failed");
                Vec::new()
            }
        }
    }

    fn list_search_sessions(&self) -> Vec<orchid_fs::FsEntry> {
        let mut sessions: Vec<find::SearchSession> = self
            .search_sessions
            .read()
            .values()
            .filter(|s| s.saved)
            .cloned()
            .collect();
        sessions.sort_by(|a, b| a.label.cmp(&b.label));
        sessions
            .into_iter()
            .filter_map(|s| {
                let path = orchid_fs::FsPath::new(s.virtual_path()).ok()?;
                Some(orchid_fs::FsEntry {
                    name: s.label,
                    path,
                    metadata: orchid_fs::FsMetadata {
                        kind: orchid_fs::FsEntryKind::Directory,
                        size: s.entries.len() as u64,
                        created: None,
                        modified: None,
                        accessed: None,
                        readonly: false,
                        hidden: false,
                        system: false,
                        mime: None,
                        extended: orchid_fs::ExtendedAttributes::default(),
                    },
                })
            })
            .collect()
    }

    fn list_network_mounts(&self) -> Vec<orchid_fs::FsEntry> {
        let mut entries: Vec<orchid_fs::FsEntry> = self
            .enabled_network_mounts()
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
            .collect();
        for share in list_mapped_network_shares() {
            if entries.iter().any(|e| e.path.as_str() == share.path) {
                continue;
            }
            let Ok(path) = orchid_fs::FsPath::new(&share.path) else {
                continue;
            };
            entries.push(orchid_fs::FsEntry {
                name: share.label,
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
            });
        }
        entries
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
        if entries.is_empty() {
            return;
        }
        let encrypted_paths = self.encrypted_paths.read().clone();
        let managed_roots = self.managed_roots.read().clone();
        let paths: Vec<orchid_fs::FsPath> = entries.iter().map(|e| e.path.clone()).collect();
        let tags = self.deps.tag_manager.get_many(&paths).unwrap_or_default();
        for e in entries.iter_mut() {
            if let Some(tag) = tags.get(e.path.as_str()) {
                e.metadata.extended.starred = tag.starred;
                e.metadata.extended.tags = tag.tags.clone();
                e.metadata.extended.color_label = tag.color_label;
            }
            let path_str = e.path.as_str();
            if managed_roots.iter().any(|root| path_str.starts_with(root)) {
                e.metadata.extended.is_managed = true;
            }
            let covered = encrypted_paths
                .iter()
                .any(|p| path_str == p || path_str.starts_with(p));
            if covered {
                e.metadata.extended.is_encrypted = true;
                continue;
            }
            // Cheap extension check for files; only directories pay the marker FS probe.
            if orchid_fs::encrypted::marker::looks_encrypted(&e.path)
                || (matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory)
                    && orchid_fs::encrypted::marker::looks_encrypted_directory(&e.path))
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
        self.encrypted_paths
            .read()
            .iter()
            .any(|p| path.as_str() == p || path.as_str().starts_with(p))
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
                "fm-encryption-unavailable".into(),
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
        self.refresh_encrypted_paths().await;
        let name = paths
            .first()
            .and_then(|p| p.rsplit(['/', '\\']).next())
            .unwrap_or("files")
            .to_string();
        self.set_activity_notice("fm-encrypted", Some(name));
        Ok(())
    }

    async fn decrypt_paths(&self, paths: &[String], passphrase: &str) -> WidgetResult<()> {
        let Some(engine) = self.deps.encrypted.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-encryption-unavailable".into(),
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
        self.refresh_encrypted_paths().await;
        let name = paths
            .first()
            .and_then(|p| p.rsplit(['/', '\\']).next())
            .unwrap_or("files")
            .to_string();
        self.set_activity_notice("fm-decrypted", Some(name));
        Ok(())
    }

    async fn reveal_paths(&self, paths: &[String], passphrase: &str) -> WidgetResult<Vec<String>> {
        let Some(engine) = self.deps.encrypted.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-encryption-unavailable".into(),
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
        let name = paths
            .first()
            .and_then(|p| p.rsplit(['/', '\\']).next())
            .unwrap_or("files")
            .to_string();
        self.set_activity_notice("fm-revealed", Some(name));
        Ok(revealed)
    }

    async fn refresh_managed_roots(&self) {
        let mut roots = Vec::new();
        let mut stats = std::collections::HashMap::new();
        let mut policies = std::collections::HashMap::new();
        if let Some(engine) = self.deps.managed.as_ref() {
            if let Ok(folders) = engine.list_folders().await {
                for f in folders.into_iter().filter(|f| f.enabled) {
                    let key = f.path.as_str().to_string();
                    policies.insert(key.clone(), f.policy.clone());
                    roots.push(key.clone());
                    if let Ok(st) = engine.folder_stats(&f.path).await {
                        stats.insert(key, st);
                    }
                }
            }
        }
        *self.managed_roots.write() = roots;
        *self.managed_stats.write() = stats;
        *self.managed_policies.write() = policies;
    }

    fn managed_root_for_path(&self, path: &str) -> Option<String> {
        self.managed_roots
            .read()
            .iter()
            .find(|root| path.starts_with(root.as_str()))
            .cloned()
    }

    async fn register_managed_folder(&self, folder: &orchid_fs::FsPath) -> WidgetResult<()> {
        let Some(engine) = self.deps.managed.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-managed-unavailable".into(),
            ));
        };
        let cfg = orchid_fs::ManagedFolderConfig {
            path: folder.clone(),
            chunk_size: orchid_crypto::ChunkerConfig::default(),
            enabled: true,
            auto_ingest: true,
            policy: None,
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
        self.set_activity_notice("fm-managed-added", None);
        Ok(())
    }

    async fn remove_selection_from_managed(&self, paths: &[String]) -> WidgetResult<()> {
        let Some(engine) = self.deps.managed.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-managed-unavailable".into(),
            ));
        };
        for p in paths {
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            engine.remove_folder(&fp).await.map_err(map_fs_error)?;
        }
        self.refresh_managed_roots().await;
        self.set_activity_notice("fm-managed-removed", None);
        Ok(())
    }

    async fn resolve_managed_folder_target(
        &self,
        paths: &[String],
    ) -> WidgetResult<orchid_fs::FsPath> {
        if paths.is_empty() {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-managed-no-selection".into(),
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
            let parent = fp.parent().ok_or_else(|| {
                WidgetError::InvalidStateForOperation("fm-no-parent-folder".into())
            })?;
            folder_candidates.push(parent);
        }
        let first = folder_candidates[0].as_str();
        if !folder_candidates.iter().all(|f| f.as_str() == first) {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-selection-multiple-folders".into(),
            ));
        }
        Ok(folder_candidates[0].clone())
    }
}

fn build_pane_payload(
    pane: &PaneState,
    pane_idx: u8,
    entries_map: &HashMap<Uuid, Arc<Vec<orchid_fs::FsEntry>>>,
    config: &FileManagerConfig,
    inner: &FileManagerInner,
) -> PanePayload {
    let tabs: Vec<TabPayload> = pane
        .tabs
        .iter()
        .enumerate()
        .map(|(tab_idx, tab)| {
            let entries = entries_map
                .get(&tab.id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let is_active_tab = tab_idx == pane.active_tab;
            build_tab_payload(tab, entries, config, inner, pane_idx, is_active_tab)
        })
        .collect();
    PanePayload {
        tabs,
        active_tab: pane.active_tab as u32,
    }
}

fn entry_display_name(name: &str, is_dir: bool, show_extensions: bool) -> String {
    if show_extensions || is_dir {
        return name.to_string();
    }
    match name.rfind('.') {
        Some(0) | None => name.to_string(),
        Some(i) => name[..i].to_string(),
    }
}

fn build_tab_payload(
    tab: &TabState,
    entries: &[orchid_fs::FsEntry],
    config: &FileManagerConfig,
    inner: &FileManagerInner,
    pane_idx: u8,
    is_active_tab: bool,
) -> TabPayload {
    let raw_path = tab.path.as_str();
    let (path_display, breadcrumbs) = if raw_path == "virtual:recycle" {
        let label = inner.deps.locale.tr("fm-virtual-recycle");
        (raw_path.to_string(), vec![(raw_path.to_string(), label)])
    } else if raw_path == "virtual:search" {
        (
            raw_path.to_string(),
            vec![(raw_path.to_string(), "Search".to_string())],
        )
    } else if let Some(id) = find::search_session_id(raw_path) {
        let label = inner
            .search_sessions
            .read()
            .get(&id)
            .map(|s| s.label.clone())
            .unwrap_or_else(|| "Search".to_string());
        (
            label.clone(),
            vec![
                ("virtual:search".to_string(), "Search".to_string()),
                (raw_path.to_string(), label),
            ],
        )
    } else {
        let crumbs = inner.navigator.breadcrumbs_for(&tab.path);
        (
            raw_path.to_string(),
            crumbs
                .into_iter()
                .map(|c| (c.path.as_str().to_string(), c.display_name))
                .collect(),
        )
    };

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

    let item_count = entries_filtered.len() as u32;
    // Format only the visible window (+ overscan already applied by UI).
    const DEFAULT_WINDOW: usize = 96;
    let stored = if is_active_tab {
        inner.viewport_by_pane.read().get(&pane_idx).copied()
    } else {
        None
    };
    let (first, end) = clamp_entry_window(stored, entries_filtered.len(), DEFAULT_WINDOW);
    let entries_offset = first as u32;
    let locale = inner.deps.orchid_config.read().locale.clone();
    let thumb_cache = inner.thumbnail_rgba.read();
    let shell_cache = inner.shell_icon_rgba.read();
    let entry_payloads: Vec<EntryPayload> = entries_filtered[first..end]
        .iter()
        .copied()
        .map(|e| {
            let path_key = e.path.as_str();
            let icon_size = shell_icon_size_for_mode(tab.view_mode);
            let shell_key = shell_icon_cache_key(path_key, icon_size);
            // Prefer image previews; fall back to OS association icons.
            let (
                has_thumbnail,
                thumbnail_rgba,
                thumbnail_width,
                thumbnail_height,
                thumbnail_is_icon,
            ) = if let Some(t) = thumb_cache.get(path_key) {
                (
                    true,
                    Some(std::sync::Arc::clone(&t.rgba)),
                    t.width,
                    t.height,
                    false,
                )
            } else if let Some(t) = shell_cache.get(&shell_key) {
                (
                    true,
                    Some(std::sync::Arc::clone(&t.rgba)),
                    t.width,
                    t.height,
                    true,
                )
            } else {
                (false, None, 0, 0, false)
            };
            let is_dir = matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory);
            EntryPayload {
                path: path_key.to_string(),
                name: entry_display_name(&e.name, is_dir, config.show_extensions),
                is_dir,
                size_text: inner.deps.locale.format_byte_size(e.metadata.size),
                modified_text: e
                    .metadata
                    .modified
                    .map(|t| locale.format_datetime(t))
                    .unwrap_or_default(),
                type_text: orchid_fs::recycle_original_path(path_key)
                    .and_then(|orig| {
                        std::path::Path::new(&orig)
                            .parent()
                            .map(|p| p.display().to_string())
                            .filter(|s| !s.is_empty())
                            .or(Some(orig))
                    })
                    .unwrap_or_else(|| {
                        classify(
                            &inner.deps.locale,
                            &e.name,
                            matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory),
                        )
                    }),
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
                thumbnail_is_icon,
                is_selected: tab.selection.is_selected(path_key),
                is_hidden: e.metadata.hidden,
                is_encrypted: e.metadata.extended.is_encrypted,
                is_managed: e.metadata.extended.is_managed,
                is_starred: e.metadata.extended.starred,
                color_label: e.metadata.extended.color_label.map(color_label_to_str),
                tags: e.metadata.extended.tags.clone(),
            }
        })
        .collect();
    let selection_count = tab.selection.count() as u32;
    let selection_bytes = inner.selection_bytes_in_tab(tab);
    let managed_stats_guard = inner.managed_stats.read();
    let managed_stats = managed_stats_guard.get(tab.path.as_str());
    let (managed_files_tracked, managed_dedup_bytes) = managed_stats
        .map(|st| {
            let saved = st.logical_bytes.saturating_sub(st.physical_bytes);
            (Some(st.files_tracked as u32), Some(saved))
        })
        .unwrap_or((None, None));
    let error = if item_count == 0 {
        virtual_folders::empty_placeholder_for_path(tab.path.as_str())
            .map(String::from)
            .or_else(|| inner.tab_errors.read().get(&tab.id).cloned().flatten())
    } else {
        inner.tab_errors.read().get(&tab.id).cloned().flatten()
    };
    TabPayload {
        tab_id: tab.id.to_string(),
        path_display,
        breadcrumbs,
        can_go_back: !tab.history_back.is_empty(),
        can_go_forward: !tab.history_forward.is_empty(),
        view_mode: to_payload_mode(tab.view_mode),
        entries: entry_payloads,
        entries_offset,
        selection_count,
        item_count,
        selection_bytes,
        managed_files_tracked,
        managed_dedup_bytes,
        quick_filter: tab.quick_filter.clone(),
        is_loading: inner.loading_tabs.read().contains(&tab.id),
        error,
        sort_by: sort_by_to_u8(tab.sort_by),
        sort_descending: tab.sort_descending,
        branch_view: tab.branch_view,
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

fn find_tab_by_id(state: &FileManagerState, tab_id: Uuid) -> Option<&TabState> {
    state
        .left_pane
        .tabs
        .iter()
        .find(|t| t.id == tab_id)
        .or_else(|| {
            state
                .right_pane
                .as_ref()
                .and_then(|p| p.tabs.iter().find(|t| t.id == tab_id))
        })
}

fn fs_event_paths(env: &orchid_core::EventEnvelope) -> Vec<orchid_fs::FsPath> {
    use orchid_fs::{FsCreatedEvent, FsDeletedEvent, FsModifiedEvent, FsRenamedEvent};
    if let Some(e) = env.downcast::<FsCreatedEvent>() {
        return vec![e.path.clone()];
    }
    if let Some(e) = env.downcast::<FsModifiedEvent>() {
        return vec![e.path.clone()];
    }
    if let Some(e) = env.downcast::<FsDeletedEvent>() {
        return vec![e.path.clone()];
    }
    if let Some(e) = env.downcast::<FsRenamedEvent>() {
        return vec![e.from.clone(), e.to.clone()];
    }
    Vec::new()
}

/// True when a filesystem event should refresh the listing of `dir`.
fn fs_event_affects_listing(event_path: &str, dir: &str) -> bool {
    if event_path == dir {
        return true;
    }
    match event_path.rsplit_once('/') {
        Some((parent, _)) => parent == dir,
        None => false,
    }
}

#[cfg(test)]
mod dir_watch_tests {
    use super::fs_event_affects_listing;

    #[test]
    fn listing_refresh_for_direct_children_only() {
        let dir = "local:c:/Users/me/Docs";
        assert!(fs_event_affects_listing(dir, dir));
        assert!(fs_event_affects_listing(
            "local:c:/Users/me/Docs/a.txt",
            dir
        ));
        assert!(!fs_event_affects_listing(
            "local:c:/Users/me/Docs/sub/a.txt",
            dir
        ));
        assert!(!fs_event_affects_listing("local:c:/Users/me/Other", dir));
    }
}

fn sort_entries(entries: &mut Vec<orchid_fs::FsEntry>, sort_by: SortBy, descending: bool) {
    use std::cmp::Ordering;

    // Precompute sort keys once — the previous comparator allocated lowercase
    // strings on every comparison (≈ O(n log n) allocs).
    let mut keyed: Vec<_> = std::mem::take(entries)
        .into_iter()
        .map(|e| {
            let is_dir = matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory);
            let name_key = e.name.to_lowercase();
            let size = e.metadata.size;
            let modified = e.metadata.modified.map(|t| t.timestamp()).unwrap_or(0);
            let ext_key = e
                .path
                .extension()
                .map(|ext| ext.to_lowercase())
                .unwrap_or_default();
            (e, is_dir, name_key, size, modified, ext_key)
        })
        .collect();

    keyed.sort_by(|a, b| {
        let dir_ord = match (a.1, b.1) {
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
            SortBy::Name => a.2.cmp(&b.2),
            SortBy::Size => a.3.cmp(&b.3),
            SortBy::Modified => a.4.cmp(&b.4),
            SortBy::Type => a.5.cmp(&b.5).then_with(|| a.2.cmp(&b.2)),
        };
        if descending {
            field.reverse()
        } else {
            field
        }
    });

    *entries = keyed.into_iter().map(|(e, ..)| e).collect();
}

fn to_payload_mode(mode: ViewMode) -> FmViewMode {
    match mode {
        ViewMode::Icons => FmViewMode::Icons,
        ViewMode::List => FmViewMode::List,
        ViewMode::Details => FmViewMode::Details,
        ViewMode::Gallery => FmViewMode::Gallery,
    }
}

/// Max extension length shown in the Type column.
///
/// Longer "extensions" (e.g. `.dotnetUserLevelCache`) produce unreadable
/// labels like `DOTNETUSERLEVELCACHE file` that clip badly in a narrow column.
const TYPE_COLUMN_MAX_EXT_LEN: usize = 8;

/// Short file extension suitable for the Type column, if any.
fn type_column_extension(name: &str) -> Option<&str> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() || ext.len() > TYPE_COLUMN_MAX_EXT_LEN {
        return None;
    }
    if !ext
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '_')
    {
        return None;
    }
    Some(ext)
}

fn classify(locale: &orchid_i18n::LocaleManager, name: &str, is_dir: bool) -> String {
    if is_dir {
        return locale.tr("fm-properties-kind-folder");
    }
    // Prefer a short extension label over raw MIME (`image/png`) — Properties
    // already surfaces the MIME type. Unknown / absurdly long extensions fall
    // back to a generic "File" so the column stays readable.
    if let Some(ext) = type_column_extension(name) {
        return locale.tr_args(
            "fm-type-ext-file",
            &orchid_i18n::FluentArgs::new().with("ext", ext.to_ascii_uppercase()),
        );
    }
    locale.tr("fm-properties-kind-file")
}

fn is_image_entry(e: &orchid_fs::FsEntry) -> bool {
    if e.metadata
        .mime
        .as_deref()
        .map(|m| m.starts_with("image/"))
        .unwrap_or(false)
    {
        return true;
    }
    e.path
        .extension()
        .is_some_and(orchid_viewers::is_image_file_extension)
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
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, bytes| {
        let persisted = match bytes {
            Some(b) if !b.is_empty() => decode_persisted(b)?,
            _ => FileManagerPersisted {
                config: FileManagerConfig::default(),
                session: None,
                path_visits: Vec::new(),
            },
        };
        Ok(Box::new(FileManagerWidget::from_persisted(
            ctx.instance_id,
            deps.clone(),
            ctx.bus.clone(),
            persisted,
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

fn parse_persisted_path(path: &str, fallback: &orchid_fs::FsPath) -> orchid_fs::FsPath {
    orchid_fs::FsPath::new(path).unwrap_or_else(|_| fallback.clone())
}

fn tab_from_persisted(pt: &PersistedTab, fallback: &orchid_fs::FsPath) -> TabState {
    let id = Uuid::parse_str(&pt.id).unwrap_or_else(|_| Uuid::new_v4());
    TabState {
        id,
        path: parse_persisted_path(&pt.path, fallback),
        history_back: pt
            .history_back
            .iter()
            .filter_map(|p| orchid_fs::FsPath::new(p).ok())
            .collect(),
        history_forward: pt
            .history_forward
            .iter()
            .filter_map(|p| orchid_fs::FsPath::new(p).ok())
            .collect(),
        view_mode: pt.view_mode,
        selection: SelectionModel::new(),
        quick_filter: String::new(),
        scroll_position: 0.0,
        sort_by: pt.sort_by,
        sort_descending: pt.sort_descending,
        branch_view: false,
    }
}

fn tab_to_persisted(tab: &TabState) -> PersistedTab {
    PersistedTab {
        id: tab.id.to_string(),
        path: tab.path.as_str().to_string(),
        history_back: tab
            .history_back
            .iter()
            .map(|p| p.as_str().to_string())
            .collect(),
        history_forward: tab
            .history_forward
            .iter()
            .map(|p| p.as_str().to_string())
            .collect(),
        view_mode: tab.view_mode,
        sort_by: tab.sort_by,
        sort_descending: tab.sort_descending,
    }
}

fn pane_from_persisted(pane: &PersistedPane, fallback: &orchid_fs::FsPath) -> PaneState {
    let tabs: Vec<TabState> = pane
        .tabs
        .iter()
        .map(|t| tab_from_persisted(t, fallback))
        .collect();
    if tabs.is_empty() {
        return PaneState::with_single_tab(TabState::new(
            fallback.clone(),
            ViewMode::Details,
            SortBy::Name,
        ));
    }
    PaneState {
        active_tab: pane.active_tab.min(tabs.len().saturating_sub(1)),
        tabs,
    }
}

fn pane_to_persisted(pane: &PaneState) -> PersistedPane {
    PersistedPane {
        tabs: pane.tabs.iter().map(tab_to_persisted).collect(),
        active_tab: pane.active_tab,
    }
}

fn state_from_persisted(
    config: &FileManagerConfig,
    session: Option<&FileManagerSession>,
    fallback_path: orchid_fs::FsPath,
) -> FileManagerState {
    let Some(session) = session else {
        return FileManagerState::single_pane(
            fallback_path,
            config.default_view_mode,
            config.sort_by,
        );
    };

    let left_pane = pane_from_persisted(&session.left_pane, &fallback_path);
    let mut right_pane = session
        .right_pane
        .as_ref()
        .map(|p| pane_from_persisted(p, &left_pane.active_tab().path));

    if config.dual_pane && right_pane.is_none() {
        let path = left_pane.active_tab().path.clone();
        right_pane = Some(PaneState::with_single_tab(TabState::new(
            path,
            config.default_view_mode,
            config.sort_by,
        )));
    } else if !config.dual_pane {
        right_pane = None;
    }

    let active_pane = match session.active_pane {
        PersistedActivePane::Left => ActivePane::Left,
        PersistedActivePane::Right if config.dual_pane && right_pane.is_some() => ActivePane::Right,
        PersistedActivePane::Right => ActivePane::Left,
    };

    FileManagerState {
        left_pane,
        right_pane,
        active_pane,
    }
}

fn session_from_state(state: &FileManagerState) -> FileManagerSession {
    FileManagerSession {
        left_pane: pane_to_persisted(&state.left_pane),
        right_pane: state.right_pane.as_ref().map(pane_to_persisted),
        active_pane: match state.active_pane {
            ActivePane::Left => PersistedActivePane::Left,
            ActivePane::Right => PersistedActivePane::Right,
        },
    }
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

async fn recycle_purge_action(
    inner: &Arc<FileManagerInner>,
    target_paths: Vec<String>,
    skip_confirm: bool,
) -> WidgetResult<ActionOutcome> {
    let paths: Vec<String> = target_paths
        .into_iter()
        .filter(|p| orchid_fs::is_recycle_item(p))
        .collect();
    if paths.is_empty() {
        return Ok(ActionOutcome::Done);
    }
    if inner.config.read().confirm_delete && !skip_confirm {
        return Ok(ActionOutcome::NeedsConfirmation {
            message: "fm-confirm-recycle-purge".into(),
            action_id: "fs.recycle-purge".into(),
            paths,
        });
    }
    orchid_fs::purge_recycle(&paths)
        .await
        .map_err(map_fs_error)?;
    inner.refresh_all_tabs().await;
    Ok(ActionOutcome::Done)
}

/// Report a passphrase failure to the status bar (dialog may stay open for retry).
pub fn report_passphrase_error(instance_id: Uuid, message: String) -> WidgetResult<()> {
    live_inner(instance_id)?.set_passphrase_error(message);
    Ok(())
}

/// Clear the passphrase failure toast.
pub fn clear_passphrase_error(instance_id: Uuid) -> WidgetResult<()> {
    live_inner(instance_id)?.clear_passphrase_error();
    Ok(())
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
) -> WidgetResult<(Vec<ContextMenuItem>, Vec<String>, Option<ContextMenuInfo>)> {
    let inner = live_inner(instance_id)?;
    if context_path.is_empty() {
        inner.deselect_all_in_pane(pane);
    }
    let (entries, target_paths, entry_count, selection_count, can_create) = {
        let state = inner.state.lock();
        let tab = active_tab_ref(&state, pane)?;
        let tab_id = tab.id;
        let selection = tab.selection.selected_paths();
        let entries = inner
            .entries_by_tab
            .read()
            .get(&tab_id)
            .cloned()
            .unwrap_or_else(|| Arc::new(Vec::new()));
        let entry_count = inner.filtered_paths_for_tab(tab).len();
        let selection_count = tab.selection.count();
        let can_create = !is_virtual(&tab.path);
        let target_paths = if context_path.is_empty() {
            Vec::new()
        } else if selection.iter().any(|p| p == context_path) {
            selection
        } else {
            vec![context_path.to_string()]
        };
        (
            entries,
            target_paths,
            entry_count,
            selection_count,
            can_create,
        )
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
        clipboard_has_contents: inner.deps.clipboard.can_paste(),
        all_encrypted: selected_entries
            .iter()
            .all(|e| e.metadata.extended.is_encrypted),
        any_encrypted: selected_entries
            .iter()
            .any(|e| e.metadata.extended.is_encrypted),
        all_managed: selected_entries
            .iter()
            .all(|e| e.metadata.extended.is_managed),
        all_starred: selected_entries.iter().all(|e| e.metadata.extended.starred),
        any_starred: selected_entries.iter().any(|e| e.metadata.extended.starred),
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
        managed_policy_available: target_paths
            .iter()
            .any(|p| inner.managed_root_for_path(p).is_some()),
        can_create,
        dual_pane: inner.state.lock().right_pane.is_some(),
        in_archive: inner
            .state
            .lock()
            .active_pane()
            .active_tab()
            .path
            .is_archive(),
        in_recycle: {
            let tab_path = inner
                .state
                .lock()
                .active_pane()
                .active_tab()
                .path
                .as_str()
                .to_string();
            orchid_fs::is_recycle_listing(&tab_path)
                || target_paths.iter().any(|p| orchid_fs::is_recycle_item(p))
        },
    };
    let fmt_locale = inner.deps.orchid_config.read().locale.clone();
    let info = info_for_selection(&selected_entries, &inner.deps.locale, &fmt_locale);
    Ok((
        build_for_selection(&selected_entries, inputs),
        target_paths,
        info,
    ))
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

fn active_tab_ref(state: &FileManagerState, pane: u8) -> WidgetResult<&TabState> {
    if pane == 1 {
        if let Some(r) = state.right_pane.as_ref() {
            return Ok(r.active_tab());
        }
    }
    Ok(state.left_pane.active_tab())
}

fn active_tab_path(inner: &FileManagerInner) -> String {
    let state = inner.state.lock();
    let pane = match state.active_pane {
        ActivePane::Left => 0,
        ActivePane::Right => 1,
    };
    active_tab_ref(&state, pane)
        .map(|t| t.path.as_str().to_string())
        .unwrap_or_default()
}

/// Navigate the given `pane` (0 left, 1 right) to `path`.
pub async fn navigate(instance_id: Uuid, pane: u8, path: orchid_fs::FsPath) -> WidgetResult<()> {
    navigate_inner(instance_id, pane, path, true).await
}

async fn navigate_inner(
    instance_id: Uuid,
    pane: u8,
    path: orchid_fs::FsPath,
    publish: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let tab = {
        let mut state = inner.state.lock();
        let changed = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut().navigate_to(path)
            } else {
                state.left_pane.active_tab_mut().navigate_to(path)
            }
        } else {
            state.left_pane.active_tab_mut().navigate_to(path)
        };
        inner.reset_pane_viewport(pane);
        let tab = active_tab_ref(&state, pane)?.clone();
        if changed {
            inner.record_visit(&tab.path);
        }
        tab
    };
    inner
        .refresh_tabs_with_opts(
            &[tab],
            RefreshOpts {
                publish,
                indicate_loading: true,
            },
        )
        .await;
    Ok(())
}

/// Back in history for `pane`.
pub async fn navigate_back(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let tab = {
        let mut state = inner.state.lock();
        let changed = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut().back()
            } else {
                state.left_pane.active_tab_mut().back()
            }
        } else {
            state.left_pane.active_tab_mut().back()
        };
        if !changed {
            return Ok(());
        }
        inner.reset_pane_viewport(pane);
        let tab = active_tab_ref(&state, pane)?.clone();
        inner.record_visit(&tab.path);
        tab
    };
    inner
        .refresh_tabs_with_opts(
            &[tab],
            RefreshOpts {
                publish: true,
                indicate_loading: true,
            },
        )
        .await;
    Ok(())
}

/// Forward in history for `pane`.
pub async fn navigate_forward(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let tab = {
        let mut state = inner.state.lock();
        let changed = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut().forward()
            } else {
                state.left_pane.active_tab_mut().forward()
            }
        } else {
            state.left_pane.active_tab_mut().forward()
        };
        if !changed {
            return Ok(());
        }
        inner.reset_pane_viewport(pane);
        let tab = active_tab_ref(&state, pane)?.clone();
        inner.record_visit(&tab.path);
        tab
    };
    inner
        .refresh_tabs_with_opts(
            &[tab],
            RefreshOpts {
                publish: true,
                indicate_loading: true,
            },
        )
        .await;
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

/// Jump the active tab to the user's home directory (or a local root fallback).
pub async fn navigate_home(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    navigate(instance_id, pane, default_initial_path()).await
}

/// Directory names under the typed address, filtered by the trailing prefix.
#[must_use]
pub async fn complete_path(instance_id: Uuid, typed: &str) -> Vec<PathCompleteItem> {
    let Ok(inner) = live_inner(instance_id) else {
        return Vec::new();
    };
    let Some((parent, prefix)) = complete_parent_and_prefix(typed) else {
        return Vec::new();
    };
    let show_hidden = inner.config.read().show_hidden;
    let result = inner.navigator.navigate(&parent, show_hidden).await;
    let prefix_l = prefix.to_lowercase();
    let mut items: Vec<PathCompleteItem> = result
        .entries
        .into_iter()
        .filter(|e| matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory))
        .filter(|e| prefix_l.is_empty() || e.name.to_lowercase().starts_with(&prefix_l))
        .map(|e| PathCompleteItem {
            path: e.path.as_str().to_string(),
            label: e.name,
        })
        .collect();
    items.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    items.truncate(12);
    items
}

/// Switch to tab by string id.
pub async fn switch_to_tab(instance_id: Uuid, pane: u8, tab_id: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let want = Uuid::parse_str(tab_id)
        .map_err(|_| WidgetError::InvalidStateForOperation("invalid tab id".into()))?;
    {
        let mut state = inner.state.lock();
        let prev_id = active_tab_ref(&state, pane).ok().map(|t| t.id);
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
        let tab = active_tab_ref(&state, pane)?;
        if prev_id != Some(tab.id) {
            inner.record_visit(&tab.path);
        }
    }
    inner.reset_pane_viewport(pane);
    inner.publish_refresh();
    Ok(())
}

/// Close tab by id.
pub async fn close_tab(instance_id: Uuid, pane: u8, tab_id: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let want = Uuid::parse_str(tab_id)
        .map_err(|_| WidgetError::InvalidStateForOperation("invalid tab id".into()))?;
    let mut closed = false;
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
                closed = true;
            }
        } else {
            if state.left_pane.tabs.len() <= 1 {
                return Ok(());
            }
            if let Some(idx) = state.left_pane.tabs.iter().position(|t| t.id == want) {
                state.left_pane.tabs.remove(idx);
                state.left_pane.active_tab = state
                    .left_pane
                    .active_tab
                    .min(state.left_pane.tabs.len().saturating_sub(1));
                closed = true;
            }
        }
    }
    if closed {
        inner.drop_tab_watch(want);
    }
    inner.publish_refresh();
    Ok(())
}

/// Create a new tab in `pane`. When `path` is `None`, clones the current folder.
pub async fn new_tab(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    new_tab_at(instance_id, pane, None).await
}

/// Create a new tab in `pane` opened at `path` (or the current folder).
pub async fn new_tab_at(
    instance_id: Uuid,
    pane: u8,
    path: Option<orchid_fs::FsPath>,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let tab = {
        let cfg = inner.config.read().clone();
        let mut state = inner.state.lock();
        let dest = if pane == 1 && state.right_pane.is_some() {
            state.right_pane.as_mut().expect("right pane")
        } else {
            &mut state.left_pane
        };
        let path = path.unwrap_or_else(|| dest.active_tab().path.clone());
        dest.tabs
            .push(TabState::new(path, cfg.default_view_mode, cfg.sort_by));
        dest.active_tab = dest.tabs.len().saturating_sub(1);
        dest.active_tab().clone()
    };
    inner
        .refresh_tabs_with_opts(
            &[tab],
            RefreshOpts {
                publish: true,
                indicate_loading: true,
            },
        )
        .await;
    Ok(())
}

/// Open `path` (or the current folder) in the opposite pane, enabling dual-pane if needed.
pub async fn open_in_other_pane(
    instance_id: Uuid,
    src_pane: u8,
    path: Option<orchid_fs::FsPath>,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let dual = inner.config.read().dual_pane;
    if !dual {
        drop(inner);
        toggle_dual_pane(instance_id).await?;
    }
    let inner = live_inner(instance_id)?;
    let dest_pane: u8 = if src_pane == 1 { 0 } else { 1 };
    let target = {
        let state = inner.state.lock();
        if let Some(p) = path {
            p
        } else {
            let src = if src_pane == 1 {
                state
                    .right_pane
                    .as_ref()
                    .unwrap_or(&state.left_pane)
                    .active_tab()
            } else {
                state.left_pane.active_tab()
            };
            src.path.clone()
        }
    };
    drop(inner);
    navigate(instance_id, dest_pane, target).await?;
    switch_active_pane(instance_id, dest_pane).await
}

/// Flatten or restore the listing of nested files in `pane` (branch view).
pub async fn toggle_branch_view(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let tab = {
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
        if is_virtual(&tab.path) {
            return Ok(());
        }
        tab.branch_view = !tab.branch_view;
        tab.selection.clear();
        inner.reset_pane_viewport(pane);
        tab.clone()
    };
    inner
        .refresh_tabs_with_opts(
            &[tab],
            RefreshOpts {
                publish: true,
                indicate_loading: true,
            },
        )
        .await;
    Ok(())
}

/// Jump `pane` to the drive / volume root of the current folder.
pub async fn navigate_drive_root(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let current = {
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
        tab.path.clone()
    };
    let Some(root) = drive_root(&current) else {
        return Ok(());
    };
    if root == current {
        return Ok(());
    }
    navigate(instance_id, pane, root).await
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

/// Shared file clipboard for a live file-manager instance.
#[must_use]
pub fn file_clipboard(instance_id: Uuid) -> Option<Arc<FileClipboard>> {
    FM_LIVE
        .get(&instance_id)
        .map(|inner| inner.deps.clipboard.clone())
}

/// Snapshot live file-manager config for the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<FileManagerConfig> {
    FM_LIVE
        .get(&instance_id)
        .map(|inner| inner.config.read().clone())
}

/// Apply a settings mutation. For dual_pane changes, mirror the pane create/destroy logic from toggle_dual_pane.
pub async fn update_config(
    instance_id: Uuid,
    mutate: impl FnOnce(&mut FileManagerConfig),
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let before_dual = inner.config.read().dual_pane;
    let before_hidden = inner.config.read().show_hidden;
    {
        let mut cfg = inner.config.write();
        mutate(&mut cfg);
    }
    let after = inner.config.read().clone();
    if before_dual != after.dual_pane {
        let enabled = after.dual_pane;
        {
            let mut state = inner.state.lock();
            if enabled && state.right_pane.is_none() {
                let path = state.left_pane.active_tab().path.clone();
                state.right_pane = Some(PaneState::with_single_tab(TabState::new(
                    path,
                    after.default_view_mode,
                    after.sort_by,
                )));
            }
            if !enabled {
                state.right_pane = None;
                state.active_pane = ActivePane::Left;
            }
        }
        inner.publish_refresh();
    } else if before_hidden != after.show_hidden {
        inner.refresh_all_tabs().await;
    } else {
        inner.publish_refresh();
    }
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

/// Update the visible filtered-entry window for a pane (used by UI virtualization).
pub fn set_viewport_window(instance_id: Uuid, pane: u8, first: usize, end: usize) {
    if let Some(inner) = FM_LIVE.get(&instance_id) {
        let end = end.max(first);
        inner.viewport_by_pane.write().insert(pane, (first, end));
    }
}

/// Drop a cached viewport slice so the next snapshot starts at the top.
pub fn reset_viewport_window(instance_id: Uuid, pane: u8) {
    if let Some(inner) = FM_LIVE.get(&instance_id) {
        inner.reset_pane_viewport(pane);
    }
}

/// Listings at or below this size are sent in full. Must match the UI
/// `FM_VIRTUALIZE_THRESHOLD` so a small folder never ships a mid-list slice
/// while Slint lays out `pad = 0` and expects every row.
const FM_VIRTUALIZE_THRESHOLD: usize = 80;

/// Map a stored virtualization window onto a listing of `len` entries.
///
/// A window that starts past the end (typical after navigating out of a long
/// folder) is treated as "show from the top" rather than an empty slice.
fn clamp_entry_window(
    stored: Option<(usize, usize)>,
    len: usize,
    default_window: usize,
) -> (usize, usize) {
    if len <= FM_VIRTUALIZE_THRESHOLD {
        return (0, len);
    }
    match stored {
        Some((first, end)) if first < len && end > first => (first, end.min(len)),
        _ => (0, default_window.min(len)),
    }
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
    inner.reset_pane_viewport(0);
    inner.reset_pane_viewport(1);
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
    let tab = {
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
        tab.clone()
    };
    inner.reset_pane_viewport(pane);
    inner.publish_refresh();
    inner.spawn_view_decorations(tab);
    Ok(())
}

/// Cycle the sort column for the active tab in `pane` (folders stay grouped first).
pub async fn cycle_sort(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let (tab_id, sort_by, descending) = {
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
        (tab.id, tab.sort_by, tab.sort_descending)
    };
    inner.reset_pane_viewport(pane);
    inner.resort_tab_in_memory(tab_id, sort_by, descending);
    inner.publish_refresh();
    Ok(())
}

/// Set sort column for the active tab in `pane`; toggles direction when the column is unchanged.
pub async fn set_sort_column(instance_id: Uuid, pane: u8, column: u8) -> WidgetResult<()> {
    let sort_by = sort_by_from_u8(column)
        .ok_or_else(|| WidgetError::InvalidStateForOperation("invalid sort column".into()))?;
    let inner = live_inner(instance_id)?;
    let (tab_id, sort_by, descending) = {
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
        (tab.id, tab.sort_by, tab.sort_descending)
    };
    inner.reset_pane_viewport(pane);
    inner.resort_tab_in_memory(tab_id, sort_by, descending);
    inner.publish_refresh();
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
    inner.reset_pane_viewport(pane);
    inner.publish_refresh();
    Ok(())
}

/// Select entry inside the active tab for `pane`.
///
/// Does **not** publish a snapshot refresh — the UI patches selection flags in
/// place. Callers that need a full rebuild must refresh explicitly.
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
        // Honor the active quick filter so range selection matches the visible list.
        (tab.id, inner.filtered_paths_for_tab(tab))
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
    Ok(())
}

/// Selected entries `(path, is_dir)` for the active tab in `pane` (live state).
#[must_use]
pub fn selected_entries(instance_id: Uuid, pane: u8) -> Vec<(String, bool)> {
    let Ok(inner) = live_inner(instance_id) else {
        return Vec::new();
    };
    let state = inner.state.lock();
    let Ok(tab) = active_tab_ref(&state, pane) else {
        return Vec::new();
    };
    let selected = tab.selection.selected_paths();
    if selected.is_empty() {
        return Vec::new();
    };
    let guard = inner.entries_by_tab.read();
    let Some(entries) = guard.get(&tab.id) else {
        return selected.into_iter().map(|p| (p, false)).collect();
    };
    selected
        .into_iter()
        .map(|path| {
            let is_dir = entries
                .iter()
                .find(|e| e.path.as_str() == path)
                .map(|e| matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory))
                .unwrap_or(false);
            (path, is_dir)
        })
        .collect()
}

/// `(selection_count, item_count, selected_bytes)` for the active tab in `pane`.
#[must_use]
pub fn selection_counts(instance_id: Uuid, pane: u8) -> Option<(u32, u32, u64)> {
    let inner = live_inner(instance_id).ok()?;
    let state = inner.state.lock();
    let tab = active_tab_ref(&state, pane).ok()?;
    let selection_count = tab.selection.count() as u32;
    let item_count = inner.filtered_paths_for_tab(tab).len() as u32;
    let bytes = inner.selection_bytes_in_tab(tab);
    Some((selection_count, item_count, bytes))
}

/// Run a context-menu action against `target_paths`.
pub async fn run_action(
    instance_id: Uuid,
    action_id: &str,
    target_paths: Vec<String>,
) -> WidgetResult<ActionOutcome> {
    run_action_with_opts(
        instance_id,
        action_id,
        target_paths,
        RunActionOpts::default(),
    )
    .await
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
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            let is_dir = entry_is_directory(&inner, &fp, false).await;
            return open_path(instance_id, pane, p, is_dir).await;
        }
        "fs.open-tab" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            let folder = if let Some(p) = target_paths.first() {
                folder_path_from_target(&inner, p).await
            } else {
                None
            };
            new_tab_at(instance_id, pane, folder).await?;
            return Ok(ActionOutcome::Done);
        }
        "fs.open-other-pane" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            let folder = if let Some(p) = target_paths.first() {
                folder_path_from_target(&inner, p).await
            } else {
                None
            };
            open_in_other_pane(instance_id, pane, folder).await?;
            return Ok(ActionOutcome::Done);
        }
        "fs.branch-view" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            toggle_branch_view(instance_id, pane).await?;
            return Ok(ActionOutcome::Done);
        }
        "fs.open-all" => {
            let mut files = Vec::new();
            for p in target_paths {
                let fp = orchid_fs::FsPath::new(&p).map_err(map_fs_error)?;
                // Skip directories even when provider metadata fails (OS fallback in helper).
                if entry_is_directory(&inner, &fp, false).await {
                    continue;
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
        "viewer.edit" => {
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
            return Ok(ActionOutcome::OpenInEditor { path: p.clone() });
        }
        "fs.file-assoc" => {
            let Some(p) = target_paths.first() else {
                return Ok(ActionOutcome::Done);
            };
            return Ok(ActionOutcome::OpenFileAssociations { path: p.clone() });
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
            return inner.paste_clipboard().await;
        }
        "fs.rename" => {
            if target_paths.len() == 1 {
                let p = target_paths[0].clone();
                let current_name = p.rsplit('/').next().unwrap_or(p.as_str()).to_string();
                return Ok(ActionOutcome::NeedsRename {
                    path: p,
                    current_name,
                });
            }
            if target_paths.len() > 1 {
                return Ok(ActionOutcome::NeedsBatchRename {
                    paths: target_paths,
                });
            }
        }
        "fs.batch-rename" => {
            if !target_paths.is_empty() {
                return Ok(ActionOutcome::NeedsBatchRename {
                    paths: target_paths,
                });
            }
        }
        "fs.delete" | "fs.delete-recycle" | "fs.delete-permanent" => {
            if target_paths.iter().any(|p| orchid_fs::is_recycle_item(p)) {
                return recycle_purge_action(&inner, target_paths, opts.skip_confirm).await;
            }
            let cfg = inner.config.read().clone();
            let recycle = match action_id {
                "fs.delete-recycle" => true,
                "fs.delete-permanent" => false,
                _ => cfg.delete_to_recycle,
            };
            if cfg.confirm_delete && !target_paths.is_empty() && !opts.skip_confirm {
                let message = if recycle {
                    "fm-confirm-delete"
                } else {
                    "fm-confirm-delete-permanent"
                };
                return Ok(ActionOutcome::NeedsConfirmation {
                    message: message.into(),
                    action_id: action_id.to_string(),
                    paths: target_paths,
                });
            }
            inner.delete_paths(&target_paths, Some(recycle)).await?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.recycle-restore" => {
            if target_paths.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            orchid_fs::restore_recycle(&target_paths)
                .await
                .map_err(map_fs_error)?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.recycle-purge" => {
            return recycle_purge_action(&inner, target_paths, opts.skip_confirm).await;
        }
        "fs.recycle-empty" => {
            if !opts.skip_confirm {
                return Ok(ActionOutcome::NeedsConfirmation {
                    message: "fm-confirm-recycle-empty".into(),
                    action_id: "fs.recycle-empty".into(),
                    paths: vec![orchid_fs::RECYCLE_PATH.to_string()],
                });
            }
            orchid_fs::empty_recycle().await.map_err(map_fs_error)?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.copy-to-other" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            return copy_to_other_pane(
                instance_id,
                pane,
                target_paths,
                TransferOptions::default(),
                opts.skip_confirm,
            )
            .await;
        }
        "fs.move-to-other" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            return move_to_other_pane(
                instance_id,
                pane,
                target_paths,
                TransferOptions::default(),
                opts.skip_confirm,
            )
            .await;
        }
        "fs.copy-verify" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            return copy_to_other_pane(
                instance_id,
                pane,
                target_paths,
                TransferOptions {
                    verify: true,
                    ..TransferOptions::default()
                },
                opts.skip_confirm,
            )
            .await;
        }
        "fs.copy-newer" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            return copy_to_other_pane(
                instance_id,
                pane,
                target_paths,
                TransferOptions {
                    newer_only: true,
                    ..TransferOptions::default()
                },
                opts.skip_confirm,
            )
            .await;
        }
        "fs.copy-structure" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            return copy_to_other_pane(
                instance_id,
                pane,
                target_paths,
                TransferOptions {
                    structure_only: true,
                    ..TransferOptions::default()
                },
                opts.skip_confirm,
            )
            .await;
        }
        "fs.link-symlink" | "fs.link-hard" | "fs.link-junction" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            create_link_in_pane(instance_id, pane, &target_paths, action_id).await?;
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
            return Ok(ActionOutcome::Done);
        }
        "fs.deselect-all" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.deselect_all_in_pane(pane);
            return Ok(ActionOutcome::Done);
        }
        "fs.select-more" => {
            return Ok(ActionOutcome::Done);
        }
        "fs.invert-selection" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.invert_selection_in_pane(pane);
            return Ok(ActionOutcome::Done);
        }
        "fs.select-files" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.apply_filter_in_pane(
                pane,
                MaskOp::Replace,
                &SelectFilter {
                    folders: false,
                    ..SelectFilter::default()
                },
            );
            return Ok(ActionOutcome::Done);
        }
        "fs.select-folders" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.apply_filter_in_pane(
                pane,
                MaskOp::Replace,
                &SelectFilter {
                    files: false,
                    ..SelectFilter::default()
                },
            );
            return Ok(ActionOutcome::Done);
        }
        "fs.select-hidden" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.apply_filter_in_pane(
                pane,
                MaskOp::Add,
                &SelectFilter {
                    hidden: true,
                    ..SelectFilter::default()
                },
            );
            return Ok(ActionOutcome::Done);
        }
        "fs.select-readonly" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.apply_filter_in_pane(
                pane,
                MaskOp::Add,
                &SelectFilter {
                    readonly: true,
                    ..SelectFilter::default()
                },
            );
            return Ok(ActionOutcome::Done);
        }
        "fs.select-mask" | "fs.select-mask-add" => {
            return Ok(ActionOutcome::NeedsSelectMask {
                op: MaskOp::Add,
                filter: false,
            });
        }
        "fs.select-mask-sub" => {
            return Ok(ActionOutcome::NeedsSelectMask {
                op: MaskOp::Subtract,
                filter: false,
            });
        }
        "fs.select-filter" => {
            return Ok(ActionOutcome::NeedsSelectMask {
                op: MaskOp::Replace,
                filter: true,
            });
        }
        "fs.find" => {
            let root = active_tab_path(&inner);
            return Ok(ActionOutcome::NeedsFindDialog { root });
        }
        "fs.find-duplicates" => {
            let root = orchid_fs::FsPath::new(active_tab_path(&inner)).map_err(map_fs_error)?;
            return find::run_find_duplicates(&inner, &root).await;
        }
        "fs.find-large" => {
            let root = active_tab_path(&inner);
            return Ok(ActionOutcome::NeedsToolPrompt {
                action_id: "fs.find-large".into(),
                paths: vec![root],
                proposed: "100MB".into(),
                title: inner.deps.locale.tr("fm-find-large-title"),
                hint: inner.deps.locale.tr("fm-find-large-hint"),
            });
        }
        "fs.new-folder" => {
            let parent = {
                let state = inner.state.lock();
                let pane = match state.active_pane {
                    ActivePane::Left => 0,
                    ActivePane::Right => 1,
                };
                active_tab_ref(&state, pane).map(|t| t.path.clone()).ok()
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
        "fs.new-file" => {
            let parent = {
                let state = inner.state.lock();
                let pane = match state.active_pane {
                    ActivePane::Left => 0,
                    ActivePane::Right => 1,
                };
                active_tab_ref(&state, pane).map(|t| t.path.clone()).ok()
            };
            if let Some(parent) = parent {
                if !is_virtual(&parent) {
                    return Ok(ActionOutcome::NeedsCreateFile {
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
        "fs.managed-policy" => {
            let root = target_paths
                .iter()
                .find_map(|p| inner.managed_root_for_path(p))
                .ok_or_else(|| {
                    WidgetError::InvalidStateForOperation("fm-not-managed-folder".into())
                })?;
            let policy = inner.managed_policies.read().get(&root).cloned().flatten();
            return Ok(ActionOutcome::NeedsManagedPolicy { path: root, policy });
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
        id if archive::is_archive_action(id) => {
            return archive::run(&inner, action_id, &target_paths, None).await;
        }
        id if is_tools_action(id) => {
            return tools::run(&inner, action_id, &target_paths, opts, None).await;
        }
        _ => {
            // Unknown actions: treat as done for MVP.
        }
    }
    inner.publish_refresh();
    Ok(ActionOutcome::Done)
}

fn is_tools_action(id: &str) -> bool {
    matches!(
        id,
        "fs.compare-dirs"
            | "fs.compare-dirs-bytes"
            | "fs.compare-files"
            | "fs.sync-to-other"
            | "fs.sync-from-other"
            | "fs.sync-both"
            | "fs.cloud-sync"
            | "fs.network-bookmark"
            | "fs.network-connect"
            | "fs.merge-to-other"
            | "fs.split"
            | "fs.join"
            | "fs.hash-md5"
            | "fs.hash-sha1"
            | "fs.hash-sha256"
            | "fs.hash-blake3"
            | "fs.hash-crc32"
            | "fs.hash-verify"
            | "fs.encode-base64"
            | "fs.decode-base64"
            | "fs.encode-uue"
            | "fs.decode-uue"
            | "fs.attr-readonly-on"
            | "fs.attr-readonly-off"
            | "fs.attr-hidden-on"
            | "fs.attr-hidden-off"
            | "fs.attr-system-on"
            | "fs.attr-system-off"
            | "fs.attr-archive-on"
            | "fs.attr-archive-off"
            | "fs.touch-now"
            | "fs.touch-set"
            | "fs.case-lower"
            | "fs.case-upper"
            | "fs.case-title"
            | "fs.chmod"
            | "fs.chown"
            | "fs.acl-view"
            | "fs.acl-grant"
            | "fs.acl-reset"
            | "fs.share"
            | "fs.share-view"
            | "fs.share-add"
            | "fs.share-remove"
            | "fs.share-os"
            | "fs.properties"
            | "fs.exif"
            | "fs.id3"
            | "fs.office-meta"
            | "fs.signature"
    )
}

/// Finish a tool prompt (`NeedsToolPrompt`) with the user's `input`.
pub async fn complete_tool(
    instance_id: Uuid,
    action_id: &str,
    paths: Vec<String>,
    input: &str,
) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    if action_id == "fs.find-large" {
        let root = paths
            .first()
            .cloned()
            .unwrap_or_else(|| active_tab_path(&inner));
        let min_size = parse_byte_size(input).unwrap_or(100 * 1024 * 1024);
        let fp = orchid_fs::FsPath::new(&root).map_err(map_fs_error)?;
        return find::run_find_large(&inner, &fp, min_size).await;
    }
    if archive::is_archive_action(action_id) {
        return archive::run(&inner, action_id, &paths, Some(input)).await;
    }
    tools::run(
        &inner,
        action_id,
        &paths,
        RunActionOpts { skip_confirm: true },
        Some(input),
    )
    .await
}

/// Run Find Files from the packed dialog payload.
pub async fn complete_find(instance_id: Uuid, packed: &str) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    let fallback = active_tab_path(&inner);
    let spec = find::FindSpec::unpack(packed, &fallback);
    find::run_find(&inner, spec).await
}

/// Commit rename on the backing filesystem.
pub async fn rename(instance_id: Uuid, old_path: &str, new_name: &str) -> WidgetResult<()> {
    if new_name.is_empty()
        || new_name.contains('/')
        || new_name.contains('\\')
        || new_name.contains(':')
    {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-invalid-rename-target".into(),
        ));
    }
    let inner = live_inner(instance_id)?;
    let old = orchid_fs::FsPath::new(old_path).map_err(map_fs_error)?;
    let parent = old
        .parent()
        .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-cannot-rename-root".into()))?;
    let new_path = parent.join(new_name);
    let provider = inner
        .deps
        .registry
        .for_path(&old)
        .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-no-provider-path".into()))?;
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
            "fm-virtual-create-denied".into(),
        ));
    }
    inner.create_folder_at(&parent, name).await?;
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Create an empty file under `parent_path`.
pub async fn create_file(instance_id: Uuid, parent_path: &str, name: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let parent = orchid_fs::FsPath::new(parent_path).map_err(map_fs_error)?;
    if is_virtual(&parent) {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-virtual-create-denied".into(),
        ));
    }
    inner.create_file_at(&parent, name).await?;
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
        return Err(WidgetError::InvalidStateForOperation("fm-empty-tag".into()));
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
///
/// Selection-only — does not re-list the directory or publish a snapshot.
pub async fn select_all_in_pane(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.select_all_in_pane(pane);
    Ok(())
}

/// Clear selection in `pane`'s active tab.
///
/// Selection-only — does not re-list the directory or publish a snapshot.
pub async fn deselect_all_in_pane(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.deselect_all_in_pane(pane);
    Ok(())
}

/// Active pane index (0 left, 1 right) for `instance_id`.
#[must_use]
pub fn focused_pane(instance_id: Uuid) -> Option<u8> {
    let inner = live_inner(instance_id).ok()?;
    let pane = inner.state.lock().active_pane;
    Some(match pane {
        ActivePane::Left => 0,
        ActivePane::Right => 1,
    })
}

/// Invert the visible selection in `pane`'s active tab.
pub async fn invert_selection_in_pane(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.invert_selection_in_pane(pane);
    Ok(())
}

/// Apply a name/attribute mask to the visible listing.
pub async fn apply_select_filter(
    instance_id: Uuid,
    pane: u8,
    op: MaskOp,
    filter: SelectFilter,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.apply_filter_in_pane(pane, op, &filter);
    Ok(())
}

/// Select listing indices `[from, to]` (inclusive). `additive` keeps the previous set.
///
/// `columns <= 1` selects a linear range (list / details). Otherwise the bounding
/// rectangle of the two tile indices in an icon grid is selected.
pub async fn select_index_range(
    instance_id: Uuid,
    pane: u8,
    from: i32,
    to: i32,
    additive: bool,
    columns: i32,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.select_index_range_in_pane(
        pane,
        from.max(0) as usize,
        to.max(0) as usize,
        additive,
        columns.max(1) as usize,
    );
    Ok(())
}

/// Move selection by `delta` entries in `pane`'s active tab (filtered order).
/// With `extend`, grows a range from the anchor (Shift+arrows).
///
/// Selection-only — does not publish a snapshot refresh.
pub async fn select_relative(
    instance_id: Uuid,
    pane: u8,
    delta: i32,
    extend: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.move_selection_in_pane(pane, delta, extend);
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
    let outcome = match purpose {
        PassphrasePurpose::Encrypt => {
            inner.encrypt_paths(&paths, &passphrase).await?;
            inner.refresh_all_tabs().await;
            ActionOutcome::Done
        }
        PassphrasePurpose::Decrypt => {
            inner.decrypt_paths(&paths, &passphrase).await?;
            inner.refresh_all_tabs().await;
            ActionOutcome::Done
        }
        PassphrasePurpose::Reveal => {
            let revealed = inner.reveal_paths(&paths, &passphrase).await?;
            ActionOutcome::OpenExternally { paths: revealed }
        }
        PassphrasePurpose::RevealInViewer => {
            let revealed = inner.reveal_paths(&paths, &passphrase).await?;
            if let Some(path) = revealed.first() {
                ActionOutcome::OpenInViewer { path: path.clone() }
            } else {
                ActionOutcome::Done
            }
        }
        PassphrasePurpose::ArchiveCreate | PassphrasePurpose::ArchiveOpen => {
            archive::apply_passphrase(&inner, &paths, &passphrase, purpose).await?
        }
    };
    let _ = inner
        .deps
        .fm_passphrase_vault
        .save_passphrase(secrecy::SecretString::from(passphrase));
    Ok(outcome)
}

/// Current single- vs double-click open behaviour.
pub fn click_behavior(instance_id: Uuid) -> WidgetResult<ClickBehavior> {
    Ok(live_inner(instance_id)?.config.read().click_behavior)
}

/// Open a path from the listing (navigate directories, reveal or view files).
pub async fn open_path(
    instance_id: Uuid,
    pane: u8,
    path: &str,
    is_dir_hint: bool,
) -> WidgetResult<ActionOutcome> {
    let t0 = std::time::Instant::now();
    debug!(%path, is_dir_hint, pane, "fm open_path start");
    let inner = live_inner(instance_id)?;
    let fp = orchid_fs::FsPath::new(path).map_err(map_fs_error)?;

    if orchid_fs::is_recycle_item(path) {
        orchid_fs::restore_recycle(&[path.to_string()])
            .await
            .map_err(map_fs_error)?;
        inner.refresh_all_tabs().await;
        return Ok(ActionOutcome::Done);
    }

    let is_dir = entry_is_directory(&inner, &fp, is_dir_hint).await;
    debug!(%path, is_dir, elapsed_ms = t0.elapsed().as_millis(), "fm open_path classified");

    if is_dir {
        if inner.is_path_encrypted(&fp) {
            return Ok(ActionOutcome::NeedsPassphrase {
                paths: vec![path.to_string()],
                purpose: PassphrasePurpose::Reveal,
            });
        }
        navigate_inner(instance_id, pane, fp, false).await?;
        debug!(%path, elapsed_ms = t0.elapsed().as_millis(), "fm open_path navigated");
        return Ok(ActionOutcome::Done);
    }

    if inner.is_path_encrypted(&fp) {
        return Ok(ActionOutcome::NeedsPassphrase {
            paths: vec![path.to_string()],
            purpose: PassphrasePurpose::RevealInViewer,
        });
    }

    if let Some(outcome) = archive::open_if_archive(&inner, instance_id, pane, &fp).await {
        return outcome;
    }

    inner.record_recent(&fp);
    debug!(%path, elapsed_ms = t0.elapsed().as_millis(), "fm open_path -> viewer");
    Ok(ActionOutcome::OpenInViewer {
        path: path.to_string(),
    })
}

async fn entry_is_directory(
    inner: &FileManagerInner,
    fp: &orchid_fs::FsPath,
    is_dir_hint: bool,
) -> bool {
    if is_virtual(fp) {
        let raw = fp.as_str();
        if orchid_fs::is_recycle_item(raw) {
            return false;
        }
        return is_dir_hint
            || category_for_virtual_path(raw).is_some()
            || label_key_for_virtual_path(raw).is_some()
            || find::is_search_virtual(raw);
    }
    if let Some(provider) = inner.deps.registry.for_path(fp) {
        if let Ok(meta) = provider.metadata(fp).await {
            return matches!(meta.kind, orchid_fs::FsEntryKind::Directory);
        }
    }
    // Provider metadata can fail under load; fall back to OS + UI hint.
    if let Ok(local) = fp.to_local() {
        if local.is_dir() {
            return true;
        }
        if local.is_file() {
            return false;
        }
    }
    is_dir_hint
}

async fn folder_path_from_target(
    inner: &FileManagerInner,
    path: &str,
) -> Option<orchid_fs::FsPath> {
    let fp = orchid_fs::FsPath::new(path).ok()?;
    if entry_is_directory(inner, &fp, false).await {
        Some(fp)
    } else {
        fp.parent()
    }
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
) -> WidgetResult<ActionOutcome> {
    transfer_into_directory(instance_id, sources, dest_dir, false).await
}

/// Copy `sources` into directory `dest_dir` (Ctrl+drag or Ctrl+OS drop).
pub async fn copy_paths_to_directory(
    instance_id: Uuid,
    sources: Vec<String>,
    dest_dir: &str,
) -> WidgetResult<ActionOutcome> {
    transfer_into_directory(instance_id, sources, dest_dir, true).await
}

async fn transfer_into_directory(
    instance_id: Uuid,
    sources: Vec<String>,
    dest_dir: &str,
    is_copy: bool,
) -> WidgetResult<ActionOutcome> {
    if sources.is_empty() {
        return Ok(ActionOutcome::Done);
    }
    let inner = live_inner(instance_id)?;
    let dest = orchid_fs::FsPath::new(dest_dir).map_err(map_fs_error)?;
    if let Some(provider) = inner.deps.registry.for_path(&dest) {
        let meta = provider.metadata(&dest).await.map_err(map_fs_error)?;
        if !matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-drop-not-directory".into(),
            ));
        }
    } else {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-drop-unavailable".into(),
        ));
    }
    inner.transfer_paths(&sources, &dest, is_copy).await
}

/// Apply a batch-rename pattern to `paths`.
pub async fn apply_batch_rename(
    instance_id: Uuid,
    paths: Vec<String>,
    pattern: &str,
    find: &str,
    replace: &str,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    for (i, p) in paths.iter().enumerate() {
        let old = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
        let Some(name) = old.file_name() else {
            continue;
        };
        let new_name = batch_rename::apply_rename_pattern(name, i, pattern, find, replace);
        if new_name.is_empty() || new_name == name {
            continue;
        }
        let Some(parent) = old.parent() else {
            continue;
        };
        let dest = parent.join(&new_name);
        let Some(provider) = inner.deps.registry.for_path(&old) else {
            continue;
        };
        provider.rename(&old, &dest).await.map_err(map_fs_error)?;
    }
    inner.refresh_all_tabs().await;
    Ok(())
}

async fn create_link_in_pane(
    instance_id: Uuid,
    pane: u8,
    paths: &[String],
    kind: &str,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let dest_dir = {
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
        tab.path.clone()
    };
    if is_virtual(&dest_dir) {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-transfer-virtual-dest".into(),
        ));
    }
    let Some(target_s) = paths.first() else {
        return Ok(());
    };
    let target = orchid_fs::FsPath::new(target_s).map_err(map_fs_error)?;
    let base = target.file_name().unwrap_or("link");
    let suffix = match kind {
        "fs.link-hard" => "hardlink",
        "fs.link-junction" => "junction",
        _ => "link",
    };
    let (stem, ext) = batch_rename::split_stem_ext(base);
    let link_name = format!("{stem} - {suffix}{ext}");
    let link = dest_dir.join(&link_name);
    match kind {
        "fs.link-hard" => orchid_fs::create_hard_link(&link, &target)
            .await
            .map_err(map_fs_error)?,
        "fs.link-junction" => orchid_fs::create_junction(&link, &target)
            .await
            .map_err(map_fs_error)?,
        _ => orchid_fs::create_symlink(&link, &target)
            .await
            .map_err(map_fs_error)?,
    }
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
        "fav:search" => orchid_fs::FsPath::new("virtual:search").ok(),
        "fav:recycle" => orchid_fs::FsPath::new("virtual:recycle").ok(),
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
        other if other.starts_with("drive:") => other
            .strip_prefix("drive:")
            .and_then(|p| orchid_fs::FsPath::new(p).ok()),
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

/// Notify every live file-manager instance that managed ingest started.
pub fn notify_managed_ingest_started(path: &orchid_fs::FsPath) {
    for entry in FM_LIVE.iter() {
        entry.value().handle_managed_ingest_started(path);
    }
}

/// Notify every live file-manager instance that managed ingest failed.
pub fn notify_managed_ingest_failed(path: &orchid_fs::FsPath) {
    for entry in FM_LIVE.iter() {
        entry.value().handle_managed_ingest_failed(path);
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

#[cfg(test)]
mod display_name_tests {
    use super::entry_display_name;

    #[test]
    fn keeps_extension_when_show_extensions() {
        assert_eq!(entry_display_name("readme.txt", false, true), "readme.txt");
    }

    #[test]
    fn strips_extension_when_hidden() {
        assert_eq!(entry_display_name("readme.txt", false, false), "readme");
    }

    #[test]
    fn keeps_dir_names_unchanged() {
        assert_eq!(entry_display_name("folder.txt", true, false), "folder.txt");
    }

    #[test]
    fn keeps_dotfiles_unchanged() {
        assert_eq!(entry_display_name(".gitignore", false, false), ".gitignore");
    }

    #[test]
    fn keeps_extensionless_files() {
        assert_eq!(entry_display_name("Makefile", false, false), "Makefile");
    }
}

#[cfg(test)]
mod type_column_tests {
    use super::type_column_extension;

    #[test]
    fn short_extensions_are_kept() {
        assert_eq!(type_column_extension("readme.txt"), Some("txt"));
        assert_eq!(type_column_extension("photo.jpeg"), Some("jpeg"));
        assert_eq!(type_column_extension("archive.tar.gz"), Some("gz"));
        assert_eq!(type_column_extension("Component.tsx"), Some("tsx"));
    }

    #[test]
    fn long_cache_extensions_are_rejected() {
        assert_eq!(
            type_column_extension("MachineId.v1.dotnetUserLevelCache"),
            None
        );
        assert_eq!(
            type_column_extension("7.0.302_IsDockerContainer.dotnetUserLevelCache"),
            None
        );
    }

    #[test]
    fn dotfiles_and_extensionless_are_rejected() {
        assert_eq!(type_column_extension(".gitignore"), None);
        assert_eq!(type_column_extension("Makefile"), None);
        assert_eq!(type_column_extension("trailing."), None);
    }
}

#[cfg(test)]
mod viewport_window_tests {
    use super::clamp_entry_window;

    #[test]
    fn missing_window_starts_at_top() {
        assert_eq!(clamp_entry_window(None, 200, 96), (0, 96));
        assert_eq!(clamp_entry_window(None, 40, 96), (0, 40));
    }

    #[test]
    fn stale_window_past_end_resets_to_top() {
        assert_eq!(clamp_entry_window(Some((200, 350)), 50, 96), (0, 50));
        assert_eq!(clamp_entry_window(Some((50, 50)), 50, 96), (0, 50));
    }

    #[test]
    fn valid_window_is_clamped_to_len() {
        assert_eq!(clamp_entry_window(Some((200, 350)), 300, 96), (200, 300));
        assert_eq!(clamp_entry_window(Some((0, 96)), 80, 96), (0, 80));
    }

    #[test]
    fn small_listing_ignores_stale_mid_window() {
        assert_eq!(clamp_entry_window(Some((10, 106)), 50, 96), (0, 50));
        assert_eq!(clamp_entry_window(Some((10, 106)), 90, 96), (10, 90));
    }
}
