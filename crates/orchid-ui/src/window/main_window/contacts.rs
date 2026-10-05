//! Contacts widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;
use uuid::Uuid;

use crate::window::spawn;

use super::MainWindowController;

impl MainWindowController {
    fn parse_contacts_id(id: &SharedString) -> Option<Uuid> {
        Uuid::parse_str(id.as_str()).ok()
    }

    pub(super) fn on_contacts_select(self: &Arc<Self>, id: &SharedString, card: &SharedString) {
        let Some(inst) = Self::parse_contacts_id(id) else {
            return;
        };
        orchid_widgets::builtin::contacts::select_contact(inst, card.as_str());
        self.refresh_contacts(inst);
    }

    pub(super) fn on_contacts_new(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_contacts_id(id) else {
            return;
        };
        orchid_widgets::builtin::contacts::new_contact(inst);
        self.refresh_contacts(inst);
    }

    pub(super) fn on_contacts_set_field(
        self: &Arc<Self>,
        id: &SharedString,
        key: &SharedString,
        value: &SharedString,
    ) {
        let Some(inst) = Self::parse_contacts_id(id) else {
            return;
        };
        orchid_widgets::builtin::contacts::set_field(inst, key.as_str(), value.to_string());
        self.refresh_contacts(inst);
    }

    pub(super) fn on_contacts_save(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_contacts_id(id) else {
            return;
        };
        orchid_widgets::builtin::contacts::save_selected(inst);
        self.refresh_contacts(inst);
    }

    pub(super) fn on_contacts_delete(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_contacts_id(id) else {
            return;
        };
        orchid_widgets::builtin::contacts::delete_selected(inst);
        self.refresh_contacts(inst);
    }

    pub(super) fn on_contacts_save_account(
        self: &Arc<Self>,
        id: &SharedString,
        url: &SharedString,
        user: &SharedString,
        password: &SharedString,
    ) {
        let Some(inst) = Self::parse_contacts_id(id) else {
            return;
        };
        orchid_widgets::builtin::contacts::set_field(inst, "account_url", url.to_string());
        orchid_widgets::builtin::contacts::set_field(inst, "account_user", user.to_string());
        orchid_widgets::builtin::contacts::set_field(
            inst,
            "account_password",
            password.to_string(),
        );
        orchid_widgets::builtin::contacts::save_account(inst);
        self.refresh_contacts(inst);
    }

    pub(super) fn on_contacts_sync(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_contacts_id(id) else {
            return;
        };
        orchid_widgets::builtin::contacts::sync(inst);
        self.refresh_contacts(inst);
    }

    fn refresh_contacts(self: &Arc<Self>, inst_id: Uuid) {
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_instance_patch(inst_id);
            }
        });
    }
}
