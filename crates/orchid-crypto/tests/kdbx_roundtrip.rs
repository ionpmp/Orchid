//! End-to-end KDBX create → populate → save → reopen.

use std::collections::BTreeMap;

use chrono::Utc;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use orchid_crypto::{PasswordDatabase, PasswordEntry};

#[test]
fn populate_save_reopen_preserves_entries_and_groups() {
    let td = tempfile::tempdir().unwrap();
    let path = td.path().join("vault.kdbx");

    let db = PasswordDatabase::create(&path, SecretString::new("master".into())).unwrap();
    let root = db.root_group().unwrap().id;

    // Ten groups with ten entries each.
    let mut titles: Vec<String> = Vec::new();
    for g in 0..10 {
        let group_id = db.add_group(root, &format!("Group-{g}")).unwrap();
        for i in 0..10 {
            let title = format!("Entry-{g}-{i}");
            titles.push(title.clone());
            let entry = PasswordEntry {
                id: Uuid::new_v4(),
                title,
                username: format!("user-{g}-{i}"),
                password: SecretString::new(format!("pw-{g}-{i}")),
                url: Some(format!("https://example.com/{g}/{i}")),
                notes: None,
                tags: vec![format!("g{g}")],
                custom_fields: BTreeMap::new(),
                totp: None,
                created_at: Utc::now(),
                modified_at: Utc::now(),
                group_id,
            };
            db.add_entry(entry).unwrap();
        }
    }

    // Persist and reopen.
    db.change_master(SecretString::new("master".into())).unwrap();
    drop(db);

    let reopened =
        PasswordDatabase::open(&path, SecretString::new("master".into())).unwrap();
    let entries = reopened.list_entries(None).unwrap();
    assert_eq!(entries.len(), 100, "all 100 entries survived");

    for expected in &titles {
        assert!(
            entries.iter().any(|e| &e.title == expected),
            "missing entry {expected}"
        );
    }

    // Spot-check a password.
    let sample = entries.iter().find(|e| e.title == "Entry-3-7").unwrap();
    assert_eq!(sample.password.expose_secret(), "pw-3-7");
    assert_eq!(sample.username, "user-3-7");

    let groups = reopened.list_groups().unwrap();
    // root + 10 subgroups
    assert!(groups.iter().any(|g| g.name == "Group-5"));
    assert!(groups.len() >= 11);
}
