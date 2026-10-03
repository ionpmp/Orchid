//! Agent panel handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;

use super::MainWindowController;

impl MainWindowController {
    pub(super) fn on_agent_send(self: &Arc<Self>, _id: &SharedString, text: &SharedString) {
        orchid_widgets::agent::submit(self.widget_manager.jobs(), text.to_string());
    }

    pub(super) fn on_agent_confirm(self: &Arc<Self>, _id: &SharedString) {
        if let Err(reason) = orchid_widgets::agent::confirm_pending_write() {
            self.push_notification(&self.locale.tr("agent-reply"), &reason, 2);
        }
    }

    pub(super) fn on_agent_dismiss(self: &Arc<Self>, _id: &SharedString) {
        orchid_widgets::agent::dismiss_pending_write();
    }

    pub(super) fn on_agent_clear(self: &Arc<Self>, _id: &SharedString) {
        orchid_widgets::agent::clear_conversation();
    }
}
