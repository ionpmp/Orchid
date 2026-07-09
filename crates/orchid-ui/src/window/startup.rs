//! Startup-window controller.
//!
//! Owns the generated Slint [`StartupWindow`] handle and populates its
//! Theme / Strings / AppState globals. Subscribes to [`ConfigUpdated`] so
//! theme, locale, and density hot-reload while the window is open.

use std::sync::Arc;

use parking_lot::RwLock;
use slint::ComponentHandle;
use slint::{ModelRc, VecModel};
use tracing::{info, warn};

use orchid_core::{
    ConfigUpdated, Event, EventBus, EventFilter, HandlerPriority, SubscriptionHandle,
};
use orchid_i18n::{FluentArgs, LocaleId, LocaleManager};
use orchid_storage::OrchidConfig;

use crate::error::{Result, UiError};
use crate::theme::ThemeManager;

use crate::slint_generated::WorkspaceModel;
use crate::slint_generated::{AppState, DockWidgetType, StartupWindow, Strings, Theme};

/// Wraps the [`StartupWindow`] with enough dependencies to push every
/// global value, including hot-reload after `config.toml` changes.
pub struct StartupWindowController {
    window: StartupWindow,
    theme: Arc<ThemeManager>,
    locale: Arc<LocaleManager>,
    config: Arc<RwLock<OrchidConfig>>,
    #[allow(dead_code)]
    bus: Arc<EventBus>,
    _config_reload_sub: SubscriptionHandle,
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

        let weak = window.as_weak();
        let theme_sub = theme.clone();
        let locale_sub = locale.clone();
        let config_sub = config.clone();
        let config_reload_sub = bus
            .subscribe_async(
                EventFilter::of_type(ConfigUpdated::event_type()),
                HandlerPriority::Normal,
                move |_env| {
                    let weak = weak.clone();
                    let theme = theme_sub.clone();
                    let locale = locale_sub.clone();
                    let config = config_sub.clone();
                    async move {
                        let _ = slint::invoke_from_event_loop(move || {
                            let Some(window) = weak.upgrade() else {
                                return;
                            };
                            if let Err(e) =
                                apply_hot_config_to_startup(&window, &theme, &locale, &config)
                            {
                                warn!(?e, "startup window config hot-reload");
                            }
                        });
                    }
                },
            )
            .map_err(|e| UiError::Slint(format!("startup config reload sub: {e}")))?;

        let controller = Self {
            window,
            theme,
            locale,
            config,
            bus,
            _config_reload_sub: config_reload_sub,
        };
        controller.apply_theme();
        controller.apply_strings();
        controller.apply_app_state();
        Ok(controller)
    }

    fn apply_theme(&self) {
        apply_theme_to(&self.window, &self.theme, &self.config);
    }

    fn apply_strings(&self) {
        apply_strings_to(&self.window, &self.locale);
    }

    fn apply_app_state(&self) {
        apply_app_state_to(&self.window, &self.theme, &self.locale, &self.config);
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

fn apply_hot_config_to_startup(
    window: &StartupWindow,
    theme: &ThemeManager,
    locale: &LocaleManager,
    config: &RwLock<OrchidConfig>,
) -> Result<()> {
    let cfg = config.read();
    if let Ok(lang) = LocaleId::parse(&cfg.locale.language) {
        locale.set_current(lang);
    }
    let theme_id = crate::system_theme::resolve_theme_id(&cfg.appearance);
    if let Err(e) = theme.set_current(&theme_id) {
        warn!(
            configured = %theme_id,
            error = %e,
            "unknown theme id after config reload (startup)"
        );
    }
    drop(cfg);
    apply_theme_to(window, theme, config);
    apply_strings_to(window, locale);
    apply_app_state_to(window, theme, locale, config);
    Ok(())
}

fn apply_theme_to(
    window: &StartupWindow,
    theme: &ThemeManager,
    config: &RwLock<OrchidConfig>,
) {
    let theme = theme.current();
    let tokens = &theme.tokens;
    let cfg = config.read();
    let scale = cfg.appearance.density.ui_scale() * cfg.appearance.font_scale.clamp(0.75, 2.0);
    let reduce_motion = cfg.appearance.reduce_motion;
    let font_sans = crate::system_theme::resolve_font_family_sans(
        &cfg.appearance,
        &tokens.typography.font_family_sans,
    );
    drop(cfg);
    let g = window.global::<Theme>();
    let c = &tokens.color;

    g.set_surface_base(c.surface_base.to_slint());
    g.set_surface_raised(c.surface_raised.to_slint());
    g.set_text_primary(c.text_primary.to_slint());
    g.set_text_secondary(c.text_secondary.to_slint());
    g.set_text_tertiary(c.text_tertiary.to_slint());
    g.set_accent_brand(c.accent_brand.to_slint());
    g.set_border_default(c.border_default.to_slint());

    g.set_font_family_sans(font_sans.into());
    g.set_font_family_mono(tokens.typography.font_family_mono.clone().into());
    g.set_font_size_sm(tokens.typography.size_sm * scale);
    g.set_font_size_md(tokens.typography.size_md * scale);
    g.set_font_size_lg(tokens.typography.size_lg * scale);
    g.set_font_size_xl(tokens.typography.size_xl * scale);
    g.set_font_size_2xl(tokens.typography.size_2xl * scale);
    g.set_font_size_3xl(tokens.typography.size_3xl * scale);
    g.set_weight_regular(i32::from(tokens.typography.weight_regular));
    g.set_weight_medium(i32::from(tokens.typography.weight_medium));
    g.set_weight_semibold(i32::from(tokens.typography.weight_semibold));

    g.set_radius_md(tokens.radius.md * scale);
    g.set_spacing_unit(tokens.spacing.unit * scale);
    g.set_reduce_motion(reduce_motion);
}

fn apply_strings_to(window: &StartupWindow, mgr: &LocaleManager) {
    let g = window.global::<Strings>();

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
    g.set_widget_close_tooltip(mgr.tr("widget-close-tooltip").into());

    g.set_password_locked(mgr.tr("password-locked").into());
    g.set_password_no_entries(mgr.tr("password-no-entries").into());
    g.set_password_search_placeholder(mgr.tr("password-search-placeholder").into());
    g.set_password_select_entry(mgr.tr("password-select-entry").into());
    g.set_password_label_username(mgr.tr("password-label-username").into());
    g.set_password_label_password(mgr.tr("password-label-password").into());
    g.set_password_label_url(mgr.tr("password-label-url").into());
    g.set_password_label_notes(mgr.tr("password-label-notes").into());
    g.set_password_label_totp(mgr.tr("password-label-totp").into());
    g.set_password_action_lock(mgr.tr("password-action-lock").into());
    g.set_password_unlock_label(mgr.tr("password-unlock-label").into());
    g.set_password_unlock_placeholder(mgr.tr("password-unlock-placeholder").into());
    g.set_password_unlock_submit(mgr.tr("password-unlock-submit").into());
    g.set_password_unlock_biometric(mgr.tr("password-unlock-biometric").into());
    g.set_password_action_add(mgr.tr("password-action-add").into());
}

fn apply_app_state_to(
    window: &StartupWindow,
    theme: &ThemeManager,
    locale: &LocaleManager,
    config: &RwLock<OrchidConfig>,
) {
    let g = window.global::<AppState>();
    let theme = theme.current();
    let language = locale.current();
    let density = config.read().appearance.density;

    let density_key = match density {
        orchid_storage::Density::Touch => "density-touch",
        orchid_storage::Density::Mouse => "density-mouse",
        orchid_storage::Density::Hybrid => "density-hybrid",
    };

    g.set_current_theme_id(theme.meta.display_name.clone().into());
    g.set_current_language({
        let key = format!("locale-name-{language}");
        let name = locale.tr(&key);
        if name == key {
            language.as_str().into()
        } else {
            name.into()
        }
    });
    g.set_current_density(locale.tr(density_key).into());
    let is_rtl = language.as_str().to_ascii_lowercase().starts_with("ar");
    g.set_is_rtl(is_rtl);
    let cfg = config.read();
    let swap_edges = matches!(cfg.input.primary_hand, orchid_storage::Hand::Left)
        || cfg.input.mirror_edge_swipes;
    g.set_edge_panels_mirrored(is_rtl ^ swap_edges);
    g.set_mode(0);
    g.set_workspace(WorkspaceModel {
        workspaces: ModelRc::new(VecModel::default()),
        active_workspace_id: "".into(),
        widgets: ModelRc::new(VecModel::default()),
        dock_types: ModelRc::new(VecModel::from(vec![DockWidgetType {
            type_id: "terminal".into(),
            label: locale.tr("dock-widget-terminal").into(),
            icon: "terminal".into(),
        }])),
        dock_add_label: locale.tr("dock-add-label").into(),
        grid_columns: 16,
        grid_rows: 10,
        canvas_content_width: 1f32,
        canvas_content_height: 1f32,
    });
}
