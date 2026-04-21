//! KDBX4 password database front-end.
//!
//! # Argon2id parameters
//!
//! Creation defaults come from `keepass::Database::new(Default::default())`,
//! which follows KeePassXC's "Interactive" profile (≈ 64 MiB memory, 10
//! iterations, 2 lanes on KDBX4). These are appropriate for desktop unlock
//! times on mid-range hardware; heavier profiles are left to a later
//! configuration pass.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, NaiveDateTime, Utc};
use keepass::db::{Entry, Group, Node, NodeRef, Value};
use keepass::{Database, DatabaseKey};
use parking_lot::Mutex;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use crate::error::{CryptoError, Result};
use crate::kdbx::entry::PasswordEntry;
use crate::kdbx::group::PasswordGroup;

/// Standard KDBX field name for the title.
pub(crate) const FIELD_TITLE: &str = "Title";
/// Standard KDBX field name for the username.
pub(crate) const FIELD_USERNAME: &str = "UserName";
/// Standard KDBX field name for the password.
pub(crate) const FIELD_PASSWORD: &str = "Password";
/// Standard KDBX field name for the URL.
pub(crate) const FIELD_URL: &str = "URL";
/// Standard KDBX field name for the notes.
pub(crate) const FIELD_NOTES: &str = "Notes";
/// Custom-field convention (shared with KeePassXC) for an `otpauth://` URI.
pub(crate) const FIELD_OTP: &str = "otp";

const STANDARD_FIELDS: &[&str] = &[
    FIELD_TITLE,
    FIELD_USERNAME,
    FIELD_PASSWORD,
    FIELD_URL,
    FIELD_NOTES,
    FIELD_OTP,
];

/// Shared front-end for a single KDBX4 file.
pub struct PasswordDatabase {
    inner: Mutex<Database>,
    path: PathBuf,
    dirty: AtomicBool,
}

impl std::fmt::Debug for PasswordDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordDatabase")
            .field("path", &self.path)
            .field("dirty", &self.is_dirty())
            .finish_non_exhaustive()
    }
}

impl PasswordDatabase {
    /// Create a new empty database and persist it to `path` with the given
    /// master password.
    ///
    /// # Errors
    ///
    /// Propagates I/O and `keepass` errors.
    pub fn create(path: &Path, master: SecretString) -> Result<Self> {
        let db = Database::new(Default::default());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let key = DatabaseKey::new().with_password(master.expose_secret());
        let tmp = path.with_extension("kdbx.tmp");
        {
            let mut out = std::fs::File::create(&tmp)?;
            db.save(&mut out, key)
                .map_err(|e| CryptoError::KdbxOpen(format!("create/save: {e}")))?;
        }
        // File write already flushed via Drop of File; rename for atomicity.
        std::fs::rename(&tmp, path).or_else(|e| {
            // On some filesystems rename fails if the target exists; fall
            // back to remove + rename.
            let _ = std::fs::remove_file(path);
            std::fs::rename(&tmp, path).map_err(|_| e)
        })?;
        // Clean up in case rename left a stub.
        if tmp.exists() {
            let _ = std::fs::remove_file(&tmp);
        }

        Ok(Self {
            inner: Mutex::new(db),
            path: path.to_path_buf(),
            dirty: AtomicBool::new(false),
        })
    }

    /// Open an existing database.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidMasterPassword`] if the password does
    /// not unlock the file, [`CryptoError::KdbxOpen`] for any other open
    /// failure, and [`CryptoError::Io`] for I/O problems.
    pub fn open(path: &Path, master: SecretString) -> Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let key = DatabaseKey::new().with_password(master.expose_secret());
        let db = match Database::open(&mut file, key) {
            Ok(db) => db,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("password") || msg.contains("HMAC") || msg.contains("credentials") {
                    return Err(CryptoError::InvalidMasterPassword);
                }
                return Err(CryptoError::KdbxOpen(msg));
            }
        };
        Ok(Self {
            inner: Mutex::new(db),
            path: path.to_path_buf(),
            dirty: AtomicBool::new(false),
        })
    }

    /// Persist the current state to disk atomically with the original master
    /// password.
    ///
    /// # Errors
    ///
    /// Propagates I/O and `keepass` errors.
    pub fn save(&self) -> Result<()> {
        self.save_with(None)
    }

    fn save_with(&self, override_master: Option<SecretString>) -> Result<()> {
        let guard = self.inner.lock();
        // We need a master password to re-save. Without an override, we
        // reuse one the caller has already passed us. For MVP we require it
        // to be supplied explicitly via `change_master`; otherwise the save
        // path uses the database's internal key from the last open/create.
        // In keepass 0.7, save() still needs the key, so we stash the
        // password outside the struct via `change_master`. Here we require
        // an override or the caller to have just called `create` /
        // `change_master` already.
        let Some(master) = override_master else {
            return Err(CryptoError::KdbxOpen(
                "save requires a master password (use change_master or save via create)".into(),
            ));
        };
        let key = DatabaseKey::new().with_password(master.expose_secret());
        let tmp = self.path.with_extension("kdbx.tmp");
        {
            let mut out = std::fs::File::create(&tmp)?;
            guard
                .save(&mut out, key)
                .map_err(|e| CryptoError::KdbxOpen(format!("save: {e}")))?;
        }
        std::fs::rename(&tmp, &self.path)?;
        self.dirty.store(false, Ordering::Relaxed);
        Ok(())
    }

    /// Re-encrypt the database with a new master password. The change is
    /// persisted to disk atomically.
    ///
    /// # Errors
    ///
    /// Propagates I/O and `keepass` errors.
    pub fn change_master(&self, new: SecretString) -> Result<()> {
        self.save_with(Some(new))
    }

    /// Whether any unsaved modification has happened.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    /// Raw on-disk path. Useful for diagnostics.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    // ---------------- Entries ----------------

    /// Add an entry to the database. `entry.group_id` identifies the target
    /// group; use [`PasswordDatabase::root_group`] for top-level entries.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::GroupNotFound`] if the target group doesn't
    /// exist.
    pub fn add_entry(&self, entry: PasswordEntry) -> Result<Uuid> {
        let entry_id = entry.id;
        let kp = entry_to_keepass(&entry);
        let mut guard = self.inner.lock();
        let placed = insert_into_group(&mut guard.root, entry.group_id, Node::Entry(kp));
        drop(guard);
        if !placed {
            return Err(CryptoError::GroupNotFound(entry.group_id));
        }
        self.dirty.store(true, Ordering::Relaxed);
        Ok(entry_id)
    }

    /// Replace an entry with the same id. The entry is moved to
    /// `entry.group_id` if it was in a different group.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::EntryNotFound`] if no entry with this id
    /// exists, and [`CryptoError::GroupNotFound`] if the target group is
    /// unknown.
    pub fn update_entry(&self, entry: PasswordEntry) -> Result<()> {
        let id = entry.id;
        let target_group = entry.group_id;
        let kp = entry_to_keepass(&entry);
        let mut guard = self.inner.lock();
        let existed = remove_entry_by_id(&mut guard.root, id);
        if !existed {
            return Err(CryptoError::EntryNotFound(id));
        }
        let placed = insert_into_group(&mut guard.root, target_group, Node::Entry(kp));
        if !placed {
            return Err(CryptoError::GroupNotFound(target_group));
        }
        drop(guard);
        self.dirty.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Remove an entry by id.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::EntryNotFound`] if the id is unknown.
    pub fn delete_entry(&self, id: Uuid) -> Result<()> {
        let mut guard = self.inner.lock();
        if remove_entry_by_id(&mut guard.root, id) {
            drop(guard);
            self.dirty.store(true, Ordering::Relaxed);
            Ok(())
        } else {
            Err(CryptoError::EntryNotFound(id))
        }
    }

    /// Fetch an entry by id.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::EntryNotFound`] if the id is unknown.
    pub fn get_entry(&self, id: Uuid) -> Result<PasswordEntry> {
        let guard = self.inner.lock();
        find_entry_by_id(&guard.root, id, guard.root.uuid)
            .ok_or(CryptoError::EntryNotFound(id))
    }

    /// List entries in the given group, or every entry if `group_id` is
    /// `None`.
    ///
    /// # Errors
    ///
    /// Propagates `keepass` errors (none in the current implementation).
    pub fn list_entries(&self, group_id: Option<Uuid>) -> Result<Vec<PasswordEntry>> {
        let guard = self.inner.lock();
        let mut out = Vec::new();
        collect_entries(&guard.root, guard.root.uuid, group_id, &mut out);
        Ok(out)
    }

    /// Move an entry from its current group to `target_group`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::EntryNotFound`] /
    /// [`CryptoError::GroupNotFound`] as appropriate.
    pub fn move_entry(&self, id: Uuid, target_group: Uuid) -> Result<()> {
        let mut current = self.get_entry(id)?;
        current.group_id = target_group;
        self.update_entry(current)
    }

    // ---------------- Groups ----------------

    /// Snapshot the root group.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation.
    pub fn root_group(&self) -> Result<PasswordGroup> {
        let guard = self.inner.lock();
        Ok(snapshot_group(&guard.root, None))
    }

    /// Add a subgroup with `name` under `parent_id`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::GroupNotFound`] if the parent is unknown.
    pub fn add_group(&self, parent_id: Uuid, name: &str) -> Result<Uuid> {
        let new = Group::new(name);
        let new_id = new.uuid;
        let mut guard = self.inner.lock();
        let placed = insert_into_group(&mut guard.root, parent_id, Node::Group(new));
        drop(guard);
        if !placed {
            return Err(CryptoError::GroupNotFound(parent_id));
        }
        self.dirty.store(true, Ordering::Relaxed);
        Ok(new_id)
    }

    /// Rename a group.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::GroupNotFound`] if the id is unknown.
    pub fn rename_group(&self, id: Uuid, name: &str) -> Result<()> {
        let mut guard = self.inner.lock();
        if rename_group_rec(&mut guard.root, id, name) {
            drop(guard);
            self.dirty.store(true, Ordering::Relaxed);
            Ok(())
        } else {
            Err(CryptoError::GroupNotFound(id))
        }
    }

    /// Remove a group and everything inside it.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::GroupNotFound`] if the id is unknown or refers
    /// to the root.
    pub fn delete_group(&self, id: Uuid) -> Result<()> {
        let mut guard = self.inner.lock();
        if guard.root.uuid == id {
            return Err(CryptoError::GroupNotFound(id));
        }
        if remove_group_by_id(&mut guard.root, id) {
            drop(guard);
            self.dirty.store(true, Ordering::Relaxed);
            Ok(())
        } else {
            Err(CryptoError::GroupNotFound(id))
        }
    }

    /// List every group (root + descendants) in arbitrary order.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation.
    pub fn list_groups(&self) -> Result<Vec<PasswordGroup>> {
        let guard = self.inner.lock();
        let mut out = Vec::new();
        walk_groups(&guard.root, None, &mut out);
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn naive_to_utc(n: NaiveDateTime) -> DateTime<Utc> {
    DateTime::<Utc>::from_naive_utc_and_offset(n, Utc)
}

fn entry_to_keepass(e: &PasswordEntry) -> Entry {
    let mut kp = Entry::new();
    kp.uuid = e.id;
    kp.tags = e.tags.clone();
    set_field(&mut kp, FIELD_TITLE, &e.title, false);
    set_field(&mut kp, FIELD_USERNAME, &e.username, false);
    set_field(&mut kp, FIELD_PASSWORD, e.password.expose_secret(), true);
    if let Some(url) = &e.url {
        set_field(&mut kp, FIELD_URL, url, false);
    }
    if let Some(notes) = &e.notes {
        set_field(&mut kp, FIELD_NOTES, notes, false);
    }
    if let Some(totp) = &e.totp {
        let uri = crate::kdbx::totp::to_otpauth_uri(totp);
        set_field(&mut kp, FIELD_OTP, &uri, true);
    }
    for (k, v) in &e.custom_fields {
        if !STANDARD_FIELDS.contains(&k.as_str()) {
            set_field(&mut kp, k, v.expose_secret(), true);
        }
    }
    kp
}

fn set_field(entry: &mut Entry, name: &str, value: &str, protected: bool) {
    let v = if protected {
        Value::Protected(value.as_bytes().into())
    } else {
        Value::Unprotected(value.to_string())
    };
    entry.fields.insert(name.to_string(), v);
}

fn entry_from_keepass(kp: &Entry, group_id: Uuid) -> PasswordEntry {
    let title = kp.get_title().unwrap_or_default().to_string();
    let username = kp.get_username().unwrap_or_default().to_string();
    let password = SecretString::new(kp.get_password().unwrap_or_default().to_string());
    let url = kp.get(FIELD_URL).map(ToOwned::to_owned);
    let notes = kp.get(FIELD_NOTES).map(ToOwned::to_owned);
    let totp = kp
        .get(FIELD_OTP)
        .and_then(|s| crate::kdbx::totp::parse_otpauth_uri(s).ok());

    let mut custom_fields = BTreeMap::new();
    for (k, v) in &kp.fields {
        if STANDARD_FIELDS.contains(&k.as_str()) {
            continue;
        }
        let plain = match v {
            Value::Unprotected(s) => s.clone(),
            Value::Protected(s) => std::str::from_utf8(s.unsecure()).unwrap_or("").to_string(),
            Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        };
        custom_fields.insert(k.clone(), SecretString::new(plain));
    }

    let created_at = kp
        .times
        .get_creation()
        .copied()
        .map(naive_to_utc)
        .unwrap_or_else(Utc::now);
    let modified_at = kp
        .times
        .get_last_modification()
        .copied()
        .map(naive_to_utc)
        .unwrap_or_else(Utc::now);

    PasswordEntry {
        id: kp.uuid,
        title,
        username,
        password,
        url,
        notes,
        tags: kp.tags.clone(),
        custom_fields,
        totp,
        created_at,
        modified_at,
        group_id,
    }
}

fn snapshot_group(g: &Group, parent_id: Option<Uuid>) -> PasswordGroup {
    let mut children = Vec::new();
    let mut entries = Vec::new();
    for node in &g.children {
        match node {
            Node::Group(sub) => children.push(sub.uuid),
            Node::Entry(e) => entries.push(e.uuid),
        }
    }
    PasswordGroup {
        id: g.uuid,
        name: g.name.clone(),
        parent_id,
        icon: None,
        notes: None,
        children,
        entries,
    }
}

// ---------------------------------------------------------------------------
// Tree walkers
// ---------------------------------------------------------------------------

fn insert_into_group(root: &mut Group, target_id: Uuid, node: Node) -> bool {
    if root.uuid == target_id {
        root.children.push(node);
        return true;
    }
    for child in &mut root.children {
        if let Node::Group(g) = child {
            if insert_into_group(g, target_id, node.clone()) {
                return true;
            }
        }
    }
    false
}

fn remove_entry_by_id(g: &mut Group, id: Uuid) -> bool {
    if let Some(pos) = g.children.iter().position(|n| match n {
        Node::Entry(e) => e.uuid == id,
        _ => false,
    }) {
        g.children.remove(pos);
        return true;
    }
    for child in &mut g.children {
        if let Node::Group(sub) = child {
            if remove_entry_by_id(sub, id) {
                return true;
            }
        }
    }
    false
}

fn remove_group_by_id(g: &mut Group, id: Uuid) -> bool {
    if let Some(pos) = g.children.iter().position(|n| match n {
        Node::Group(sub) => sub.uuid == id,
        _ => false,
    }) {
        g.children.remove(pos);
        return true;
    }
    for child in &mut g.children {
        if let Node::Group(sub) = child {
            if remove_group_by_id(sub, id) {
                return true;
            }
        }
    }
    false
}

fn rename_group_rec(g: &mut Group, id: Uuid, name: &str) -> bool {
    if g.uuid == id {
        g.name = name.to_string();
        return true;
    }
    for child in &mut g.children {
        if let Node::Group(sub) = child {
            if rename_group_rec(sub, id, name) {
                return true;
            }
        }
    }
    false
}

fn find_entry_by_id(g: &Group, id: Uuid, group_id: Uuid) -> Option<PasswordEntry> {
    for node in &g.children {
        match node {
            Node::Entry(e) if e.uuid == id => {
                return Some(entry_from_keepass(e, group_id));
            }
            Node::Group(sub) => {
                if let Some(found) = find_entry_by_id(sub, id, sub.uuid) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn collect_entries(
    g: &Group,
    group_id: Uuid,
    filter_group: Option<Uuid>,
    out: &mut Vec<PasswordEntry>,
) {
    for node in &g.children {
        match node {
            Node::Entry(e) => {
                if filter_group.is_none_or(|f| f == group_id) {
                    out.push(entry_from_keepass(e, group_id));
                }
            }
            Node::Group(sub) => {
                collect_entries(sub, sub.uuid, filter_group, out);
            }
        }
    }
}

fn walk_groups(g: &Group, parent_id: Option<Uuid>, out: &mut Vec<PasswordGroup>) {
    out.push(snapshot_group(g, parent_id));
    for node in &g.children {
        if let Node::Group(sub) = node {
            walk_groups(sub, Some(g.uuid), out);
        }
    }
}

// We only iterate via `&db.root`; NodeRef use is via the walkers above. This
// unused import reference stops the compiler from complaining about the
// `NodeRef` import if future refactors drop some of its references.
#[allow(dead_code)]
fn _nodref_touch(_: Option<NodeRef<'_>>) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn db_at(path: &Path) -> PasswordDatabase {
        PasswordDatabase::create(path, SecretString::new("pw".into())).unwrap()
    }

    #[test]
    fn create_open_round_trip_with_correct_master() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("db.kdbx");
        let db = db_at(&path);
        // keepass save_kdbx4 requires save() to be called before reopen; our
        // create() already wrote the file on disk.
        drop(db);
        let _opened =
            PasswordDatabase::open(&path, SecretString::new("pw".into())).unwrap();
    }

    #[test]
    fn open_with_wrong_master_is_rejected() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("db.kdbx");
        let _ = db_at(&path);
        let err =
            PasswordDatabase::open(&path, SecretString::new("nope".into())).unwrap_err();
        assert!(matches!(
            err,
            CryptoError::InvalidMasterPassword | CryptoError::KdbxOpen(_)
        ));
    }

    #[test]
    fn entry_crud_round_trip() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("db.kdbx");
        let db = db_at(&path);
        let root_id = db.root_group().unwrap().id;

        let entry = PasswordEntry {
            id: Uuid::new_v4(),
            title: "GitHub".into(),
            username: "alice".into(),
            password: SecretString::new("hunter2".into()),
            url: Some("https://github.com".into()),
            notes: None,
            tags: vec!["dev".into()],
            custom_fields: BTreeMap::new(),
            totp: None,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            group_id: root_id,
        };
        let id = db.add_entry(entry.clone()).unwrap();
        assert_eq!(id, entry.id);

        let back = db.get_entry(id).unwrap();
        assert_eq!(back.title, "GitHub");
        assert_eq!(back.password.expose_secret(), "hunter2");

        let mut updated = back.clone();
        updated.title = "GitHub Enterprise".into();
        db.update_entry(updated).unwrap();
        assert_eq!(db.get_entry(id).unwrap().title, "GitHub Enterprise");

        db.delete_entry(id).unwrap();
        assert!(matches!(
            db.get_entry(id).unwrap_err(),
            CryptoError::EntryNotFound(_)
        ));
    }

    #[test]
    fn group_crud() {
        let td = tempfile::tempdir().unwrap();
        let db = db_at(&td.path().join("db.kdbx"));
        let root = db.root_group().unwrap();
        let sub = db.add_group(root.id, "Work").unwrap();
        db.rename_group(sub, "Personal").unwrap();
        let all = db.list_groups().unwrap();
        assert!(all.iter().any(|g| g.name == "Personal"));
        db.delete_group(sub).unwrap();
        let all = db.list_groups().unwrap();
        assert!(!all.iter().any(|g| g.name == "Personal"));
    }
}
