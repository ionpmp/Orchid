//! File-manager widget.

pub mod archive;
pub mod batch_rename;
pub mod clipboard;
pub mod config;
pub mod context_menu;
pub mod find;
mod image_batch;
mod image_edit;
mod image_print;
mod image_share;
mod wrap_orchid;
pub(crate) use image_share::{copy_loaded, paste_loaded};
mod meta_edit;
pub mod navigation;
pub mod selection;
pub mod state;
pub mod tools;
pub mod transfer;
mod undo;
pub mod view_mode;
pub mod virtual_folders;
pub mod visit_log;

mod actions;
mod api;
mod inner;
mod listing;

pub use actions::*;
pub use api::*;
pub(crate) use api::{
    active_tab_path, active_tab_ref, clamp_entry_window, create_link_in_pane, entry_is_directory,
    folder_path_from_target, network_mount_display_name, FM_VIRTUALIZE_THRESHOLD,
};
pub use listing::descriptor;
pub(crate) use listing::*;

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
    /// Play selected audio files in the Audio Player widget (queue replace + play).
    PlayInAudioPlayer {
        paths: Vec<String>,
    },
    /// Append selected audio files to the Audio Player queue without starting playback.
    EnqueueInAudioPlayer {
        paths: Vec<String>,
    },
    /// Play selected video files in the Video Player widget.
    PlayInVideoPlayer {
        paths: Vec<String>,
    },
    /// Append selected video files to the Video Player queue.
    EnqueueInVideoPlayer {
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
    /// Chunk store for linked `.orchid` wraps inside managed folders.
    pub chunk_store: Option<Arc<orchid_crypto::ChunkStore>>,
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
/// Shared file-manager core keyed from UI callbacks.
pub(crate) struct FileManagerInner {
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
    /// Bumped when a tab starts listing so a stale preview cannot overwrite a newer nav.
    listing_epoch: RwLock<HashMap<Uuid, u64>>,
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
    /// Generation counter for coalescing decoration (icon/thumb) snapshot publishes.
    decoration_publish_gen: AtomicU64,
    /// Folder visit counts for the history dropdown.
    visit_log: parking_lot::Mutex<VisitLog>,
    /// Runtime find / duplicate / large-file result sets (`virtual:search/<id>`).
    search_sessions: RwLock<HashMap<Uuid, find::SearchSession>>,
    /// Session-only undo / redo for copy, move, rename, create, and recycle.
    undo: parking_lot::Mutex<undo::FsUndoStack>,
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

/// Byte budget for the shell icon cache.
///
/// A flat entry count cannot span the size buckets: a 32px icon is 4 KB while
/// a jumbo one is 256 KB, so 1024 entries is either a rounding error or a
/// quarter of a gigabyte depending on the view mode.
const SHELL_ICON_CACHE_BYTES: usize = 48 * 1024 * 1024;
const THUMBNAIL_CACHE_CAP: usize = 256;

/// Pick the icon bucket one step above the drawn size, never below it.
///
/// Downscaling stays sharp, upscaling does not, and the drawn size is in
/// logical pixels that the display scale multiplies again: rows draw 20pt,
/// which is already 30px at 150%, and icon tiles draw roughly 60–100pt.
fn shell_icon_size_for_mode(mode: ViewMode) -> orchid_fs::ShellIconSize {
    match mode {
        // Gallery tiles are large; jumbo sources stay sharp after crop.
        ViewMode::Gallery => orchid_fs::ShellIconSize::Jumbo,
        // Icons view draws ~60–100 logical px — ExtraLarge (48) is enough and
        // avoids the COM/DIB cost of 256px for every visible tile.
        ViewMode::Icons => orchid_fs::ShellIconSize::ExtraLarge,
        // Rows draw ~20 logical px; at 200% DPI that is 40 physical, so 48px
        // sources stay sharp where 32px would upscale.
        ViewMode::List | ViewMode::Details => orchid_fs::ShellIconSize::ExtraLarge,
    }
}

/// Max edge length stored in the shell-icon cache for a view mode.
///
/// Slint scales on every paint; keeping decoded RGBA near the drawn size cuts
/// both cache pressure and hover/scroll frame cost.
fn shell_icon_display_px(mode: ViewMode) -> u32 {
    match mode {
        ViewMode::Gallery => 192,
        ViewMode::Icons => 96,
        ViewMode::List | ViewMode::Details => 40,
    }
}

fn downscale_shell_icon(icon: orchid_fs::ShellIcon, max_edge: u32) -> orchid_fs::ShellIcon {
    if max_edge == 0 || (icon.width <= max_edge && icon.height <= max_edge) {
        return icon;
    }
    let Some(img) = image::RgbaImage::from_raw(icon.width, icon.height, (*icon.rgba).clone())
    else {
        return icon;
    };
    let resized = image::DynamicImage::ImageRgba8(img)
        .resize(max_edge, max_edge, image::imageops::FilterType::Triangle)
        .into_rgba8();
    let (w, h) = resized.dimensions();
    orchid_fs::ShellIcon {
        rgba: Arc::new(resized.into_raw()),
        width: w,
        height: h,
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

/// Insert into an LRU cache bounded by decoded bytes rather than entry count.
fn insert_capped_icon(
    map: &mut HashMap<String, orchid_viewers::Thumbnail>,
    order: &mut VecDeque<String>,
    key: String,
    value: orchid_viewers::Thumbnail,
    byte_budget: usize,
) {
    if !map.contains_key(&key) {
        order.push_back(key.clone());
    }
    map.insert(key, value);
    let mut used: usize = map.values().map(|t| t.rgba.len()).sum();
    while used > byte_budget {
        let Some(old) = order.pop_front() else {
            break;
        };
        if let Some(dropped) = map.remove(&old) {
            used = used.saturating_sub(dropped.rgba.len());
        }
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
                listing_epoch: RwLock::new(HashMap::new()),
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
                decoration_publish_gen: AtomicU64::new(0),
                visit_log: parking_lot::Mutex::new(VisitLog::from_entries(persisted.path_visits)),
                search_sessions: RwLock::new(HashMap::new()),
                undo: parking_lot::Mutex::new(undo::FsUndoStack::default()),
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

        if let Some(rt) = right {
            tokio::join!(
                self.inner.refresh_tab(&left, show_hidden),
                self.inner.refresh_tab(&rt, show_hidden)
            );
        } else {
            self.inner.refresh_tab(&left, show_hidden).await;
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
