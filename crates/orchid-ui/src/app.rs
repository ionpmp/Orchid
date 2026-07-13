//! Orchid application composition root.
//!
//! Wires the always-on subsystems — config, storage, event bus, locale,
//! theme — plus widget/workspace/terminal state for the desktop shell.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use tracing::{info, warn};
use uuid::Uuid;

use orchid_core::{
    CommandPalette, CommandRegistry, ConfigUpdated, Event, EventBus, EventBusConfig, EventFilter,
    HandlerPriority, SubscriptionHandle,
};
use orchid_i18n::{default_language, LocaleId, LocaleManager};
use orchid_fs::{FsProvider, FsProviderRegistry, LocalProvider, register_rclone_providers};
use orchid_storage::{ConfigLoader, ConfigWatcher, NetworkMountConfig, OrchidConfig, OrchidPaths, StateStore};
use orchid_terminal::{SessionManager, TerminalClipboardWrite};
use orchid_widgets::{
    builtin::search::{CommandsSource, FilesSource, SearchAggregator, SearchSource, SettingsSource},
    commands::build_command_set,
    GroupManager, LayoutEngine, WidgetManager, WidgetManagerOptions, WidgetRegistry, WorkspaceManager,
};

use crate::commands::build_ui_command_set;
use crate::error::{Result, UiError};
use crate::theme::ThemeManager;
use crate::widgets::terminal::ArboardClipboard;
use crate::widgets::terminal::{build_terminal_command_set, palette::palette_from_theme};
use crate::widgets::terminal::terminal_descriptor;
use crate::widgets::terminal::TerminalWidgetDeps;
use crate::window::main_window::MainWindowController;
use crate::window::startup::StartupWindowController;

/// Composition root of the Orchid application.
pub struct OrchidApp {
    #[allow(dead_code)]
    paths: OrchidPaths,
    config: Arc<RwLock<OrchidConfig>>,
    storage: Arc<StateStore>,
    bus: Arc<EventBus>,
    locale: Arc<LocaleManager>,
    theme: Arc<ThemeManager>,

    widget_manager: Arc<WidgetManager>,
    workspace_manager: Arc<WorkspaceManager>,
    layout_engine: Arc<LayoutEngine>,
    session_manager: Arc<SessionManager>,
    session_routing: Arc<Mutex<HashMap<Uuid, Uuid>>>,
    terminal_deps: TerminalWidgetDeps,
    /// Shared command registry (search + command palette UI).
    command_registry: Arc<CommandRegistry>,
    /// Fuzzy command search (palette + universal search commands source).
    command_palette: Arc<CommandPalette>,
    /// Group manager backing widget-related commands.
    #[allow(dead_code)]
    group_manager: Arc<GroupManager>,
    /// Filesystem provider registry (viewer, search, future file manager).
    #[allow(dead_code)]
    fs_registry: Arc<FsProviderRegistry>,
    /// Keeps the OSC 52 → system clipboard bus handler alive.
    _terminal_clipboard_sub: Option<SubscriptionHandle>,
    /// Refreshes file-manager UI when managed files are ingested.
    _managed_ingest_sub: Option<SubscriptionHandle>,
    _managed_ingest_started_sub: Option<SubscriptionHandle>,
    _managed_ingest_failed_sub: Option<SubscriptionHandle>,
    /// Shared network mount list (hot-reloaded from config.toml).
    #[allow(dead_code)] // held for lifetime; FM and rclone providers hold clones
    network_mounts: Arc<RwLock<Vec<NetworkMountConfig>>>,
    /// Keeps the Tantivy index scheduler workers alive.
    #[allow(dead_code)]
    _index_scheduler: Arc<orchid_search::IndexScheduler>,
    /// Keeps the FS→index bus subscriber alive.
    #[allow(dead_code)]
    _index_subscriber: orchid_search::IndexFsSubscriber,
    /// Active notify watches for search included-roots.
    #[allow(dead_code)]
    _index_watch_handles: Vec<orchid_fs::WatchHandle>,
    /// Application-wide recent-files list.
    recent_files: Arc<orchid_widgets::RecentFilesStore>,
    /// KDBX password vault (unlock via passphrase or Windows Hello).
    password_vault: Arc<orchid_crypto::PasswordVault>,
    /// FM encrypted-folder passphrase vault (Windows Hello).
    fm_passphrase_vault: Arc<orchid_crypto::FmPassphraseVault>,
    /// Keeps the config watcher background task alive.
    _config_watcher: ConfigWatcher,
}

impl std::fmt::Debug for OrchidApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchidApp")
            .field("theme", &self.theme.current().meta.id)
            .field("locale", &self.locale.current().as_str())
            .field("workspaces", &self.workspace_manager.list().len())
            .field("widgets", &self.widget_manager.list_instances().len())
            .finish_non_exhaustive()
    }
}

impl OrchidApp {
    /// Bring every subsystem online. Does not open any window.
    ///
    /// # Errors
    ///
    /// Propagates whichever subsystem refuses to start.
    pub async fn bootstrap(paths: OrchidPaths) -> Result<Self> {
        paths.ensure_directories()?;

        let config = Arc::new(RwLock::new(ConfigLoader::load_or_create(&paths.config_file)?));
        {
            let mut cfg = config.write();
            match orchid_crypto::protect_network_mount_passwords(
                &mut cfg.file_manager.network_mounts,
            ) {
                Ok(true) => {
                    if let Err(e) = ConfigLoader::save(&cfg, &paths.config_file) {
                        tracing::warn!(?e, "failed to rewrite DPAPI-protected mount passwords");
                    } else {
                        tracing::info!("migrated network mount passwords to DPAPI storage");
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(?e, "could not DPAPI-protect network mount passwords");
                }
            }
        }

        let storage = Arc::new(StateStore::open(
            &paths.state_db_path,
            env!("CARGO_PKG_VERSION"),
        )?);

        let bus = Arc::new(EventBus::new(EventBusConfig::default()));

        let initial_lang = {
            let cfg = config.read();
            LocaleId::parse(&cfg.locale.language).unwrap_or_else(|_| default_language())
        };

        let locale = Arc::new(LocaleManager::new(initial_lang, Some(paths.locales_dir.clone()))?);

        let theme_id = crate::system_theme::resolve_theme_id(&config.read().appearance);
        let theme = Arc::new(ThemeManager::new(Some(paths.themes_dir.clone()))?);
        if let Err(e) = theme.set_current(&theme_id) {
            tracing::warn!(
                configured = %theme_id,
                error = %e,
                "unknown theme id in config; using default"
            );
        }

        let session_routing: Arc<Mutex<HashMap<Uuid, Uuid>>> = Arc::new(Mutex::new(HashMap::new()));
        let session_manager: Arc<SessionManager> =
            Arc::new(SessionManager::new(bus.clone(), storage.clone()));
        let layout_engine: Arc<LayoutEngine> = Arc::new(LayoutEngine::default());

        let widget_registry: Arc<WidgetRegistry> = Arc::new(WidgetRegistry::new());

        let terminal_palette = Arc::new(RwLock::new(palette_from_theme(&theme.current())));
        let terminal_layouts: Arc<Mutex<HashMap<Uuid, orchid_terminal::LayoutRoot>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let terminal_deps = TerminalWidgetDeps {
            sessions: session_manager.clone(),
            palette: terminal_palette,
            bus: bus.clone(),
            storage: storage.clone(),
            session_routing: session_routing.clone(),
            layouts: terminal_layouts,
        };
        widget_registry
            .register(terminal_descriptor(terminal_deps.clone()))
            .map_err(|e| UiError::Slint(format!("register terminal: {e}")))?;

        let http = reqwest::Client::builder()
            .user_agent(format!("Orchid/{}", env!("CARGO_PKG_VERSION")))
            // Shared by weather + RSS; bounded waits keep tests and the UI
            // thread from depending on hung outbound connections.
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| UiError::Slint(format!("HTTP client: {e}")))?;

        let command_registry: Arc<CommandRegistry> = Arc::new(CommandRegistry::new());
        let command_palette: Arc<CommandPalette> =
            Arc::new(CommandPalette::new(command_registry.clone()));

        let search_index_dir = paths.data_dir.join("search_index");
        std::fs::create_dir_all(&search_index_dir).map_err(|e| {
            UiError::Slint(format!("create search index dir: {e}"))
        })?;
        let search_engine: Arc<orchid_search::SearchEngine> = Arc::new(
            orchid_search::SearchEngine::open(&search_index_dir)
                .map_err(|e| UiError::Slint(format!("open search index: {e}")))?,
        );
        let search_sources: Vec<Arc<dyn SearchSource>> = vec![
            Arc::new(FilesSource::new(search_engine.clone())),
            Arc::new(CommandsSource::new(command_palette.clone())),
            Arc::new(SettingsSource::new()),
        ];
        let search_aggregator: Arc<SearchAggregator> = Arc::new(SearchAggregator::new(search_sources));

        widget_registry
            .register(orchid_widgets::builtin::weather::descriptor(http.clone()))
            .map_err(|e| UiError::Slint(format!("register weather: {e}")))?;
        widget_registry
            .register(orchid_widgets::builtin::moon::descriptor())
            .map_err(|e| UiError::Slint(format!("register moon: {e}")))?;
        widget_registry
            .register(orchid_widgets::builtin::system::descriptor())
            .map_err(|e| UiError::Slint(format!("register system: {e}")))?;
        widget_registry
            .register(orchid_widgets::builtin::rss::descriptor(http))
            .map_err(|e| UiError::Slint(format!("register rss: {e}")))?;
        widget_registry
            .register(orchid_widgets::builtin::search::descriptor(search_aggregator))
            .map_err(|e| UiError::Slint(format!("register search: {e}")))?;

        widget_registry
            .register(orchid_widgets::builtin::media::descriptor())
            .map_err(|e| UiError::Slint(format!("register media: {e}")))?;

        // Password manager: needs an unlocked database + a secure clipboard.
        // For MVP we auto-create/unlock a dev database in debug builds. In release
        // builds, if the database can't be opened yet, we still register the
        // widget over an empty dev database so the UI can land; full unlock UI
        // is a later task.
        #[derive(Debug)]
        struct NullClipboard;
        #[async_trait::async_trait]
        impl orchid_crypto::SecureClipboard for NullClipboard {
            async fn copy_with_auto_clear(
                &self,
                _secret: secrecy::SecretString,
                _clear_after: std::time::Duration,
            ) -> orchid_crypto::Result<()> {
                Ok(())
            }
            async fn clear_if_ours(&self) -> orchid_crypto::Result<bool> {
                Ok(false)
            }
        }

        let (clipboard, terminal_clipboard_sub): (
            Arc<dyn orchid_crypto::SecureClipboard>,
            Option<SubscriptionHandle>,
        ) = match ArboardClipboard::new() {
            Ok(cb) => {
                let cb = Arc::new(cb);
                let sub = bus
                    .subscribe_sync(
                        EventFilter::of_type(TerminalClipboardWrite::event_type()),
                        HandlerPriority::Normal,
                        {
                            let cb = Arc::clone(&cb);
                            move |env| {
                                let Some(ev) = env.downcast::<TerminalClipboardWrite>() else {
                                    return;
                                };
                                if let Err(e) = cb.copy(&ev.text) {
                                    warn!(
                                        error = %e,
                                        session = %ev.session_id,
                                        "terminal OSC 52 clipboard write failed"
                                    );
                                }
                            }
                        },
                    )
                    .map_err(|e| UiError::Slint(format!("terminal clipboard bus sub: {e}")))?;
                (cb, Some(sub))
            }
            Err(e) => {
                warn!(error = %e, "clipboard unavailable; password copy will be disabled in this environment");
                (Arc::new(NullClipboard), None)
            }
        };

        let password_vault = orchid_crypto::PasswordVault::new(paths.data_dir.clone());
        let fm_passphrase_vault = orchid_crypto::FmPassphraseVault::new(paths.data_dir.clone());
        #[cfg(debug_assertions)]
        if !password_vault.db_exists() {
            use secrecy::SecretString;
            password_vault
                .unlock_with_passphrase(SecretString::new("orchid-dev".to_string()))
                .map_err(|e| UiError::Slint(format!("dev password vault: {e}")))?;
        }

        widget_registry
            .register(orchid_widgets::builtin::password::descriptor(
                password_vault.clone(),
                clipboard,
            ))
            .map_err(|e| UiError::Slint(format!("register password: {e}")))?;

        let fs_registry: Arc<FsProviderRegistry> = Arc::new(FsProviderRegistry::new());
        fs_registry
            .register(Arc::new(LocalProvider::new()) as Arc<dyn FsProvider>)
            .map_err(|e| UiError::Slint(format!("register local fs provider: {e}")))?;
        let network_mounts = Arc::new(RwLock::new(
            config.read().file_manager.network_mounts.clone(),
        ));
        register_rclone_providers(&fs_registry, network_mounts.clone())
            .map_err(|e| UiError::Slint(format!("register rclone providers: {e}")))?;
        let syntax_highlighter = Arc::new(orchid_viewers::SyntaxHighlighter::new());
        widget_registry
            .register(orchid_widgets::builtin::viewer::descriptor(
                orchid_widgets::builtin::viewer::ViewerDeps {
                    registry: fs_registry.clone(),
                    highlighter: syntax_highlighter,
                },
            ))
            .map_err(|e| UiError::Slint(format!("register viewer: {e}")))?;

        let tag_manager = Arc::new(orchid_fs::TagManager::new(storage.clone(), bus.clone()));

        // Live Tantivy indexing: subscribe to fs.* bus events, watch configured
        // roots, and seed the index with a bounded bootstrap crawl.
        let index_scheduler = Arc::new(orchid_search::IndexScheduler::new(
            search_engine.clone(),
            2,
        ));
        let mut index_extractor = orchid_search::Extractor::new();
        if config.read().search.extract_pdf {
            index_extractor = index_extractor.with_pdf();
        }
        let index_extractor = Arc::new(index_extractor);
        let index_subscriber = orchid_search::IndexFsSubscriber::new(
            bus.clone(),
            index_scheduler.clone(),
            index_extractor.clone(),
            fs_registry.clone(),
            tag_manager.clone(),
        );
        let (index_scope, index_roots) = build_search_index_scope(&config.read().search);
        index_subscriber.set_scope(index_scope.clone());
        index_subscriber
            .start()
            .await
            .map_err(|e| UiError::Slint(format!("search index subscriber: {e}")))?;

        let index_watcher =
            Arc::new(orchid_fs::FileWatcher::new(bus.clone(), fs_registry.clone()));
        let mut index_watch_handles = Vec::new();
        for root in &index_roots {
            match index_watcher.watch(root.clone()).await {
                Ok(handle) => index_watch_handles.push(handle),
                Err(e) => {
                    warn!(error = %e, %root, "search: failed to watch index root");
                }
            }
        }

        {
            let registry = fs_registry.clone();
            let scheduler = index_scheduler.clone();
            let extractor = index_extractor.clone();
            let tags = tag_manager.clone();
            let scope = index_scope;
            let roots = index_roots;
            tokio::spawn(async move {
                if let Err(e) = orchid_search::crawl_roots(
                    registry.as_ref(),
                    scheduler.as_ref(),
                    extractor.as_ref(),
                    tags.as_ref(),
                    &scope,
                    &roots,
                )
                .await
                {
                    warn!(error = %e, "search: bootstrap crawl failed");
                } else {
                    info!("search: bootstrap crawl finished");
                }
            });
        }

        let thumbnails = Arc::new(
            orchid_viewers::ThumbnailService::new(paths.cache_dir.join("thumbnails"))
                .map_err(|e| UiError::Slint(format!("thumbnail service: {e}")))?,
        );
        let chunk_store = Arc::new(
            orchid_crypto::ChunkStore::new(paths.chunks_dir.clone(), storage.clone())
                .map_err(|e| UiError::Slint(format!("chunk store: {e}")))?,
        );
        let deduplicator = Arc::new(
            orchid_crypto::Deduplicator::new(
                chunk_store.clone(),
                orchid_crypto::ChunkerConfig::default(),
            ),
        );
        let file_watcher_managed =
            Arc::new(orchid_fs::FileWatcher::new(bus.clone(), fs_registry.clone()));
        let reveal_manager = Arc::new(
            orchid_crypto::RevealManager::new(paths.cache_dir.join("reveal"), bus.clone()),
        );
        let file_watcher_encrypted =
            Arc::new(orchid_fs::FileWatcher::new(bus.clone(), fs_registry.clone()));
        let encrypted_engine = Arc::new(orchid_fs::EncryptedFolderEngine::new(
            storage.clone(),
            fs_registry.clone(),
            reveal_manager,
            bus.clone(),
            file_watcher_encrypted,
        ));
        encrypted_engine
            .start()
            .await
            .map_err(|e| UiError::Slint(format!("encrypted folder engine: {e}")))?;
        let managed_engine = Arc::new(orchid_fs::ManagedFolderEngine::new(
            storage.clone(),
            chunk_store,
            deduplicator,
            fs_registry.clone(),
            bus.clone(),
            file_watcher_managed,
        ));
        managed_engine
            .start()
            .await
            .map_err(|e| UiError::Slint(format!("managed folder engine: {e}")))?;
        let managed_ingest_sub = bus
            .subscribe_async(
                EventFilter::of_type(orchid_fs::ManagedFileIngestedEvent::event_type()),
                HandlerPriority::Normal,
                move |env| {
                    let path = env
                        .downcast_arc::<orchid_fs::ManagedFileIngestedEvent>()
                        .map(|e| e.path.clone());
                    async move {
                        if let Some(path) = path {
                            orchid_widgets::builtin::file_manager::notify_managed_ingest(&path);
                        }
                    }
                },
            )
            .map_err(|e| UiError::Slint(format!("managed ingest bus sub: {e}")))?;
        let managed_ingest_started_sub = bus
            .subscribe_async(
                EventFilter::of_type(orchid_fs::ManagedFileIngestStartedEvent::event_type()),
                HandlerPriority::Normal,
                move |env| {
                    let path = env
                        .downcast_arc::<orchid_fs::ManagedFileIngestStartedEvent>()
                        .map(|e| e.path.clone());
                    async move {
                        if let Some(path) = path {
                            orchid_widgets::builtin::file_manager::notify_managed_ingest_started(
                                &path,
                            );
                        }
                    }
                },
            )
            .map_err(|e| UiError::Slint(format!("managed ingest started sub: {e}")))?;
        let managed_ingest_failed_sub = bus
            .subscribe_async(
                EventFilter::of_type(orchid_fs::ManagedFileIngestFailedEvent::event_type()),
                HandlerPriority::Normal,
                move |env| {
                    let path = env
                        .downcast_arc::<orchid_fs::ManagedFileIngestFailedEvent>()
                        .map(|e| e.path.clone());
                    async move {
                        if let Some(path) = path {
                            orchid_widgets::builtin::file_manager::notify_managed_ingest_failed(
                                &path,
                            );
                        }
                    }
                },
            )
            .map_err(|e| UiError::Slint(format!("managed ingest failed sub: {e}")))?;
        let recent_files = orchid_widgets::RecentFilesStore::new(50);
        let fm_deps = orchid_widgets::builtin::file_manager::FileManagerDeps {
            registry: fs_registry.clone(),
            clipboard: Arc::new(orchid_widgets::builtin::file_manager::FileClipboard::new()),
            tag_manager,
            thumbnails,
            search: Some(search_engine.clone()),
            managed: Some(managed_engine),
            encrypted: Some(encrypted_engine),
            network_mounts: network_mounts.clone(),
            recent_files: recent_files.clone(),
            fm_passphrase_vault: fm_passphrase_vault.clone(),
            orchid_config: config.clone(),
            locale: locale.clone(),
        };
        widget_registry
            .register(orchid_widgets::builtin::file_manager::descriptor(fm_deps))
            .map_err(|e| UiError::Slint(format!("register file manager: {e}")))?;
        widget_registry
            .register(orchid_widgets::builtin::recent_files::descriptor(
                recent_files.clone(),
            ))
            .map_err(|e| UiError::Slint(format!("register recent files: {e}")))?;

        let widget_manager: Arc<WidgetManager> = Arc::new(WidgetManager::new(
            widget_registry,
            bus.clone(),
            storage.clone(),
            config.clone(),
            locale.clone(),
            WidgetManagerOptions::default(),
        ));

        let workspace_manager: Arc<WorkspaceManager> = Arc::new(WorkspaceManager::new(
            bus.clone(),
            storage.clone(),
        ));

        workspace_manager
            .restore_from_storage()
            .await
            .map_err(|e| UiError::Slint(format!("restore workspaces: {e}")))?;
        widget_manager
            .restore_from_storage()
            .await
            .map_err(|e| UiError::Slint(format!("restore widgets: {e}")))?;
        widget_manager
            .start()
            .await
            .map_err(|e| UiError::Slint(format!("widget sweeper: {e}")))?;
        widget_manager
            .prime_snapshot_caches()
            .await
            .map_err(|e| UiError::Slint(format!("prime widget snapshots: {e}")))?;

        let group_manager: Arc<GroupManager> = Arc::new(GroupManager::new(bus.clone(), storage.clone()));
        group_manager
            .restore_from_storage()
            .map_err(|e| UiError::Slint(format!("restore groups: {e}")))?;
        for group in group_manager.list_all() {
            for member_id in &group.members {
                if let Ok(inst) = widget_manager.get_instance(*member_id) {
                    *inst.group_id.write() = Some(group.id);
                }
            }
        }

        for (desc, factory) in build_command_set(
            widget_manager.clone(),
            workspace_manager.clone(),
            group_manager.clone(),
            widget_manager.registry().clone(),
        ) {
            let cmd_id = desc.id.clone();
            command_registry
                .register(desc, factory)
                .map_err(|e| UiError::Slint(format!("register command {cmd_id}: {e}")))?;
        }

        for (desc, factory) in build_terminal_command_set(
            terminal_deps.clone(),
            widget_manager.clone(),
            workspace_manager.clone(),
        ) {
            let cmd_id = desc.id.clone();
            command_registry
                .register(desc, factory)
                .map_err(|e| UiError::Slint(format!("register command {cmd_id}: {e}")))?;
        }

        for (desc, factory) in build_ui_command_set() {
            let cmd_id = desc.id.clone();
            command_registry
                .register(desc, factory)
                .map_err(|e| UiError::Slint(format!("register command {cmd_id}: {e}")))?;
        }

        apply_command_shortcut_overrides(&command_registry, &config.read().shortcuts.overrides);

        crate::autostart::sync_open_on_startup(&config.read().general);

        info!(
            theme = %theme.current().meta.id,
            language = %locale.current().as_str(),
            "orchid subsystems ready"
        );

        let (config_watcher, mut config_rx) = ConfigWatcher::start(paths.config_file.clone())
            .await
            .map_err(|e| UiError::Slint(format!("config watcher: {e}")))?;
        let config_reload = config.clone();
        let mounts_reload = network_mounts.clone();
        let bus_reload = bus.clone();
        tokio::spawn(async move {
            loop {
                match config_rx.recv().await {
                    Ok(new_cfg) => {
                        info!("config.toml reloaded");
                        *config_reload.write() = new_cfg.clone();
                        *mounts_reload.write() = new_cfg.file_manager.network_mounts.clone();
                        orchid_widgets::builtin::file_manager::refresh_all_instances().await;
                        bus_reload.publish(
                            orchid_core::EventSource::System,
                            ConfigUpdated,
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        Ok(Self {
            paths,
            config,
            storage,
            bus,
            locale,
            theme,
            widget_manager,
            workspace_manager,
            layout_engine,
            session_manager,
            session_routing,
            terminal_deps,
            command_registry,
            command_palette,
            group_manager,
            fs_registry,
            _terminal_clipboard_sub: terminal_clipboard_sub,
            _managed_ingest_sub: Some(managed_ingest_sub),
            _managed_ingest_started_sub: Some(managed_ingest_started_sub),
            _managed_ingest_failed_sub: Some(managed_ingest_failed_sub),
            network_mounts,
            _index_scheduler: index_scheduler,
            _index_subscriber: index_subscriber,
            _index_watch_handles: index_watch_handles,
            recent_files,
            password_vault,
            fm_passphrase_vault,
            _config_watcher: config_watcher,
        })
    }

    /// Open a path in a viewer widget on the active workspace.
    ///
    /// # Errors
    ///
    /// Returns [`UiError`] when there is no active workspace, widget creation fails, or the
    /// viewer cannot open the path.
    pub async fn open_in_viewer(&self, path: orchid_fs::FsPath) -> Result<Uuid, UiError> {
        let ws_id = self
            .workspace_manager
            .active()
            .map_err(|e| UiError::Slint(format!("no active workspace: {e}")))?
            .id;

        let viewer_ids: Vec<Uuid> = self
            .widget_manager
            .instances_for_workspace(ws_id)
            .into_iter()
            .filter(|i| i.type_id == orchid_widgets::builtin::viewer::TYPE_ID)
            .map(|i| i.id)
            .collect();

        if let Some(existing) =
            orchid_widgets::builtin::viewer::find_instance_for_path(&viewer_ids, &path)
        {
            self.recent_files.touch(&path, Some(&self.bus));
            return Ok(existing);
        }

        let id = self
            .widget_manager
            .create(orchid_widgets::CreateWidgetRequest {
                type_id: orchid_widgets::builtin::viewer::TYPE_ID.into(),
                workspace_id: ws_id,
                position: None,
                size: None,
                initial_lifecycle: None,
                config_bytes: None,
            })
            .await
            .map_err(|e| UiError::Slint(format!("viewer create: {e}")))?;

        for _ in 0..50 {
            if self.widget_manager.get_instance(id).is_ok()
                && orchid_widgets::builtin::viewer::set_floating_bounds(
                    id,
                    Some(orchid_widgets::PixelBounds {
                        x: 40.0,
                        y: 40.0,
                        width: 480.0,
                        height: 360.0,
                    }),
                )
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        orchid_widgets::builtin::viewer::open_path(id, path.clone())
            .await
            .map_err(|e| UiError::Slint(format!("viewer open: {e}")))?;

        self.recent_files.touch(&path, Some(&self.bus));

        Ok(id)
    }

    /// Open the main workspace window and run the Slint event loop.
    ///
    /// The caller in `orchid-app` should `await` [`OrchidApp::flush_after_window`]
    /// from the process [`tokio::runtime::Handle`] so widget and terminal
    /// cleanup runs *after* the event loop, without blocking the Tokio runtime
    /// from inside this crate.
    /// the `orchid-ui` sources (11B-Fix).
    pub fn run_main(&self) -> Result<()> {
        let c = MainWindowController::new(
            self.theme.clone(),
            self.locale.clone(),
            self.config.clone(),
            self.paths.config_file.clone(),
            self.recent_files.clone(),
            self.password_vault.clone(),
            self.fm_passphrase_vault.clone(),
            self.storage.clone(),
            self.bus.clone(),
            self.command_registry.clone(),
            self.command_palette.clone(),
            self.widget_manager.clone(),
            self.workspace_manager.clone(),
            self.layout_engine.clone(),
            self.group_manager.clone(),
            self.session_manager.clone(),
            self.session_routing.clone(),
            self.terminal_deps.clone(),
        )?;
        c.run()?;
        Ok(())
    }

    /// Best-effort shutdown of widget and terminal subsystems after the
    /// window has closed. Call from the binary, not from inside Slint.
    pub async fn flush_after_window(&self) {
        if let Err(e) = self.widget_manager.shutdown().await {
            warn!(%e, "widget manager shutdown");
        }
        if let Err(e) = self.session_manager.close_all().await {
            warn!(%e, "close terminal sessions");
        }
    }

    /// Open the startup window and run the Slint event loop until the
    /// window closes.
    ///
    /// # Errors
    ///
    /// Propagates any controller-creation or event-loop failure.
    pub fn run_startup(self) -> Result<()> {
        info!("opening startup window");
        let controller = StartupWindowController::new(
            self.theme.clone(),
            self.locale.clone(),
            self.config.clone(),
            self.bus.clone(),
        )?;
        controller.run()
    }

    /// Shared theme manager.
    #[must_use]
    pub fn theme(&self) -> &Arc<ThemeManager> {
        &self.theme
    }

    /// Shared locale manager.
    #[must_use]
    pub fn locale(&self) -> &Arc<LocaleManager> {
        &self.locale
    }

    /// Shared configuration.
    #[must_use]
    pub fn config(&self) -> &Arc<RwLock<OrchidConfig>> {
        &self.config
    }

    /// Shared event bus.
    #[must_use]
    pub fn bus(&self) -> &Arc<EventBus> {
        &self.bus
    }

    /// Shared state store.
    #[must_use]
    pub fn storage(&self) -> &Arc<StateStore> {
        &self.storage
    }

    /// Widget instance manager.
    #[must_use]
    pub fn widget_manager(&self) -> &Arc<WidgetManager> {
        &self.widget_manager
    }

    /// Workspace (virtual desktop) manager.
    #[must_use]
    pub fn workspace_manager(&self) -> &Arc<WorkspaceManager> {
        &self.workspace_manager
    }

    /// Registered commands (palette, shortcuts, gestures).
    #[must_use]
    pub fn command_registry(&self) -> &Arc<CommandRegistry> {
        &self.command_registry
    }
}

fn apply_command_shortcut_overrides(
    registry: &CommandRegistry,
    overrides: &std::collections::HashMap<String, String>,
) {
    if overrides.is_empty() {
        return;
    }
    for result in registry.apply_shortcut_overrides(overrides) {
        if let Err(reason) = result.outcome {
            tracing::warn!(
                command = %result.command_id,
                reason = %reason,
                "shortcut override rejected"
            );
        }
    }
}

/// Resolve `[search]` config into an [`IndexScope`] plus concrete roots to watch.
fn build_search_index_scope(
    cfg: &orchid_storage::SearchConfig,
) -> (orchid_search::IndexScope, Vec<orchid_fs::FsPath>) {
    let mut roots = Vec::new();
    for raw in &cfg.included_roots {
        match orchid_fs::FsPath::new(raw.trim()) {
            Ok(p) => roots.push(p),
            Err(e) => {
                warn!(error = %e, root = %raw, "search: invalid included-roots entry");
            }
        }
    }
    if roots.is_empty() {
        if let Some(docs) = directories::UserDirs::new().and_then(|u| u.document_dir().map(PathBuf::from))
        {
            match orchid_fs::FsPath::from_local(&docs) {
                Ok(p) => {
                    info!(%p, "search: using Documents folder as default index root");
                    roots.push(p);
                }
                Err(e) => {
                    warn!(error = %e, "search: could not convert Documents path");
                }
            }
        } else {
            warn!("search: no included-roots and Documents folder unavailable; index stays empty until configured");
        }
    }

    let max_file_size = cfg.max_file_size_mib.saturating_mul(1024 * 1024);
    let scope = orchid_search::IndexScope {
        included_roots: roots.clone(),
        excluded_patterns: cfg.excluded_patterns.clone(),
        max_file_size,
        extract_text_content: cfg.extract_text,
        extract_pdf_content: cfg.extract_pdf,
    };
    (scope, roots)
}

// Keep the import graph obvious for maintainers.
#[allow(dead_code)]
fn _require_uierror_visibility(_: UiError) {}
