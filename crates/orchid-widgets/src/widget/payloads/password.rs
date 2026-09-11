//! Payload for the password-manager widget.
//!
//! Passwords never appear in snapshots — only metadata. Users reveal the
//! secret by copying it to the clipboard, which goes through the secure
//! clipboard abstraction.

/// Render-ready password-manager payload.
#[derive(Debug, Clone, Default)]
pub struct PasswordManagerPayload {
    /// Whether the database is currently unlocked.
    pub is_unlocked: bool,
    /// Localised reason when the database is locked / unavailable.
    pub lock_reason: Option<String>,
    /// Entries matching the current search query.
    pub entries: Vec<PasswordEntryView>,
    /// Detail for the currently-selected entry, if any.
    pub selected: Option<PasswordEntryDetailView>,
    /// Current search query.
    pub search_query: String,
    /// Vault groups (root first). Empty when locked.
    pub groups: Vec<PasswordGroupView>,
    /// Selected group filter; `None` means every group.
    pub selected_group_id: Option<String>,
    /// Whether Windows Hello unlock is offered in the UI.
    pub biometric_available: bool,
    /// Last unlock attempt error message (if any).
    pub unlock_error: Option<String>,
}

/// One KDBX group in the sidebar / dialog picker.
#[derive(Debug, Clone)]
pub struct PasswordGroupView {
    /// Stable group id.
    pub id: String,
    /// User-visible name.
    pub name: String,
}

/// Summary row in the entry list. No passwords are exposed.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct PasswordEntryView {
    pub id: String,
    pub title: String,
    pub username: String,
    pub url_host: Option<String>,
    pub has_totp: bool,
    pub tags: Vec<String>,
    pub color_label: Option<String>,
    pub modified_text: String,
    pub group_id: String,
    pub group_name: String,
}

/// Detail view for one selected entry. Still no raw password.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct PasswordEntryDetailView {
    pub id: String,
    pub title: String,
    pub username: String,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub totp_code: Option<String>,
    pub totp_remaining_seconds: u32,
    pub tags: Vec<String>,
    pub group_id: String,
}
