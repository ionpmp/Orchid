//! Local contact cards and one CardDAV account.

#![allow(missing_docs)]

use bincode_reloaded::{Decode, Encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One person. `id` is the vCard UID.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, PartialEq, Eq)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub email: String,
    pub phone: String,
    pub notes: String,
    /// Absolute URL of the vCard, empty when the card has not been uploaded.
    pub href: String,
    /// Server `ETag`, empty when unknown.
    pub etag: String,
}

impl Contact {
    pub fn blank() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: String::new(),
            email: String::new(),
            phone: String::new(),
            notes: String::new(),
            href: String::new(),
            etag: String::new(),
        }
    }
}

/// Saved contacts for one widget instance.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, PartialEq, Eq)]
pub struct ContactsConfig {
    pub contacts: Vec<Contact>,
    pub carddav_url: String,
    pub carddav_user: String,
    /// DPAPI-protected password, or empty.
    pub carddav_password: String,
}

impl Default for ContactsConfig {
    fn default() -> Self {
        Self {
            contacts: Vec::new(),
            carddav_url: String::new(),
            carddav_user: String::new(),
            carddav_password: String::new(),
        }
    }
}

/// Replace linked cards with the server copy. Cards that were never uploaded stay.
pub fn merge_remote(local: &mut Vec<Contact>, remote: Vec<Contact>) {
    let remote_ids: Vec<String> = remote.iter().map(|card| card.id.clone()).collect();
    local.retain(|card| card.href.is_empty() || remote_ids.iter().any(|id| id == &card.id));
    for card in remote {
        if let Some(existing) = local.iter_mut().find(|item| item.id == card.id) {
            *existing = card;
        } else {
            local.push(card);
        }
    }
}
