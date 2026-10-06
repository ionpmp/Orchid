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
    /// Second email. Empty when the card has only one.
    pub email2: String,
    /// Second phone. Empty when the card has only one.
    pub phone2: String,
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
            email2: String::new(),
            phone2: String::new(),
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

/// Cards saved before the second email and phone existed.
#[derive(Debug, Serialize, Deserialize)]
struct ContactV1 {
    id: String,
    name: String,
    email: String,
    phone: String,
    notes: String,
    href: String,
    etag: String,
}

/// Widget config saved before the second email and phone existed.
#[derive(Debug, Serialize, Deserialize)]
struct ContactsConfigV1 {
    contacts: Vec<ContactV1>,
    carddav_url: String,
    carddav_user: String,
    carddav_password: String,
}

/// Load the current config, or a card list saved before the second email and phone.
pub fn load_config(bytes: &[u8]) -> ContactsConfig {
    if let Ok(cfg) = crate::widget::config::restore_state::<ContactsConfig>(bytes) {
        return cfg;
    }
    let Ok(old) = crate::widget::config::restore_state::<ContactsConfigV1>(bytes) else {
        return ContactsConfig::default();
    };
    ContactsConfig {
        contacts: old
            .contacts
            .into_iter()
            .map(|card| Contact {
                id: card.id,
                name: card.name,
                email: card.email,
                phone: card.phone,
                notes: card.notes,
                href: card.href,
                etag: card.etag,
                email2: String::new(),
                phone2: String::new(),
            })
            .collect(),
        carddav_url: old.carddav_url,
        carddav_user: old.carddav_user,
        carddav_password: old.carddav_password,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_cards_load_with_an_empty_second_email_and_phone() {
        let old = ContactsConfigV1 {
            contacts: vec![ContactV1 {
                id: "a".into(),
                name: "Ada".into(),
                email: "a@example.com".into(),
                phone: "1".into(),
                notes: String::new(),
                href: String::new(),
                etag: String::new(),
            }],
            carddav_url: String::new(),
            carddav_user: String::new(),
            carddav_password: String::new(),
        };
        let bytes = crate::widget::config::save_state(&old).expect("encode");
        let loaded = load_config(&bytes);
        assert_eq!(loaded.contacts[0].email, "a@example.com");
        assert!(loaded.contacts[0].email2.is_empty());
        assert!(loaded.contacts[0].phone2.is_empty());

        let mut current = loaded;
        current.contacts[0].email2 = "b@example.com".into();
        current.contacts[0].phone2 = "2".into();
        let bytes = crate::widget::config::save_state(&current).expect("encode");
        let again = load_config(&bytes);
        assert_eq!(again.contacts[0].email2, "b@example.com");
        assert_eq!(again.contacts[0].phone2, "2");
    }
}
