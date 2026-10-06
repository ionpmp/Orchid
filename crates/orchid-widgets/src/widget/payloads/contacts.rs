//! Payload for the contacts widget.

#![allow(missing_docs)]

/// One row in the contact list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactRow {
    pub id: String,
    pub label: String,
    pub selected: bool,
}

/// List, editor, and CardDAV account form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactsPayload {
    pub rows: Vec<ContactRow>,
    pub name: String,
    pub email: String,
    pub email2: String,
    pub phone: String,
    pub phone2: String,
    pub notes: String,
    pub has_selection: bool,
    pub account_url: String,
    pub account_user: String,
    pub status: String,
}
