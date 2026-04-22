//! Startup-window controller.
//!
//! Owns the generated Slint [`StartupWindow`] handle and populates its
//! Theme / Strings / AppState globals once on construction. Hot-reload
//! of any of these is deferred to a later task — a single initial
//! apply is all the MVP window requires.

use std::sync::Arc;

use parking_lot::RwLock;
use slint::ComponentHandle;
use slint::{ModelRc, VecModel};
use tracing::info;

use orchid_core::EventBus;
use orchid_i18n::{FluentArgs, LocaleManager};
use orchid_storage::OrchidConfig;

use crate::error::{Result, UiError};
use crate::theme::ThemeManager;

use crate::slint_generated::{AppState, DockWidgetType, StartupWindow, Strings, Theme};
use crate::slint_generated::WorkspaceModel;

/// Wraps the [`StartupWindow`] with enough dependencies to push every
/// global value once.
pub struct StartupWindowController {
    window: StartupWindow,
    theme: Arc<ThemeManager>,
    locale: Arc<LocaleManager>,
    config: Arc<RwLock<OrchidConfig>>,
    #[allow(dead_code)]
    bus: Arc<EventBus>,
}

impl StartupWindowController {
    /// Build the window and apply every Slint global.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Slint`] when Slint window creation fails.
    pub fn new(
        theme: Arc<ThemeManager>,
        locale: Arc<LocaleManager>,
        config: Arc<RwLock<OrchidConfig>>,
        bus: Arc<EventBus>,
    ) -> Result<Self> {
        let window = StartupWindow::new()
            .map_err(|e| UiError::Slint(format!("failed to create window: {e}")))?;

        let controller = Self {
            window,
            theme,
            locale,
            config,
            bus,
        };
        controller.apply_theme();
        controller.apply_strings();
        controller.apply_app_state();
        Ok(controller)
    }

    fn apply_theme(&self) {
        let theme = self.theme.current();
        let g = self.window.global::<Theme>();
        let tokens = &theme.tokens;
        let c = &tokens.color;

        g.set_surface_base(c.surface_base.to_slint());
        g.set_surface_raised(c.surface_raised.to_slint());
        g.set_text_primary(c.text_primary.to_slint());
        g.set_text_secondary(c.text_secondary.to_slint());
        g.set_text_tertiary(c.text_tertiary.to_slint());
        g.set_accent_brand(c.accent_brand.to_slint());
        g.set_border_default(c.border_default.to_slint());

        g.set_font_family_sans(tokens.typography.font_family_sans.clone().into());
        g.set_font_family_mono(tokens.typography.font_family_mono.clone().into());
        g.set_font_size_sm(tokens.typography.size_sm);
        g.set_font_size_md(tokens.typography.size_md);
        g.set_font_size_lg(tokens.typography.size_lg);
        g.set_font_size_xl(tokens.typography.size_xl);
        g.set_font_size_2xl(tokens.typography.size_2xl);
        g.set_font_size_3xl(tokens.typography.size_3xl);
        g.set_weight_regular(i32::from(tokens.typography.weight_regular));
        g.set_weight_medium(i32::from(tokens.typography.weight_medium));
        g.set_weight_semibold(i32::from(tokens.typography.weight_semibold));

        g.set_radius_md(tokens.radius.md);
        g.set_spacing_unit(tokens.spacing.unit);
    }

    fn apply_strings(&self) {
        let g = self.window.global::<Strings>();
        let mgr = &self.locale;

        g.set_window_title(mgr.tr("window-title").into());
        g.set_welcome(mgr.tr("startup-welcome").into());
        g.set_subtitle(mgr.tr("startup-subtitle").into());

        let version = env!("CARGO_PKG_VERSION");
        let args = FluentArgs::new().with("version", version);
        g.set_version_label(mgr.tr_args("startup-version-label", &args).into());

        g.set_theme_label(mgr.tr("status-theme").into());
        g.set_language_label(mgr.tr("status-language").into());
        g.set_density_label(mgr.tr("status-density").into());
        g.set_get_started_label(mgr.tr("startup-get-started").into());
        g.set_workspace_new_label(mgr.tr("workspace-new").into());
        g.set_dock_add_label(mgr.tr("dock-add-label").into());
        g.set_widget_close_tooltip(mgr.tr("widget-close-tooltip").into());
    }

    fn apply_app_state(&self) {
        let g = self.window.global::<AppState>();
        let theme = self.theme.current();
        let language = self.locale.current();
        let density = self.config.read().appearance.density;

        let density_key = match density {
            orchid_storage::Density::Touch => "density-touch",
            orchid_storage::Density::Mouse => "density-mouse",
            orchid_storage::Density::Hybrid => "density-hybrid",
        };

        g.set_current_theme_id(theme.meta.id.clone().into());
        g.set_current_language(language.as_str().into());
        g.set_current_density(self.locale.tr(density_key).into());
        g.set_mode(0);
        g.set_workspace(WorkspaceModel {
            workspaces: ModelRc::new(VecModel::default()),
            active_workspace_id: "".into(),
            widgets: ModelRc::new(VecModel::default()),
            dock_types: ModelRc::new(VecModel::from(vec![DockWidgetType {
                type_id: "terminal".into(),
                label: self.locale.tr("dock-widget-terminal").into(),
                icon: "terminal".into(),
            }])),
            dock_add_label: self.locale.tr("dock-add-label").into(),
            grid_columns: 16,
            grid_rows: 10,
        });
    }

    /// Show the window and enter the Slint event loop until it closes.
    ///
    /// # Errors
    ///
    /// Propagates [`UiError::Slint`] when the event loop misbehaves.
    pub fn run(self) -> Result<()> {
        info!("Showing startup window");

        self.window
            .show()
            .map_err(|e| UiError::Slint(format!("window show failed: {e}")))?;

        slint::run_event_loop()
            .map_err(|e| UiError::Slint(format!("event loop error: {e}")))?;

        info!("Startup window closed");
        Ok(())
    }
}
