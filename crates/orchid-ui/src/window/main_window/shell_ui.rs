//! Settings, navigation, notifications, onboarding, and command-palette shell UI.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use slint::ComponentHandle;
use slint::Model;
use slint::SharedString;
use slint::VecModel;
use tracing::warn;

use orchid_i18n::LocaleManager;
use orchid_storage::{ConfigLoader, OrchidConfig};

use crate::error::{Result, UiError};
use crate::slint_generated::{
    AppState, CommandPaletteGlobal, NavigationGlobal, NotificationGlobal, NotificationItem,
    OnboardingGlobal, SearchModel, SettingsGlobal,
};
use crate::window::errors::{storage_localized_error, ui_localized_error};
use crate::window::models::{
    build_palette_candidates, build_settings_fields, build_settings_sections,
    settings_section_id, settings_section_index, SETTINGS_SECTION_IDS,
};
use crate::window::spawn;

use super::{AddWidgetPlacement, MainWindowController, WORKSPACE_SWITCHER_H, sync_vec_model};

const COMMAND_PALETTE_LIMIT: usize = 50;

const ONBOARDING_STEP_COUNT: i32 = 4;
const ONBOARDING_STEP_KEYS: [(&str, &str); 4] = [
    ("onboarding-step-welcome-title", "onboarding-step-welcome-body"),
    ("onboarding-step-workspace-title", "onboarding-step-workspace-body"),
    ("onboarding-step-palette-title", "onboarding-step-palette-body"),
    ("onboarding-step-gestures-title", "onboarding-step-gestures-body"),
];

/// Soft cap so bridges/toasts cannot grow the in-memory list without bound.
pub(super) const NOTIFICATION_LIST_CAP: usize = 50;

impl MainWindowController {
    pub(super) fn sync_settings_global(self: &Arc<Self>) {
        let st = self.settings.read().clone();
        let section = if st.section.is_empty() {
            SETTINGS_SECTION_IDS[0].to_string()
        } else {
            st.section.clone()
        };
        let title_key = format!("settings.section.{}", section);
        let title = self.locale.tr(&title_key).into();
        let hint = self.locale.tr("settings-panel-hint").into();
        // Shortcuts (and similar) are view-only in the panel — surface the
        // dedicated coming-soon copy so users know to edit config.toml.
        let coming_soon = if section == "shortcuts" {
            self.locale.tr("settings-panel-coming-soon").into()
        } else {
            SharedString::default()
        };
        let cfg = self.config.read();
        let fields = build_settings_fields(
            &section,
            &cfg,
            &self.locale,
            &self.theme,
            &self.command_registry,
        );
        drop(cfg);
        sync_vec_model(&self.settings_sections, build_settings_sections(&self.locale));
        sync_vec_model(&self.settings_fields, fields);
        let g = self.window.global::<SettingsGlobal>();
        g.set_visible(st.visible);
        g.set_panel_title(title);
        g.set_hint_text(hint);
        g.set_coming_soon_text(coming_soon);
        g.set_config_path(self.config_file_path.display().to_string().into());
        g.set_current_section_id(section.clone().into());
        g.set_selected_section_index(settings_section_index(&section));
        g.set_sections(self.settings_sections.clone());
        g.set_fields(self.settings_fields.clone());
    }

    pub(super) fn on_settings_field_changed(self: &Arc<Self>, section: &str, key: &str, value: &str) {
        if !self.settings.read().visible {
            return;
        }
        let mut cfg = self.config.write();
        if let Err(reason) = apply_settings_field(&mut cfg, section, key, value, &self.locale) {
            warn!(
                section = %section,
                key = %key,
                value = %value,
                reason = %reason,
                "settings field rejected"
            );
            let body = self.locale.tr_args(
                "settings-field-rejected",
                &orchid_i18n::FluentArgs::new().with("reason", reason),
            );
            self.push_notification(&self.locale.tr("settings-panel-title"), &body, 2);
            return;
        }
        if let Err(e) = cfg.validate() {
            warn!(
                section = %section,
                key = %key,
                value = %value,
                error = %e,
                "settings field failed validation"
            );
            let body = self.locale.tr_args(
                "settings-validation-failed",
                &orchid_i18n::FluentArgs::new().with("reason", e.to_string()),
            );
            self.push_notification(&self.locale.tr("settings-panel-title"), &body, 2);
            return;
        }
        let snapshot = cfg.clone();
        drop(cfg);
        if let Err(e) = ConfigLoader::save(&snapshot, &self.config_file_path) {
            warn!(?e, "settings save failed");
            let reason = storage_localized_error(&self.locale, &e);
            let body = self.locale.tr_args(
                "settings-save-failed",
                &orchid_i18n::FluentArgs::new().with("reason", reason),
            );
            self.push_notification(&self.locale.tr("settings-panel-title"), &body, 3);
            return;
        }
        if let Err(e) = self.apply_hot_config() {
            warn!(?e, "settings apply after save");
            let reason = ui_localized_error(&self.locale, &e);
            let body = self.locale.tr_args(
                "settings-config-reload-failed",
                &orchid_i18n::FluentArgs::new().with("reason", reason),
            );
            self.push_notification(&self.locale.tr("settings-panel-title"), &body, 2);
        }
    }

    pub(super) fn open_settings(self: &Arc<Self>, section: &str) {
        self.on_command_palette_dismiss();
        let section = if SETTINGS_SECTION_IDS.iter().any(|&id| id == section) {
            section.to_string()
        } else {
            SETTINGS_SECTION_IDS[0].to_string()
        };
        {
            let mut st = self.settings.write();
            st.visible = true;
            st.section = section;
        }
        self.sync_settings_global();
    }

    pub(super) fn on_settings_dismiss(self: &Arc<Self>) {
        if !self.settings.read().visible {
            return;
        }
        self.settings.write().visible = false;
        self.sync_settings_global();
    }

    pub(super) fn on_settings_section_selected(self: &Arc<Self>, idx: i32) {
        if !self.settings.read().visible {
            return;
        }
        self.settings.write().section = settings_section_id(idx).to_string();
        self.sync_settings_global();
    }

    pub(super) fn open_config_file(self: &Arc<Self>) {
        let path = self.config_file_path.clone();
        if !path.exists() {
            warn!(?path, "config file missing");
            return;
        }
        if let Err(e) = opener::open(&path) {
            warn!(?e, path = %path.display(), "open config file");
        }
    }

    pub(super) fn sync_navigation_global(self: &Arc<Self>) {
        let nav = self.navigation.read().clone();
        let hint_mode = self.config.read().onboarding.hint_mode_enabled;
        let g = self.window.global::<NavigationGlobal>();
        g.set_workspace_panel_visible(nav.workspace_panel_visible);
        g.set_notification_center_visible(nav.notification_center_visible);
        g.set_dock_visible(nav.dock_visible);
        g.set_hint_mode_enabled(hint_mode);
        g.set_workspace_panel_title(self.locale.tr("navigation-workspace-panel-title").into());
        g.set_notification_center_title(self.locale.tr("notification-center-title").into());
        g.set_notification_center_placeholder(
            self.locale.tr("notification-center-placeholder").into(),
        );
        g.set_panel_dismiss_label(self.locale.tr("notification-center-dismiss").into());
        g.set_hint_dock_label(self.locale.tr("onboarding-hint-dock").into());
        g.set_hint_workspace_label(self.locale.tr("onboarding-hint-workspace").into());
        g.set_hint_gestures_label(self.locale.tr("onboarding-hint-gestures").into());
    }

    pub(super) fn sync_notification_global(self: &Arc<Self>) {
        let g = self.window.global::<NotificationGlobal>();
        g.set_notifications(self.notifications.clone());
        g.set_clear_all_label(self.locale.tr("notification-center-clear").into());
        g.set_dismiss_label(self.locale.tr("notification-center-dismiss").into());
        g.set_empty_placeholder(self.locale.tr("notification-center-placeholder").into());
    }

    pub(super) fn push_notification(self: &Arc<Self>, title: &str, body: &str, severity: i32) {
        let item = NotificationItem {
            id: uuid::Uuid::new_v4().to_string().into(),
            title: title.into(),
            body: body.into(),
            time_label: self
                .config
                .read()
                .locale
                .format_time(chrono::Utc::now())
                .into(),
            severity,
        };
        if let Some(model) = self.notifications.as_any().downcast_ref::<VecModel<NotificationItem>>()
        {
            model.insert(0, item);
            while model.row_count() > NOTIFICATION_LIST_CAP {
                model.remove(model.row_count() - 1);
            }
        }
        self.sync_notification_global();
        self.persist_notifications();
    }

    pub(super) fn clear_notifications(self: &Arc<Self>) {
        if let Some(model) = self.notifications.as_any().downcast_ref::<VecModel<NotificationItem>>()
        {
            model.set_vec(Vec::new());
        }
        self.sync_notification_global();
        self.persist_notifications();
    }

    pub(super) fn dismiss_notification(self: &Arc<Self>, id: &str) {
        if let Some(model) = self.notifications.as_any().downcast_ref::<VecModel<NotificationItem>>()
        {
            if let Some(idx) = (0..model.row_count()).find(|&i| {
                model.row_data(i).is_some_and(|item| item.id.as_str() == id)
            }) {
                model.remove(idx);
            }
        }
        self.sync_notification_global();
        self.persist_notifications();
    }

    fn snapshot_notifications(&self) -> orchid_storage::NotificationCenterState {
        let mut items = Vec::new();
        if let Some(model) = self.notifications.as_any().downcast_ref::<VecModel<NotificationItem>>()
        {
            for i in 0..model.row_count() {
                if let Some(row) = model.row_data(i) {
                    items.push(orchid_storage::NotificationCenterItem {
                        id: row.id.to_string(),
                        title: row.title.to_string(),
                        body: row.body.to_string(),
                        time_label: row.time_label.to_string(),
                        severity: row.severity,
                    });
                }
            }
        }
        orchid_storage::NotificationCenterState { items }
    }

    pub(super) fn persist_notifications(self: &Arc<Self>) {
        let state = self.snapshot_notifications();
        if let Err(e) = (|| -> Result<()> {
            let mut w = self.storage.write().map_err(UiError::Storage)?;
            w.put_notification_center(&state)
                .map_err(UiError::Storage)?;
            w.commit().map_err(UiError::Storage)?;
            Ok(())
        })() {
            warn!(?e, "persist notification center");
        }
    }

    pub(super) fn ensure_startup_notification_tip(self: &Arc<Self>) {
        if self
            .notification_tip_pushed
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        if self.notifications.row_count() > 0 {
            return;
        }
        self.push_notification(
            &self.locale.tr("notification-center-tip-title"),
            &self.locale.tr("notification-center-tip-body"),
            1,
        );
    }

    pub(super) fn sync_onboarding_global(self: &Arc<Self>) {
        let ob = self.onboarding.read().clone();
        let step = ob.current_step.clamp(0, ONBOARDING_STEP_COUNT - 1) as usize;
        let (title_key, body_key) = ONBOARDING_STEP_KEYS[step];
        let g = self.window.global::<OnboardingGlobal>();
        g.set_overlay_visible(ob.overlay_visible);
        g.set_current_step(step as i32);
        g.set_step_count(ONBOARDING_STEP_COUNT);
        let progress = self.locale.tr_args(
            "onboarding-step-progress",
            &orchid_i18n::FluentArgs::new()
                .with("current", (step + 1).to_string())
                .with("total", ONBOARDING_STEP_COUNT.to_string()),
        );
        g.set_step_progress_label(progress.into());
        g.set_step_title(self.locale.tr(title_key).into());
        g.set_step_body(self.locale.tr(body_key).into());
        g.set_back_label(self.locale.tr("onboarding-back").into());
        g.set_next_label(self.locale.tr("onboarding-next").into());
        g.set_skip_label(self.locale.tr("onboarding-skip").into());
        g.set_finish_label(self.locale.tr("onboarding-finish").into());
    }

    pub(super) fn save_config_to_disk(self: &Arc<Self>) {
        let mut cfg = self.config.read().clone();
        if let Err(e) = cfg.validate() {
            warn!(?e, "config validation failed on save");
            return;
        }
        match orchid_crypto::protect_network_mount_passwords(&mut cfg.file_manager.network_mounts)
        {
            Ok(true) => {
                self.config.write().file_manager.network_mounts =
                    cfg.file_manager.network_mounts.clone();
            }
            Ok(false) => {}
            Err(e) => warn!(?e, "could not DPAPI-protect mount passwords before save"),
        }
        if let Err(e) = ConfigLoader::save(&cfg, &self.config_file_path) {
            warn!(?e, "failed to save config.toml");
        }
    }

    pub(super) fn complete_onboarding(self: &Arc<Self>) {
        {
            let mut cfg = self.config.write();
            cfg.onboarding.completed = true;
        }
        self.onboarding.write().overlay_visible = false;
        self.save_config_to_disk();
        self.sync_onboarding_global();
    }

    pub(super) fn ensure_workspace_mode_for_onboarding(self: &Arc<Self>) {
        if self.window.global::<AppState>().get_mode() == 0 {
            self.on_get_started();
        }
    }

    pub(super) fn on_onboarding_next(self: &Arc<Self>) {
        if !self.onboarding.read().overlay_visible {
            return;
        }
        let step = self.onboarding.read().current_step;
        if step + 1 >= ONBOARDING_STEP_COUNT {
            self.ensure_workspace_mode_for_onboarding();
            self.complete_onboarding();
            return;
        }
        if step == 0 {
            self.ensure_workspace_mode_for_onboarding();
        }
        {
            let mut ob = self.onboarding.write();
            ob.current_step = step + 1;
        }
        self.sync_onboarding_global();
    }

    pub(super) fn on_onboarding_back(self: &Arc<Self>) {
        let mut ob = self.onboarding.write();
        if !ob.overlay_visible || ob.current_step <= 0 {
            return;
        }
        ob.current_step -= 1;
        drop(ob);
        self.sync_onboarding_global();
    }

    pub(super) fn on_onboarding_skip(self: &Arc<Self>) {
        if !self.onboarding.read().overlay_visible {
            return;
        }
        self.ensure_workspace_mode_for_onboarding();
        self.complete_onboarding();
    }

    pub(super) fn toggle_hint_mode(self: &Arc<Self>) {
        {
            let mut cfg = self.config.write();
            cfg.onboarding.hint_mode_enabled = !cfg.onboarding.hint_mode_enabled;
        }
        self.save_config_to_disk();
        self.sync_navigation_global();
    }

    pub(super) fn toggle_workspace_panel(self: &Arc<Self>) {
        self.on_command_palette_dismiss();
        {
            let mut nav = self.navigation.write();
            nav.workspace_panel_visible = !nav.workspace_panel_visible;
            if nav.workspace_panel_visible {
                nav.notification_center_visible = false;
            }
        }
        self.sync_navigation_global();
    }

    pub(super) fn toggle_notification_center(self: &Arc<Self>) {
        self.on_command_palette_dismiss();
        let opening = {
            let mut nav = self.navigation.write();
            nav.notification_center_visible = !nav.notification_center_visible;
            if nav.notification_center_visible {
                nav.workspace_panel_visible = false;
            }
            nav.notification_center_visible
        };
        if opening {
            self.ensure_startup_notification_tip();
        }
        self.sync_navigation_global();
    }

    pub(super) fn toggle_dock(self: &Arc<Self>) {
        {
            let mut nav = self.navigation.write();
            nav.dock_visible = !nav.dock_visible;
        }
        self.sync_navigation_global();
        self.update_gesture_bounds();
        let _ = self.sync_canvas_size_from_winit();
        self.schedule_rebuild();
    }

    pub(super) fn on_navigation_workspace_panel_dismiss(self: &Arc<Self>) {
        if !self.navigation.read().workspace_panel_visible {
            return;
        }
        self.navigation.write().workspace_panel_visible = false;
        self.sync_navigation_global();
    }

    pub(super) fn on_notification_center_dismiss(self: &Arc<Self>) {
        if !self.navigation.read().notification_center_visible {
            return;
        }
        self.navigation.write().notification_center_visible = false;
        self.sync_navigation_global();
    }

    pub(super) fn show_universal_search(self: &Arc<Self>) {
        self.on_command_palette_dismiss();
        if let Ok(w) = self.workspace_manager.active() {
            if let Some(inst) = self
                .widget_manager
                .instances_for_workspace(w.id)
                .into_iter()
                .find(|inst| inst.type_id == "universal-search" || inst.type_id == "search")
            {
                *self.search_autofocus_pending.lock() = Some(inst.id);
                self.schedule_rebuild();
                return;
            }
        }
        // UI allowlist + dock use the short id; registry maps it to `universal-search`.
        self.spawn_add_widget("search", AddWidgetPlacement::AutoSlot);
    }

    pub(super) fn show_widget_catalog_center(self: &Arc<Self>) {
        let (vw, vh) = *self.canvas_size.lock();
        let (scroll_x, scroll_y) = *self.canvas_scroll.lock();
        {
            let mut cat = self.catalog.write();
            cat.visible = true;
            cat.content_x = vw / 2.0 + scroll_x;
            cat.content_y = vh / 2.0 + scroll_y;
            cat.screen_x = vw / 2.0;
            cat.screen_y = WORKSPACE_SWITCHER_H + vh / 2.0;
            cat.search_query.clear();
        }
        self.sync_widget_catalog_global();
    }

    pub(super) fn sync_command_palette_global(self: &Arc<Self>) {
        let st = self.palette.read().clone();
        let candidates = build_palette_candidates(
            &self.command_palette,
            &self.command_registry,
            &self.locale,
            &st.query,
            COMMAND_PALETTE_LIMIT,
        );
        sync_vec_model(&self.palette_candidates, candidates);
        let count = self.palette_candidates.row_count();
        let selected = if count == 0 {
            -1
        } else {
            st.selected_index.clamp(0, count as i32 - 1)
        };
        let no_results_text = if !st.query.trim().is_empty() {
            self.locale.tr_args(
                "search-no-results",
                &orchid_i18n::FluentArgs::new().with("query", st.query.clone()),
            )
        } else {
            self.locale.tr("search-no-results-short")
        };
        let g = self.window.global::<CommandPaletteGlobal>();
        g.set_visible(st.visible);
        g.set_model(SearchModel {
            query: st.query.clone().into(),
            candidates: self.palette_candidates.clone(),
            is_searching: false,
            error: SharedString::new(),
            selected_index: selected,
            placeholder_text: self.locale.tr("command-palette-placeholder").into(),
            empty_state_text: self.locale.tr("command-palette-empty").into(),
            no_results_text: no_results_text.into(),
            searching_text: self.locale.tr("search-searching").into(),
            request_autofocus: st.request_autofocus,
        });
        if st.request_autofocus {
            self.palette.write().request_autofocus = false;
        }
    }

    pub(super) fn toggle_command_palette(self: &Arc<Self>) {
        if self.palette.read().visible {
            self.on_command_palette_dismiss();
        } else {
            self.open_command_palette();
        }
    }

    pub(super) fn open_command_palette(self: &Arc<Self>) {
        {
            let mut st = self.palette.write();
            st.visible = true;
            st.query.clear();
            st.selected_index = 0;
            st.request_autofocus = true;
        }
        self.sync_command_palette_global();
    }

    pub(super) fn on_command_palette_dismiss(self: &Arc<Self>) {
        if !self.palette.read().visible {
            return;
        }
        self.palette.write().visible = false;
        self.sync_command_palette_global();
    }

    pub(super) fn on_command_palette_query_changed(self: &Arc<Self>, query: &SharedString) {
        {
            let mut st = self.palette.write();
            st.query = query.to_string();
            st.selected_index = 0;
        }
        self.sync_command_palette_global();
    }

    pub(super) fn on_command_palette_selection_changed(self: &Arc<Self>, new_idx: i32) {
        let count = self.palette_candidates.row_count();
        let clamped = if count == 0 {
            -1
        } else {
            new_idx.clamp(0, count as i32 - 1)
        };
        self.palette.write().selected_index = clamped;
        self.sync_command_palette_global();
    }

    pub(super) fn on_command_palette_candidate_activated(self: &Arc<Self>, cmd_id: &SharedString) {
        let id = cmd_id.to_string();
        if id.is_empty() {
            return;
        }
        self.on_command_palette_dismiss();
        let this = Arc::clone(self);
        spawn::spawn_local_compat(async move {
            this.dispatch_command(&id).await;
            this.schedule_rebuild();
        });
    }
}

fn apply_settings_field(
    cfg: &mut OrchidConfig,
    section: &str,
    key: &str,
    value: &str,
    locale: &LocaleManager,
) -> Result<(), String> {
    match (section, key) {
        ("general", "open-on-startup") => {
            cfg.general.open_on_startup = parse_settings_bool(value)?;
        }
        ("appearance", "theme") => {
            if value.is_empty() {
                return Err("theme id must not be empty".into());
            }
            cfg.appearance.theme = value.to_string();
        }
        ("appearance", "density") => {
            cfg.appearance.density = match value {
                "touch" => orchid_storage::Density::Touch,
                "mouse" => orchid_storage::Density::Mouse,
                "hybrid" => orchid_storage::Density::Hybrid,
                other => return Err(format!("unknown density `{other}`")),
            };
        }
        ("appearance", "font-family") => {
            let trimmed = value.trim();
            let system_default = locale.tr("settings-value-system-default");
            cfg.appearance.font_family = if trimmed.is_empty() || trimmed == system_default {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        ("appearance", "font-scale") => {
            cfg.appearance.font_scale = value
                .parse::<f32>()
                .map_err(|_| format!("invalid font scale `{value}`"))?;
        }
        ("appearance", "reduce-motion") => {
            cfg.appearance.reduce_motion = parse_settings_bool(value)?;
        }
        ("appearance", "follow-system-theme") => {
            cfg.appearance.follow_system_theme = parse_settings_bool(value)?;
        }
        ("appearance", "dark-theme") => {
            if value.is_empty() {
                return Err("dark theme id must not be empty".into());
            }
            cfg.appearance.dark_theme = value.to_string();
        }
        ("appearance", "light-theme") => {
            if value.is_empty() {
                return Err("light theme id must not be empty".into());
            }
            cfg.appearance.light_theme = value.to_string();
        }
        ("input", "primary-hand") => {
            cfg.input.primary_hand = match value {
                "left" => orchid_storage::Hand::Left,
                "right" => orchid_storage::Hand::Right,
                other => return Err(format!("unknown hand `{other}`")),
            };
        }
        ("input", "mirror-edge-swipes") => {
            cfg.input.mirror_edge_swipes = parse_settings_bool(value)?;
        }
        ("shortcuts", "leader-key") => {
            cfg.shortcuts.leader_key = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        ("shortcuts", "leader-timeout") => {
            cfg.shortcuts.leader_timeout_ms = value
                .parse::<u64>()
                .map_err(|_| format!("invalid leader timeout `{value}`"))?;
        }
        ("locale", "language") => {
            if value.is_empty() {
                return Err("language tag must not be empty".into());
            }
            cfg.locale.language = value.to_string();
        }
        ("locale", "date-format") => {
            let trimmed = value.trim();
            let default_label = locale.tr("settings-value-default");
            cfg.locale.date_format = if trimmed.is_empty() || trimmed == default_label {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        ("locale", "time-format") => {
            let trimmed = value.trim();
            let default_label = locale.tr("settings-value-default");
            cfg.locale.time_format = if trimmed.is_empty() || trimmed == default_label {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        ("locale", "first-day-of-week") => {
            cfg.locale.first_day_of_week = match value {
                "0" => 0,
                "1" => 1,
                other => return Err(format!("first day of week must be 0 or 1, got `{other}`")),
            };
        }
        ("privacy", "record-action-history") => {
            cfg.privacy.record_action_history = parse_settings_bool(value)?;
        }
        ("privacy", "history-retention-days") => {
            cfg.privacy.history_retention_days = value
                .parse::<u32>()
                .map_err(|_| format!("invalid history retention `{value}`"))?;
        }
        ("privacy", "clear-clipboard-seconds") => {
            cfg.privacy.clear_clipboard_seconds = value
                .parse::<u32>()
                .map_err(|_| format!("invalid clipboard timeout `{value}`"))?;
        }
        ("privacy", "vault-auto-lock-seconds") => {
            cfg.privacy.vault_auto_lock_seconds = value
                .parse::<u32>()
                .map_err(|_| format!("invalid vault auto-lock `{value}`"))?;
        }
        _ => return Err(format!("field `{section}.{key}` is not editable")),
    }
    Ok(())
}

fn parse_settings_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("expected true/false, got `{other}`")),
    }
}

pub(super) fn density_i18n_key(density: orchid_storage::Density) -> &'static str {
    match density {
        orchid_storage::Density::Touch => "density-touch",
        orchid_storage::Density::Mouse => "density-mouse",
        orchid_storage::Density::Hybrid => "density-hybrid",
    }
}
