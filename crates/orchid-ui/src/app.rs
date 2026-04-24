//! Orchid application composition root.
//!
//! Wires the always-on subsystems — config, storage, event bus, locale,
//! theme — plus widget/workspace/terminal state for the desktop shell.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use tracing::{info, warn};
use uuid::Uuid;

use orchid_core::{EventBus, EventBusConfig};
use orchid_i18n::{default_language, LocaleId, LocaleManager};
use orchid_storage::{ConfigLoader, OrchidConfig, OrchidPaths, StateStore};
use orchid_terminal::SessionManager;
use orchid_widgets::{
    LayoutEngine, WidgetManager, WidgetManagerOptions, WidgetRegistry, WorkspaceManager,
};

use crate::error::{Result, UiError};
use crate::theme::ThemeManager;
use crate::widgets::terminal::palette::palette_from_theme;
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

        let theme_id = config.read().appearance.theme.clone();
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
        let terminal_deps = TerminalWidgetDeps {
            sessions: session_manager.clone(),
            palette: terminal_palette,
            bus: bus.clone(),
            storage: storage.clone(),
            session_routing: session_routing.clone(),
        };
        widget_registry
            .register(terminal_descriptor(terminal_deps))
            .map_err(|e| UiError::Slint(format!("register terminal: {e}")))?;

        let http = reqwest::Client::builder()
            .user_agent(format!("Orchid/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| UiError::Slint(format!("HTTP client: {e}")))?;
        widget_registry
            .register(orchid_widgets::builtin::weather::descriptor(http))
            .map_err(|e| UiError::Slint(format!("register weather: {e}")))?;
        widget_registry
            .register(orchid_widgets::builtin::moon::descriptor())
            .map_err(|e| UiError::Slint(format!("register moon: {e}")))?;
        widget_registry
            .register(orchid_widgets::builtin::system::descriptor())
            .map_err(|e| UiError::Slint(format!("register system: {e}")))?;

        let widget_manager: Arc<WidgetManager> = Arc::new(WidgetManager::new(
            widget_registry,
            bus.clone(),
            storage.clone(),
            config.clone(),
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

        info!(
            theme = %theme.current().meta.id,
            language = %locale.current().as_str(),
            "orchid subsystems ready"
        );

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
        })
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
            self.bus.clone(),
            self.widget_manager.clone(),
            self.workspace_manager.clone(),
            self.layout_engine.clone(),
            self.session_manager.clone(),
            self.session_routing.clone(),
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
}

// Keep the import graph obvious for maintainers.
#[allow(dead_code)]
fn _require_uierror_visibility(_: UiError) {}
