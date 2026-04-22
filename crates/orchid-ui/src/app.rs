//! Orchid application composition root.
//!
//! Wires the always-on subsystems — config, storage, event bus, locale,
//! theme — and hands back a builder that the binary drives. Widget /
//! workspace / terminal bootstrapping is deliberately out of scope for
//! task 11A; they land in 11B.

use std::sync::Arc;

use parking_lot::RwLock;
use tracing::info;

use orchid_core::{EventBus, EventBusConfig};
use orchid_i18n::{default_language, LocaleId, LocaleManager};
use orchid_storage::{ConfigLoader, OrchidConfig, OrchidPaths, StateStore};

use crate::error::{Result, UiError};
use crate::theme::ThemeManager;
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
}

impl std::fmt::Debug for OrchidApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchidApp")
            .field("theme", &self.theme.current().meta.id)
            .field("locale", &self.locale.current().as_str())
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
            // Unknown theme id in config — fall back to the default dark
            // theme rather than refusing to start.
            tracing::warn!(
                configured = %theme_id,
                error = %e,
                "unknown theme id in config; using default"
            );
        }

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
        })
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
}

// Keep the import graph obvious for maintainers.
#[allow(dead_code)]
fn _require_uierror_visibility(_: UiError) {}
