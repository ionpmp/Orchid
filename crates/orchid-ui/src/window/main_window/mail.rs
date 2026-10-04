//! Mail widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::ComponentHandle;
use slint::SharedString;
use uuid::Uuid;

use super::MainWindowController;
use crate::html_webview::HtmlDocument;

impl MainWindowController {
    fn parse_mail_id(id: &SharedString) -> Option<Uuid> {
        Uuid::parse_str(id.as_str()).ok()
    }

    pub(super) fn on_mail_select_account(
        self: &Arc<Self>,
        id: &SharedString,
        account: &SharedString,
    ) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::select_account(iid, account.as_str());
        }
    }

    pub(super) fn on_mail_select_folder(
        self: &Arc<Self>,
        id: &SharedString,
        folder: &SharedString,
    ) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::select_folder(iid, folder.as_str());
        }
    }

    pub(super) fn on_mail_select_message(self: &Arc<Self>, id: &SharedString, uid: i32) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::select_message(iid, uid);
            self.sync_mail_html(iid);
        }
    }

    pub(super) fn on_mail_refresh(self: &Arc<Self>, id: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::refresh(iid);
        }
    }

    pub(super) fn on_mail_search(self: &Arc<Self>, id: &SharedString, query: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::set_search(iid, query.as_str());
        }
    }

    pub(super) fn on_mail_open_wizard(self: &Arc<Self>, id: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::open_wizard(iid);
        }
    }

    pub(super) fn on_mail_show_mailbox(self: &Arc<Self>, id: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::show_mailbox(iid);
            self.html_webview.set_document(iid, HtmlDocument::None);
        }
    }

    pub(super) fn on_mail_wizard_set(
        self: &Arc<Self>,
        id: &SharedString,
        field: &SharedString,
        value: &SharedString,
    ) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::wizard_set(iid, field.as_str(), value.as_str());
        }
    }

    pub(super) fn on_mail_wizard_discover(self: &Arc<Self>, id: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::wizard_discover(iid);
        }
    }

    pub(super) fn on_mail_wizard_save(self: &Arc<Self>, id: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::wizard_save_password(iid);
        }
    }

    pub(super) fn on_mail_wizard_oauth(self: &Arc<Self>, id: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::wizard_oauth_start(iid);
        }
    }

    pub(super) fn on_mail_open_compose(self: &Arc<Self>, id: &SharedString, kind: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::open_compose(iid, kind.as_str());
            self.html_webview.set_document(iid, HtmlDocument::None);
        }
    }

    pub(super) fn on_mail_compose_set(
        self: &Arc<Self>,
        id: &SharedString,
        field: &SharedString,
        value: &SharedString,
    ) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::compose_set(iid, field.as_str(), value.as_str());
        }
    }

    pub(super) fn on_mail_compose_send(self: &Arc<Self>, id: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::compose_send(iid);
        }
    }

    pub(super) fn on_mail_compose_draft(self: &Arc<Self>, id: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::compose_save_draft(iid);
        }
    }

    pub(super) fn on_mail_toggle_seen(self: &Arc<Self>, id: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::toggle_seen(iid);
        }
    }

    pub(super) fn on_mail_toggle_flagged(self: &Arc<Self>, id: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::toggle_flagged(iid);
        }
    }

    pub(super) fn on_mail_delete(self: &Arc<Self>, id: &SharedString) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::delete_selected(iid);
            self.html_webview.set_document(iid, HtmlDocument::None);
        }
    }

    pub(super) fn on_mail_set_remote_images(self: &Arc<Self>, id: &SharedString, allow: bool) {
        if let Some(iid) = Self::parse_mail_id(id) {
            orchid_widgets::builtin::mail::set_allow_remote_images(iid, allow);
            self.sync_mail_html(iid);
        }
    }

    pub(super) fn on_mail_embed_bounds(
        self: &Arc<Self>,
        id: &SharedString,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        visible: bool,
    ) {
        let Some(iid) = Self::parse_mail_id(id) else {
            return;
        };
        let scale = f64::from(self.window.window().scale_factor());
        let px = |v: f32| (f64::from(v) * scale).round() as i32;
        self.html_webview.set_bounds(
            iid,
            px(x),
            px(y),
            px(w),
            px(h),
            visible,
            super::html_embed::parent_hwnd_bits(self.window.window()),
        );
        if visible {
            self.sync_mail_html(iid);
        }
    }

    pub(super) fn sync_mail_html(&self, id: Uuid) {
        let document = match orchid_widgets::builtin::mail::reading_html(id) {
            Some(html) => HtmlDocument::Html(html),
            None => HtmlDocument::None,
        };
        self.html_webview.set_document(id, document);
    }
}
