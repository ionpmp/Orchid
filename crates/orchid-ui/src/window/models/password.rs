use orchid_i18n::LocaleManager;
use slint::{Model, ModelRc, SharedString, VecModel};

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
    let mut model = empty_password_model(locale);
    patch_password_model(&mut model, p, toast, autofocus, add_dialog, locale);
    model
}

/// Update an existing [`PasswordModel`] in place, keeping list `ModelRc`s.
pub(crate) fn patch_password_model(
    model: &mut PasswordModel,
    p: &orchid_widgets::PasswordManagerPayload,
    toast: Option<(String, bool)>,
    autofocus: bool,
    add_dialog: PasswordAddDialogOverlay,
    locale: &LocaleManager,
) {
    sync_entries(&model.entries, &p.entries);
    patch_selected(&mut model.selected, p.selected.as_ref(), locale);

    let (toast_msg, toast_vis) = toast.unwrap_or((String::new(), false));
    model.is_unlocked = p.is_unlocked;
    model.lock_reason = p
        .lock_reason
        .as_deref()
        .map(|r| password_localized_error(locale, r))
        .unwrap_or_default()
        .into();
    model.biometric_available = p.biometric_available;
    model.unlock_error = p
        .unlock_error
        .as_deref()
        .map(|r| password_localized_error(locale, r))
        .unwrap_or_default()
        .into();
    model.search_query = p.search_query.clone().into();
    model.toast_message = toast_msg.into();
    model.toast_visible = toast_vis;
    model.request_autofocus = autofocus;

    model.add_dialog.visible = add_dialog.visible;
    model.add_dialog.error = add_dialog.error.unwrap_or_default().into();
    model.add_dialog.request_autofocus = add_dialog.request_autofocus;
    model.add_dialog.gen_password = add_dialog.generated_password.unwrap_or_default().into();
    model.add_dialog.gen_seq = add_dialog.generation_seq as i32;
}

fn sync_entries(model: &ModelRc<PasswordEntryItem>, entries: &[orchid_widgets::PasswordEntryView]) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<PasswordEntryItem>>() else {
        return;
    };
    while v.row_count() > entries.len() {
        v.remove(v.row_count() - 1);
    }
    for (i, e) in entries.iter().enumerate() {
        let tags: Vec<SharedString> = e.tags.iter().map(|t| t.clone().into()).collect();
        let row = PasswordEntryItem {
            id: e.id.clone().into(),
            title: e.title.clone().into(),
            username: e.username.clone().into(),
            url_host: e.url_host.clone().unwrap_or_default().into(),
            has_totp: e.has_totp,
            tags: ModelRc::new(VecModel::from(tags)),
            color_label: e.color_label.clone().unwrap_or_default().into(),
            modified: e.modified_text.clone().into(),
        };
        if i < v.row_count() {
            if let Some(old) = v.row_data(i) {
                if old.id == row.id
                    && old.title == row.title
                    && old.username == row.username
                    && old.url_host == row.url_host
                    && old.has_totp == row.has_totp
                    && old.color_label == row.color_label
                    && old.modified == row.modified
                {
                    // Tags rarely change on the 1Hz TOTP tick; skip full replace.
                    continue;
                }
            }
            v.set_row_data(i, row);
        } else {
            v.push(row);
        }
    }
}

fn patch_selected(
    selected: &mut PasswordDetail,
    detail: Option<&orchid_widgets::PasswordEntryDetailView>,
    locale: &LocaleManager,
) {
    let Some(d) = detail else {
        if selected.has_selection {
            let tags = selected.tags.clone();
            *selected = empty_password_detail();
            selected.tags = tags;
            if let Some(v) = selected
                .tags
                .as_any()
                .downcast_ref::<VecModel<PasswordTagChip>>()
            {
                while v.row_count() > 0 {
                    v.remove(v.row_count() - 1);
                }
            }
        }
        return;
    };
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
    selected.has_selection = true;
    selected.id = d.id.clone().into();
    selected.title = d.title.clone().into();
    selected.username = d.username.clone().into();
    selected.url = d.url.clone().unwrap_or_default().into();
    selected.notes = d.notes.clone().unwrap_or_default().into();
    selected.totp_code = d.totp_code.clone().unwrap_or_default().into();
    selected.totp_remaining = totp_remaining;
    selected.totp_remaining_label = totp_remaining_label;

    let chips: Vec<PasswordTagChip> = d
        .tags
        .iter()
        .map(|t| PasswordTagChip {
            label: t.clone().into(),
        })
        .collect();
    if let Some(v) = selected
        .tags
        .as_any()
        .downcast_ref::<VecModel<PasswordTagChip>>()
    {
        while v.row_count() > chips.len() {
            v.remove(v.row_count() - 1);
        }
        for (i, chip) in chips.into_iter().enumerate() {
            if i < v.row_count() {
                if let Some(old) = v.row_data(i) {
                    if old.label == chip.label {
                        continue;
                    }
                }
                v.set_row_data(i, chip);
            } else {
                v.push(chip);
            }
        }
    } else {
        selected.tags = ModelRc::new(VecModel::from(chips));
    }
}
