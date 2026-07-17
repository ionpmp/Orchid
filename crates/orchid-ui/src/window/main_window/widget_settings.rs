//! Per-widget settings dialog handlers.

use std::sync::Arc;

use slint::Model;
use slint::ModelRc;
use slint::SharedString;
use slint::VecModel;
use uuid::Uuid;

use crate::slint_generated::{WidgetFrameModel, WidgetSettingsDialog};
use crate::window::models::{
    apply_widget_setting, build_widget_settings_fields, widget_has_settings,
};
use crate::window::spawn;

use super::{empty_widget_settings_dialog, MainWindowController};

impl MainWindowController {
    pub(super) fn on_widget_settings_clicked(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let Ok(iref) = self.widget_manager.get_instance(u) else {
            return;
        };
        if !widget_has_settings(iref.type_id.as_str()) {
            return;
        }
        let fields = build_widget_settings_fields(iref.type_id.as_str(), u, &self.locale);
        let dlg = WidgetSettingsDialog {
            visible: true,
            title: self.locale.tr("widget-settings-title").into(),
            close_label: self.locale.tr("settings-panel-ok").into(),
            fields: ModelRc::new(VecModel::from(fields)),
        };
        self.settings_dialog_overlays.write().insert(u, dlg);
        self.patch_frame_settings_dialog(u);
    }

    pub(super) fn on_widget_settings_dismiss(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        self.settings_dialog_overlays.write().remove(&u);
        self.patch_frame_settings_dialog(u);
    }

    pub(super) fn on_widget_settings_field_changed(
        self: &Arc<Self>,
        id: &SharedString,
        key: &SharedString,
        value: &SharedString,
    ) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let Ok(iref) = self.widget_manager.get_instance(u) else {
            return;
        };
        let type_id = iref.type_id.clone();
        let key = key.to_string();
        let value = value.to_string();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            apply_widget_setting(&type_id, u, &key, &value).await;
            let Some(c) = t.upgrade() else {
                return;
            };
            // Refresh dialog fields so combos / text stay in sync with applied config.
            if c.settings_dialog_overlays
                .read()
                .get(&u)
                .is_some_and(|d| d.visible)
            {
                let fields = build_widget_settings_fields(&type_id, u, &c.locale);
                let dlg = WidgetSettingsDialog {
                    visible: true,
                    title: c.locale.tr("widget-settings-title").into(),
                    close_label: c.locale.tr("settings-panel-ok").into(),
                    fields: ModelRc::new(VecModel::from(fields)),
                };
                c.settings_dialog_overlays.write().insert(u, dlg);
                c.patch_frame_settings_dialog(u);
            }
            if let Err(e) = c.widget_manager.save_widget_state(u).await {
                tracing::warn!(%u, error = %e, "widget settings: persist failed");
            }
            let _ = c.widget_manager.refresh_snapshot_cache(u).await;
            c.schedule_rebuild();
        });
    }

    pub(super) fn patch_frame_settings_dialog(&self, id: Uuid) {
        let dlg = self
            .settings_dialog_overlays
            .read()
            .get(&id)
            .cloned()
            .unwrap_or_else(empty_widget_settings_dialog);
        let needle = id.to_string();
        for model in [&self.workspace_widgets, &self.workspace_floating_widgets] {
            let Some(v) = model
                .as_any()
                .downcast_ref::<VecModel<WidgetFrameModel>>()
            else {
                continue;
            };
            for r in 0..v.row_count() {
                let Some(mut row) = v.row_data(r) else {
                    continue;
                };
                if row.instance_id.as_str() == needle.as_str() {
                    row.settings_dialog = dlg.clone();
                    v.set_row_data(r, row);
                    break;
                }
            }
        }
    }
}
