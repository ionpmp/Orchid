use orchid_i18n::LocaleManager;
use slint::{ModelRc, SharedString, VecModel};

use super::super::errors::password_localized_error;
use crate::slint_generated::{
    PasswordAddDialogState, PasswordDetail, PasswordEntryItem, PasswordModel, PasswordTagChip,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct PasswordAddDialogOverlay {
    pub visible: bool,
    pub error: Option<String>,
    pub request_autofocus: bool,
    pub generated_password: Option<String>,
    pub generation_seq: u32,
}


fn empty_password_detail() -> PasswordDetail {
    PasswordDetail {
        has_selection: false,
        id: SharedString::new(),
        title: SharedString::new(),
        username: SharedString::new(),
        url: SharedString::new(),
        notes: SharedString::new(),
        totp_code: SharedString::new(),
        totp_remaining: 0,
        totp_remaining_label: SharedString::new(),
        tags: ModelRc::new(VecModel::default()),
    }
}

fn empty_password_add_dialog(locale: &LocaleManager) -> PasswordAddDialogState {
    PasswordAddDialogState {
        visible: false,
        title: locale.tr("password-add-title").into(),
        title_label: locale.tr("password-label-title").into(),
        username_label: locale.tr("password-label-username").into(),
        password_label: locale.tr("password-label-password").into(),
        url_label: locale.tr("password-label-url").into(),
        submit_label: locale.tr("password-add-submit").into(),
        cancel_label: locale.tr("password-add-cancel").into(),
        generate_label: locale.tr("password-generate").into(),
        gen_password: SharedString::new(),
        gen_seq: 0,
        error: SharedString::new(),
        request_autofocus: false,
    }
}

pub(crate) fn empty_password_model(locale: &LocaleManager) -> PasswordModel {
    PasswordModel {
        is_unlocked: false,
        lock_reason: SharedString::new(),
        biometric_available: false,
        unlock_error: SharedString::new(),
        entries: ModelRc::new(VecModel::default()),
        selected: empty_password_detail(),
        search_query: SharedString::new(),
        toast_message: SharedString::new(),
        toast_visible: false,
        request_autofocus: false,
        add_dialog: empty_password_add_dialog(locale),
    }
}

pub(crate) fn build_password_model(
    p: &orchid_widgets::PasswordManagerPayload,
    toast: Option<(String, bool)>,
    autofocus: bool,
    add_dialog: PasswordAddDialogOverlay,
    locale: &LocaleManager,
) -> PasswordModel {
    let entries: Vec<PasswordEntryItem> = p
        .entries
        .iter()
        .map(|e| {
            let tags: Vec<SharedString> = e.tags.iter().map(|t| t.clone().into()).collect();
            PasswordEntryItem {
                id: e.id.clone().into(),
                title: e.title.clone().into(),
                username: e.username.clone().into(),
                url_host: e.url_host.clone().unwrap_or_default().into(),
                has_totp: e.has_totp,
                tags: ModelRc::new(VecModel::from(tags)),
                color_label: e.color_label.clone().unwrap_or_default().into(),
                modified: e.modified_text.clone().into(),
            }
        })
        .collect();

    let selected = match &p.selected {
        Some(d) => {
            let tags: Vec<PasswordTagChip> = d
                .tags
                .iter()
                .map(|t| PasswordTagChip {
                    label: t.clone().into(),
                })
                .collect();
            let totp_remaining = d.totp_remaining_seconds as i32;
            let totp_remaining_label = if d.totp_code.as_deref().unwrap_or("").is_empty() {
                SharedString::new()
            } else {
                locale
                    .tr_args(
                        "password-totp-remaining",
                        &orchid_i18n::FluentArgs::new().with("s", totp_remaining.to_string()),
                    )
                    .into()
            };
            PasswordDetail {
                has_selection: true,
                id: d.id.clone().into(),
                title: d.title.clone().into(),
                username: d.username.clone().into(),
                url: d.url.clone().unwrap_or_default().into(),
                notes: d.notes.clone().unwrap_or_default().into(),
                totp_code: d.totp_code.clone().unwrap_or_default().into(),
                totp_remaining,
                totp_remaining_label,
                tags: ModelRc::new(VecModel::from(tags)),
            }
        }
        None => empty_password_detail(),
    };

    let (toast_msg, toast_vis) = toast.unwrap_or((String::new(), false));

    let mut dialog = empty_password_add_dialog(locale);
    dialog.visible = add_dialog.visible;
    dialog.error = add_dialog.error.unwrap_or_default().into();
    dialog.request_autofocus = add_dialog.request_autofocus;
    dialog.gen_password = add_dialog.generated_password.unwrap_or_default().into();
    dialog.gen_seq = add_dialog.generation_seq as i32;

    PasswordModel {
        is_unlocked: p.is_unlocked,
        lock_reason: p
            .lock_reason
            .as_deref()
            .map(|r| password_localized_error(locale, r))
            .unwrap_or_default()
            .into(),
        biometric_available: p.biometric_available,
        unlock_error: p
            .unlock_error
            .as_deref()
            .map(|r| password_localized_error(locale, r))
            .unwrap_or_default()
            .into(),
        entries: ModelRc::new(VecModel::from(entries)),
        selected,
        search_query: p.search_query.clone().into(),
        toast_message: toast_msg.into(),
        toast_visible: toast_vis,
        request_autofocus: autofocus,
        add_dialog: dialog,
    }
}
