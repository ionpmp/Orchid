//! Slint models for the mail widget.

use orchid_i18n::LocaleManager;
use orchid_widgets::MailPayload;
use slint::{ModelRc, VecModel};

use super::sync_eq_rows;
use crate::slint_generated::{MailAccountEntry, MailFolderEntry, MailMessageEntry, MailModel};

pub(crate) fn empty_mail_model(locale: &LocaleManager) -> MailModel {
    build_mail_model(
        &MailPayload {
            mode: 1,
            accounts: Vec::new(),
            folders: Vec::new(),
            messages: Vec::new(),
            selected_account_id: String::new(),
            selected_folder: String::new(),
            selected_uid: 0,
            reading_from: String::new(),
            reading_to: String::new(),
            reading_subject: String::new(),
            reading_date: String::new(),
            reading_text: String::new(),
            reading_has_html: false,
            allow_remote_images: false,
            status: String::new(),
            syncing: false,
            wizard_email: String::new(),
            wizard_display_name: String::new(),
            wizard_password: String::new(),
            wizard_imap_host: String::new(),
            wizard_imap_port: "993".into(),
            wizard_smtp_host: String::new(),
            wizard_smtp_port: "587".into(),
            wizard_tls_imap: "implicit".into(),
            wizard_tls_smtp: "starttls".into(),
            wizard_source: String::new(),
            wizard_oauth_provider: String::new(),
            wizard_error: String::new(),
            compose_to: String::new(),
            compose_cc: String::new(),
            compose_subject: String::new(),
            compose_body: String::new(),
            search_query: String::new(),
        },
        locale,
    )
}

pub(crate) fn build_mail_model(payload: &MailPayload, locale: &LocaleManager) -> MailModel {
    MailModel {
        mode: payload.mode,
        accounts: ModelRc::new(VecModel::from(
            payload
                .accounts
                .iter()
                .map(|a| MailAccountEntry {
                    id: a.id.clone().into(),
                    label: a.label.clone().into(),
                    email: a.email.clone().into(),
                    selected: a.selected,
                })
                .collect::<Vec<_>>(),
        )),
        folders: ModelRc::new(VecModel::from(
            payload
                .folders
                .iter()
                .map(|f| MailFolderEntry {
                    path: f.path.clone().into(),
                    name: f.name.clone().into(),
                    depth: f.depth,
                    unread: f.unread,
                    selected: f.selected,
                })
                .collect::<Vec<_>>(),
        )),
        messages: ModelRc::new(VecModel::from(
            payload
                .messages
                .iter()
                .map(|m| MailMessageEntry {
                    uid: m.uid,
                    from: m.from.clone().into(),
                    subject: m.subject.clone().into(),
                    date: m.date.clone().into(),
                    snippet: m.snippet.clone().into(),
                    seen: m.seen,
                    flagged: m.flagged,
                    has_attachment: m.has_attachment,
                    selected: m.selected,
                    thread_indent: m.thread_indent,
                })
                .collect::<Vec<_>>(),
        )),
        selected_account_id: payload.selected_account_id.clone().into(),
        selected_folder: payload.selected_folder.clone().into(),
        selected_uid: payload.selected_uid,
        reading_from: payload.reading_from.clone().into(),
        reading_to: payload.reading_to.clone().into(),
        reading_subject: payload.reading_subject.clone().into(),
        reading_date: payload.reading_date.clone().into(),
        reading_text: payload.reading_text.clone().into(),
        reading_has_html: payload.reading_has_html,
        allow_remote_images: payload.allow_remote_images,
        status: status_label(locale, &payload.status).into(),
        syncing: payload.syncing,
        wizard_email: payload.wizard_email.clone().into(),
        wizard_display_name: payload.wizard_display_name.clone().into(),
        wizard_password: payload.wizard_password.clone().into(),
        wizard_imap_host: payload.wizard_imap_host.clone().into(),
        wizard_imap_port: payload.wizard_imap_port.clone().into(),
        wizard_smtp_host: payload.wizard_smtp_host.clone().into(),
        wizard_smtp_port: payload.wizard_smtp_port.clone().into(),
        wizard_tls_imap: payload.wizard_tls_imap.clone().into(),
        wizard_tls_smtp: payload.wizard_tls_smtp.clone().into(),
        wizard_source: payload.wizard_source.clone().into(),
        wizard_oauth_provider: payload.wizard_oauth_provider.clone().into(),
        wizard_error: wizard_error_label(locale, &payload.wizard_error).into(),
        compose_to: payload.compose_to.clone().into(),
        compose_cc: payload.compose_cc.clone().into(),
        compose_subject: payload.compose_subject.clone().into(),
        compose_body: payload.compose_body.clone().into(),
        search_query: payload.search_query.clone().into(),
        empty_label: locale.tr("mail-empty").into(),
        add_account_label: locale.tr("mail-add-account").into(),
        discover_label: locale.tr("mail-discover").into(),
        save_label: locale.tr("mail-save-account").into(),
        oauth_label: locale.tr("mail-oauth").into(),
        cancel_label: locale.tr("mail-cancel").into(),
        refresh_label: locale.tr("mail-refresh").into(),
        search_placeholder: locale.tr("mail-search-placeholder").into(),
        compose_label: locale.tr("mail-compose").into(),
        send_label: locale.tr("mail-send").into(),
        draft_label: locale.tr("mail-save-draft").into(),
        reply_label: locale.tr("mail-reply").into(),
        reply_all_label: locale.tr("mail-reply-all").into(),
        forward_label: locale.tr("mail-forward").into(),
        delete_label: locale.tr("mail-delete").into(),
        flag_label: locale.tr("mail-flag").into(),
        seen_label: locale.tr("mail-toggle-seen").into(),
        remote_images_label: locale.tr("mail-remote-images").into(),
        wizard_title: locale.tr("mail-wizard-title").into(),
        email_label: locale.tr("mail-email").into(),
        password_label: locale.tr("mail-password").into(),
        display_name_label: locale.tr("mail-display-name").into(),
        imap_label: locale.tr("mail-imap").into(),
        smtp_label: locale.tr("mail-smtp").into(),
        to_label: locale.tr("mail-to").into(),
        cc_label: locale.tr("mail-cc").into(),
        subject_label: locale.tr("mail-subject").into(),
        body_label: locale.tr("mail-body").into(),
    }
}

pub(crate) fn patch_mail_model(
    model: &mut MailModel,
    payload: &MailPayload,
    locale: &LocaleManager,
) {
    let fresh = build_mail_model(payload, locale);
    sync_eq_rows(&model.accounts, rows_of(&fresh.accounts));
    sync_eq_rows(&model.folders, rows_of(&fresh.folders));
    sync_eq_rows(&model.messages, rows_of(&fresh.messages));
    model.mode = fresh.mode;
    model.selected_account_id = fresh.selected_account_id;
    model.selected_folder = fresh.selected_folder;
    model.selected_uid = fresh.selected_uid;
    model.reading_from = fresh.reading_from;
    model.reading_to = fresh.reading_to;
    model.reading_subject = fresh.reading_subject;
    model.reading_date = fresh.reading_date;
    model.reading_text = fresh.reading_text;
    model.reading_has_html = fresh.reading_has_html;
    model.allow_remote_images = fresh.allow_remote_images;
    model.status = fresh.status;
    model.syncing = fresh.syncing;
    model.wizard_email = fresh.wizard_email;
    model.wizard_display_name = fresh.wizard_display_name;
    model.wizard_password = fresh.wizard_password;
    model.wizard_imap_host = fresh.wizard_imap_host;
    model.wizard_imap_port = fresh.wizard_imap_port;
    model.wizard_smtp_host = fresh.wizard_smtp_host;
    model.wizard_smtp_port = fresh.wizard_smtp_port;
    model.wizard_tls_imap = fresh.wizard_tls_imap;
    model.wizard_tls_smtp = fresh.wizard_tls_smtp;
    model.wizard_source = fresh.wizard_source;
    model.wizard_oauth_provider = fresh.wizard_oauth_provider;
    model.wizard_error = fresh.wizard_error;
    model.compose_to = fresh.compose_to;
    model.compose_cc = fresh.compose_cc;
    model.compose_subject = fresh.compose_subject;
    model.compose_body = fresh.compose_body;
    model.empty_label = fresh.empty_label;
    model.add_account_label = fresh.add_account_label;
    model.discover_label = fresh.discover_label;
    model.save_label = fresh.save_label;
    model.oauth_label = fresh.oauth_label;
    model.cancel_label = fresh.cancel_label;
    model.refresh_label = fresh.refresh_label;
    model.compose_label = fresh.compose_label;
    model.send_label = fresh.send_label;
    model.draft_label = fresh.draft_label;
    model.reply_label = fresh.reply_label;
    model.reply_all_label = fresh.reply_all_label;
    model.forward_label = fresh.forward_label;
    model.delete_label = fresh.delete_label;
    model.flag_label = fresh.flag_label;
    model.seen_label = fresh.seen_label;
    model.remote_images_label = fresh.remote_images_label;
    model.wizard_title = fresh.wizard_title;
    model.email_label = fresh.email_label;
    model.password_label = fresh.password_label;
    model.display_name_label = fresh.display_name_label;
    model.imap_label = fresh.imap_label;
    model.smtp_label = fresh.smtp_label;
    model.to_label = fresh.to_label;
    model.cc_label = fresh.cc_label;
    model.subject_label = fresh.subject_label;
    model.body_label = fresh.body_label;
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
        "syncing" => locale.tr("mail-syncing"),
        "draft-saved" => locale.tr("mail-draft-saved"),
        other => other.to_string(),
    }
}

fn wizard_error_label(locale: &LocaleManager, err: &str) -> String {
    match err {
        "" => String::new(),
        "oauth-waiting" => locale.tr("mail-oauth-waiting"),
        "oauth-cancelled" => locale.tr("mail-oauth-cancelled"),
        "oauth-provider-missing" => locale.tr("mail-oauth-provider-missing"),
        other => other.to_string(),
    }
}
