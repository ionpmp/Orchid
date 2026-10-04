//! Mail account metadata (non-secret).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How the account authenticates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AuthKind {
    /// Username + password (or app password).
    #[default]
    Password,
    /// OAuth2 access / refresh tokens (Gmail, Microsoft 365).
    Oauth2,
}

/// TLS mode for IMAP / SMTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TlsMode {
    /// Implicit TLS (IMAPS 993 / SMTPS 465).
    #[default]
    Implicit,
    /// STARTTLS on a cleartext port.
    StartTls,
    /// No TLS (tests / LAN only).
    None,
}

/// One side of the mail stack (IMAP or SMTP).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEndpoint {
    /// Hostname.
    pub host: String,
    /// Port.
    pub port: u16,
    /// TLS mode.
    pub tls: TlsMode,
    /// Login username (often the email address).
    pub username: String,
}

impl Default for ServerEndpoint {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 0,
            tls: TlsMode::Implicit,
            username: String::new(),
        }
    }
}

/// Persisted account record (secrets live in [`crate::secrets::MailSecretsStore`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailAccount {
    /// Stable id.
    pub id: Uuid,
    /// Display name shown in the widget.
    pub display_name: String,
    /// Primary email address.
    pub email: String,
    /// Optional identity name for From.
    pub identity_name: String,
    /// Authentication kind.
    pub auth: AuthKind,
    /// OAuth provider id when [`AuthKind::Oauth2`] (`google`, `microsoft`).
    #[serde(default)]
    pub oauth_provider: Option<String>,
    /// IMAP endpoint.
    pub imap: ServerEndpoint,
    /// SMTP endpoint.
    pub smtp: ServerEndpoint,
    /// When the account was added.
    pub created_at: DateTime<Utc>,
    /// Last successful sync, if any.
    pub last_sync_at: Option<DateTime<Utc>>,
}

impl MailAccount {
    /// Build a new account shell with fresh id and timestamps.
    #[must_use]
    pub fn new(email: impl Into<String>, display_name: impl Into<String>) -> Self {
        let email = email.into();
        Self {
            id: Uuid::new_v4(),
            display_name: display_name.into(),
            email: email.clone(),
            identity_name: String::new(),
            auth: AuthKind::Password,
            oauth_provider: None,
            imap: ServerEndpoint {
                username: email.clone(),
                ..ServerEndpoint::default()
            },
            smtp: ServerEndpoint {
                username: email,
                ..ServerEndpoint::default()
            },
            created_at: Utc::now(),
            last_sync_at: None,
        }
    }
}

/// Folder row cached for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailFolder {
    /// Account id.
    pub account_id: Uuid,
    /// IMAP mailbox path (e.g. `INBOX`, `INBOX/Archive`).
    pub path: String,
    /// Human label (last path segment or SPECIAL-USE name).
    pub name: String,
    /// Nesting depth for tree indentation.
    pub depth: u32,
    /// SPECIAL-USE role when known (`inbox`, `sent`, `drafts`, `trash`, `junk`, `archive`).
    pub role: Option<String>,
    /// Unread count from the last LIST/STATUS.
    pub unread: u32,
    /// Total messages.
    pub total: u32,
}

/// Lightweight message header for list panes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageHeader {
    /// Account id.
    pub account_id: Uuid,
    /// Folder path.
    pub folder: String,
    /// IMAP UID.
    pub uid: u32,
    /// Message-ID header when present.
    pub message_id: Option<String>,
    /// From display.
    pub from: String,
    /// To display (comma-joined).
    pub to: String,
    /// Subject.
    pub subject: String,
    /// Date (RFC 2822 parsed → RFC 3339) or empty.
    pub date: String,
    /// Epoch seconds for sorting (0 when unknown).
    pub date_unix: i64,
    /// Seen flag.
    pub seen: bool,
    /// Flagged.
    pub flagged: bool,
    /// Has attachment.
    pub has_attachment: bool,
    /// Snippet of plain body when known.
    pub snippet: String,
}

/// Full message body for the reading pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageBody {
    /// Account id.
    pub account_id: Uuid,
    /// Folder path.
    pub folder: String,
    /// IMAP UID.
    pub uid: u32,
    /// Plain-text body (may be empty when only HTML exists).
    pub text: String,
    /// HTML body (may be empty).
    pub html: String,
    /// Attachment descriptors.
    pub attachments: Vec<AttachmentMeta>,
    /// Part bytes, in the same order as `attachments`. Kept out of the metadata JSON.
    #[serde(skip)]
    pub parts: Vec<Vec<u8>>,
}

/// Attachment name and size. The bytes live beside the cached body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentMeta {
    /// Part index / stable id within the message.
    pub id: String,
    /// Filename.
    pub filename: String,
    /// MIME type.
    pub content_type: String,
    /// Size in bytes when known.
    pub size: u64,
}

/// Draft / outbound compose model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ComposeMessage {
    /// From account id.
    pub account_id: Option<Uuid>,
    /// To recipients (comma or semicolon separated in the UI).
    pub to: String,
    /// Cc.
    pub cc: String,
    /// Bcc.
    pub bcc: String,
    /// Subject.
    pub subject: String,
    /// Plain body.
    pub body: String,
    /// Absolute paths of files to attach.
    pub attachments: Vec<String>,
    /// In-Reply-To Message-ID when replying.
    pub in_reply_to: Option<String>,
    /// References chain.
    pub references: Option<String>,
}

/// Suggested servers from autodiscover (before credentials are verified).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ServerSuggestion {
    /// IMAP.
    pub imap: ServerEndpoint,
    /// SMTP.
    pub smtp: ServerEndpoint,
    /// Preferred auth.
    pub auth: AuthKind,
    /// OAuth provider when applicable.
    pub oauth_provider: Option<String>,
    /// Human source (`profile`, `ispdb`, `srv`, `probe`, `manual`).
    pub source: String,
}
