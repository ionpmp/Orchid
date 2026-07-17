//! Calculator widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;
use uuid::Uuid;

use crate::window::spawn;

use super::MainWindowController;

impl MainWindowController {
    pub(super) fn find_active_calculator_widget(&self) -> Option<Uuid> {
        let w = self.workspace_manager.active().ok()?;
        for inst in self.widget_manager.instances_for_workspace(w.id) {
            if inst.type_id == "calculator" {
                return Some(inst.id);
            }
        }
        None
    }

    pub(super) fn on_calculator_button(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = self.find_active_calculator_widget() else {
            return;
        };
        orchid_widgets::builtin::calculator::press_id(inst_id, id.as_str());
        self.refresh_calculator(inst_id);
    }

    pub(super) fn on_calculator_key(
        self: &Arc<Self>,
        text: &SharedString,
        ctrl: bool,
        shift: bool,
    ) {
        let Some(inst_id) = self.find_active_calculator_widget() else {
            return;
        };
        if ctrl && matches!(text.as_str(), "c" | "C") {
            if let Some(display) =
                orchid_widgets::builtin::calculator::current_display(inst_id)
            {
                if !display.is_empty() {
                    self.copy_calculator_display(&display);
                }
            }
            return;
        }
        orchid_widgets::builtin::calculator::press_text(inst_id, text.as_str(), ctrl, shift);
        self.refresh_calculator(inst_id);
    }

    pub(super) fn on_calculator_history(self: &Arc<Self>, index: i32) {
        let Some(inst_id) = self.find_active_calculator_widget() else {
            return;
        };
        orchid_widgets::builtin::calculator::recall_history(inst_id, index);
        self.refresh_calculator(inst_id);
    }

    fn copy_calculator_display(self: &Arc<Self>, text: &str) {
        match crate::widgets::terminal::ArboardClipboard::new() {
            Ok(cb) => {
                let _ = cb.copy(text);
                self.push_notification(
                    &self.locale.tr("widget-calculator-name"),
                    &self.locale.tr("calc-copied"),
                    0,
                );
            }
            Err(e) => {
                self.push_notification(
                    &self.locale.tr("widget-calculator-name"),
                    &e.to_string(),
                    1,
                );
            }
        }
    }

    fn refresh_calculator(self: &Arc<Self>, inst_id: Uuid) {
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }
}
