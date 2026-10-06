//! Payload for the mail client widget.

#![allow(missing_docs)]

/// Account chip in the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailAccountRow {
    pub id: String,
    pub label: String,
    pub email: String,
    pub selected: bool,
}

/// Folder tree row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailFolderRow {
    pub path: String,
    pub name: String,
    pub depth: i32,
    pub unread: i32,
    pub selected: bool,
}

/// Message list row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailMessageRow {
    pub uid: i32,
    pub from: String,
    pub subject: String,
    pub date: String,
    pub snippet: String,
    pub seen: bool,
    pub flagged: bool,
    pub has_attachment: bool,
    pub selected: bool,
    /// 0 for the newest message in a subject thread, 1 for an older reply.
    pub thread_indent: i32,
}

/// One file attached to the open message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailAttachmentRow {
    pub id: String,
    /// Filename and size, ready to show.
    pub label: String,
}

/// Wizard / compose / reading state for the mail UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailPayload {
    /// 0 = mailbox, 1 = wizard, 2 = compose.
    pub mode: i32,
    pub accounts: Vec<MailAccountRow>,
    pub folders: Vec<MailFolderRow>,
    pub messages: Vec<MailMessageRow>,
    pub selected_account_id: String,
    pub selected_folder: String,
    pub selected_uid: i32,
    pub reading_from: String,
    pub reading_to: String,
    pub reading_subject: String,
    pub reading_date: String,
    pub reading_text: String,
    pub reading_has_html: bool,
    pub allow_remote_images: bool,
    pub status: String,
    pub syncing: bool,
    // Wizard fields
    pub wizard_email: String,
    pub wizard_display_name: String,
    pub wizard_password: String,
    pub wizard_imap_host: String,
    pub wizard_imap_port: String,
    pub wizard_smtp_host: String,
    pub wizard_smtp_port: String,
    pub wizard_tls_imap: String,
    pub wizard_tls_smtp: String,
    pub wizard_source: String,
    pub wizard_oauth_provider: String,
    pub wizard_error: String,
    // Compose
    pub compose_to: String,
    pub compose_cc: String,
    /// Bcc addresses. They go on the SMTP envelope and into a Bcc header.
    pub compose_bcc: String,
    pub compose_subject: String,
    pub compose_body: String,
    /// Files chosen for the outgoing message. `id` is the path on this computer.
    pub compose_files: Vec<MailAttachmentRow>,
    /// Current mailbox search. Empty shows the cached folder.
    pub search_query: String,
    /// Files on the open message. Empty when no message is open.
    pub attachments: Vec<MailAttachmentRow>,
}
