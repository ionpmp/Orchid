//! Orchid-facing representation of a KDBX group.

use uuid::Uuid;

/// A folder-like container for [`crate::PasswordEntry`]s.
#[derive(Debug, Clone)]
pub struct PasswordGroup {
    /// Stable identifier.
    pub id: Uuid,
    /// User-visible name.
    pub name: String,
    /// Parent group id, or `None` for the root.
    pub parent_id: Option<Uuid>,
    /// Icon name from the icon pack (optional).
    pub icon: Option<String>,
    /// Free-form notes.
    pub notes: Option<String>,
    /// Direct child group ids.
    pub children: Vec<Uuid>,
    /// Ids of entries living directly in this group.
    pub entries: Vec<Uuid>,
}
