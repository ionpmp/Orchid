//! Slint models for the contacts widget.

use orchid_i18n::LocaleManager;
use orchid_widgets::ContactsPayload;
use slint::{ModelRc, VecModel};

use super::sync_eq_rows;
use crate::slint_generated::{ContactEntry, ContactsModel};

pub(crate) fn empty_contacts_model(locale: &LocaleManager) -> ContactsModel {
    build_contacts_model(
        &ContactsPayload {
            rows: Vec::new(),
            name: String::new(),
            email: String::new(),
            email2: String::new(),
            phone: String::new(),
            phone2: String::new(),
            notes: String::new(),
            has_selection: false,
            account_url: String::new(),
            account_user: String::new(),
            status: String::new(),
        },
        locale,
    )
}

pub(crate) fn build_contacts_model(
    payload: &ContactsPayload,
    locale: &LocaleManager,
) -> ContactsModel {
    ContactsModel {
        rows: ModelRc::new(VecModel::from(
            payload
                .rows
                .iter()
                .map(|row| ContactEntry {
                    id: row.id.clone().into(),
                    label: row.label.clone().into(),
                    selected: row.selected,
                })
                .collect::<Vec<_>>(),
        )),
        name: payload.name.clone().into(),
        email: payload.email.clone().into(),
        email2: payload.email2.clone().into(),
        phone: payload.phone.clone().into(),
        phone2: payload.phone2.clone().into(),
        notes: payload.notes.clone().into(),
        has_selection: payload.has_selection,
        account_url: payload.account_url.clone().into(),
        account_user: payload.account_user.clone().into(),
        status: status_label(locale, &payload.status).into(),
        new_label: locale.tr("contacts-new").into(),
        save_label: locale.tr("contacts-save").into(),
        delete_label: locale.tr("contacts-delete").into(),
        sync_label: locale.tr("contacts-sync").into(),
        name_label: locale.tr("contacts-name").into(),
        email_label: locale.tr("contacts-email").into(),
        email2_label: locale.tr("contacts-email-2").into(),
        phone_label: locale.tr("contacts-phone").into(),
        phone2_label: locale.tr("contacts-phone-2").into(),
        notes_label: locale.tr("contacts-notes").into(),
        url_label: locale.tr("contacts-url").into(),
        user_label: locale.tr("contacts-user").into(),
        password_label: locale.tr("contacts-password").into(),
        save_account_label: locale.tr("contacts-save-account").into(),
        empty_label: locale.tr("contacts-empty").into(),
        hint: locale.tr("contacts-hint").into(),
    }
}

pub(crate) fn patch_contacts_model(
    model: &mut ContactsModel,
    payload: &ContactsPayload,
    locale: &LocaleManager,
) {
    let fresh = build_contacts_model(payload, locale);
    sync_eq_rows(&model.rows, rows_of(&fresh.rows));
    model.name = fresh.name;
    model.email = fresh.email;
    model.email2 = fresh.email2;
    model.phone = fresh.phone;
    model.phone2 = fresh.phone2;
    model.notes = fresh.notes;
    model.has_selection = fresh.has_selection;
    model.account_url = fresh.account_url;
    model.account_user = fresh.account_user;
    model.status = fresh.status;
    model.new_label = fresh.new_label;
    model.save_label = fresh.save_label;
    model.delete_label = fresh.delete_label;
    model.sync_label = fresh.sync_label;
    model.name_label = fresh.name_label;
    model.email_label = fresh.email_label;
    model.email2_label = fresh.email2_label;
    model.phone_label = fresh.phone_label;
    model.phone2_label = fresh.phone2_label;
    model.notes_label = fresh.notes_label;
    model.url_label = fresh.url_label;
    model.user_label = fresh.user_label;
    model.password_label = fresh.password_label;
    model.save_account_label = fresh.save_account_label;
    model.empty_label = fresh.empty_label;
    model.hint = fresh.hint;
}

fn rows_of<T: Clone + 'static>(model: &ModelRc<T>) -> Vec<T> {
    use slint::Model;
    let Some(rows) = model.as_any().downcast_ref::<VecModel<T>>() else {
        return Vec::new();
    };
    (0..rows.row_count())
        .filter_map(|i| rows.row_data(i))
        .collect()
}

fn status_label(locale: &LocaleManager, status: &str) -> String {
    match status {
        "" => String::new(),
        "account-saved" => locale.tr("contacts-account-saved"),
        "saved-local" => locale.tr("contacts-saved-local"),
        "saved-remote" => locale.tr("contacts-saved-remote"),
        "deleted" => locale.tr("contacts-deleted"),
        "need-account" => locale.tr("contacts-need-account"),
        "syncing" => locale.tr("contacts-syncing"),
        "synced" => locale.tr("contacts-synced"),
        "truncated" => locale.tr("contacts-truncated"),
        "secret" => locale.tr("contacts-secret"),
        "auth" => locale.tr("contacts-auth"),
        "conflict" => locale.tr("contacts-conflict"),
        "one-url" => locale.tr("contacts-one-url"),
        "bad-url" => locale.tr("contacts-bad-url"),
        other => {
            if let Some(reason) = other.strip_prefix("failed:") {
                locale.tr_args(
                    "contacts-failed",
                    &orchid_i18n::FluentArgs::new().with("reason", reason),
                )
            } else {
                other.to_string()
            }
        }
    }
}
