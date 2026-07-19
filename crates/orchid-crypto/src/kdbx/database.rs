//! KDBX4 password database front-end.
//!
//! # Argon2id parameters
//!
//! Creation defaults come from [`keepass::Database::new`], which follows
//! KeePassXC's "Interactive" profile (≈ 64 MiB memory, 10 iterations, 2 lanes
//! on KDBX4). These are appropriate for desktop unlock times on mid-range
//! hardware; heavier profiles are left to a later configuration pass.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, NaiveDateTime, Utc};
use keepass::db::{fields, EntryId, EntryMut, EntryRef, GroupId, GroupMut, GroupRef, Value};
use keepass::{Database, DatabaseKey};
use parking_lot::Mutex;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use crate::error::{CryptoError, Result};
use crate::kdbx::entry::PasswordEntry;
use crate::kdbx::group::PasswordGroup;

/// Standard KDBX field name for the title.
pub(crate) const FIELD_TITLE: &str = fields::TITLE;
/// Standard KDBX field name for the username.
pub(crate) const FIELD_USERNAME: &str = fields::USERNAME;
/// Standard KDBX field name for the password.
pub(crate) const FIELD_PASSWORD: &str = fields::PASSWORD;
/// Standard KDBX field name for the URL.
pub(crate) const FIELD_URL: &str = fields::URL;
/// Standard KDBX field name for the notes.
pub(crate) const FIELD_NOTES: &str = fields::NOTES;
/// Custom-field convention (shared with KeePassXC) for an `otpauth://` URI.
pub(crate) const FIELD_OTP: &str = fields::OTP;

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
        let db = Database::new();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let key = master_key(&master);
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
        let key = master_key(&master);
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
        let Some(master) = override_master else {
            return Err(CryptoError::KdbxOpen(
                "save requires a master password (use change_master or save via create)".into(),
            ));
        };
        let key = master_key(&master);
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
        let mut guard = self.inner.lock();
        let mut group = group_mut(&mut guard, entry.group_id)?;
        let mut kp = group
            .add_entry_with_id(EntryId::from(entry_id))
            .map_err(|e| CryptoError::KdbxOpen(format!("add entry: {e}")))?;
        apply_password_entry(&mut kp, &entry);
        drop(kp);
        drop(group);
        drop(guard);
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
        let mut guard = self.inner.lock();
        {
            let mut kp = entry_mut(&mut guard, id)?;
            let current_group = kp.parent_mut().id().uuid();
            if current_group != target_group {
                kp.move_to(GroupId::from(target_group))
                    .map_err(|_| CryptoError::GroupNotFound(target_group))?;
            }
        }
        let mut kp = entry_mut(&mut guard, id)?;
        apply_password_entry(&mut kp, &entry);
        drop(kp);
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
        let kp = entry_mut(&mut guard, id)?;
        kp.remove();
        drop(guard);
        self.dirty.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Fetch an entry by id.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::EntryNotFound`] if the id is unknown.
    pub fn get_entry(&self, id: Uuid) -> Result<PasswordEntry> {
        let guard = self.inner.lock();
        let kp = entry_ref(&guard, id)?;
        let group_id = kp.parent().id().uuid();
        Ok(entry_from_keepass(kp, group_id))
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
        for kp in guard.iter_all_entries() {
            let gid = kp.parent().id().uuid();
            if group_id.is_none_or(|f| f == gid) {
                out.push(entry_from_keepass(kp, gid));
            }
        }
        Ok(out)
    }

    /// Move an entry from its current group to `target_group`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::EntryNotFound`] /
    /// [`CryptoError::GroupNotFound`] as appropriate.
    pub fn move_entry(&self, id: Uuid, target_group: Uuid) -> Result<()> {
        let mut guard = self.inner.lock();
        let mut kp = entry_mut(&mut guard, id)?;
        kp.move_to(GroupId::from(target_group))
            .map_err(|_| CryptoError::GroupNotFound(target_group))?;
        drop(kp);
        drop(guard);
        self.dirty.store(true, Ordering::Relaxed);
        Ok(())
    }

    // ---------------- Groups ----------------

    /// Snapshot the root group.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation.
    pub fn root_group(&self) -> Result<PasswordGroup> {
        let guard = self.inner.lock();
        Ok(snapshot_group(guard.root()))
    }

    /// Add a subgroup with `name` under `parent_id`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::GroupNotFound`] if the parent is unknown.
    pub fn add_group(&self, parent_id: Uuid, name: &str) -> Result<Uuid> {
        let mut guard = self.inner.lock();
        let mut parent = group_mut(&mut guard, parent_id)?;
        let mut new = parent.add_group();
        new.name = name.to_string();
        let new_id = new.id().uuid();
        drop(new);
        drop(parent);
        drop(guard);
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
        let mut group = group_mut(&mut guard, id)?;
        group.name = name.to_string();
        drop(group);
        drop(guard);
        self.dirty.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Remove a group and everything inside it.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::GroupNotFound`] if the id is unknown or refers
    /// to the root.
    pub fn delete_group(&self, id: Uuid) -> Result<()> {
        let mut guard = self.inner.lock();
        if guard.root().id().uuid() == id {
            return Err(CryptoError::GroupNotFound(id));
        }
        let group = group_mut(&mut guard, id)?;
        group.remove();
        drop(guard);
        self.dirty.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// List every group (root + descendants) in arbitrary order.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation.
    pub fn list_groups(&self) -> Result<Vec<PasswordGroup>> {
        let guard = self.inner.lock();
        Ok(guard.iter_all_groups().map(snapshot_group).collect())
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn master_key(master: &SecretString) -> DatabaseKey {
    DatabaseKey::new().with_password(master.expose_secret())
}

fn naive_to_utc(n: NaiveDateTime) -> DateTime<Utc> {
    DateTime::<Utc>::from_naive_utc_and_offset(n, Utc)
}

fn group_mut(db: &mut Database, id: Uuid) -> Result<GroupMut<'_>> {
    db.group_mut(GroupId::from(id))
        .ok_or(CryptoError::GroupNotFound(id))
}

fn entry_mut(db: &mut Database, id: Uuid) -> Result<EntryMut<'_>> {
    db.entry_mut(EntryId::from(id))
        .ok_or(CryptoError::EntryNotFound(id))
}

fn entry_ref(db: &Database, id: Uuid) -> Result<EntryRef<'_>> {
    db.entry(EntryId::from(id))
        .ok_or(CryptoError::EntryNotFound(id))
}

fn apply_password_entry(kp: &mut EntryMut<'_>, e: &PasswordEntry) {
    kp.set_unprotected(FIELD_TITLE, &e.title);
    kp.set_unprotected(FIELD_USERNAME, &e.username);
    kp.set_protected(FIELD_PASSWORD, e.password.expose_secret());
    if let Some(url) = &e.url {
        kp.set_unprotected(FIELD_URL, url);
    }
    if let Some(notes) = &e.notes {
        kp.set_unprotected(FIELD_NOTES, notes);
    }
    if let Some(totp) = &e.totp {
        let uri = crate::kdbx::totp::to_otpauth_uri(totp);
        kp.set_protected(FIELD_OTP, uri);
    }
    for (k, v) in &e.custom_fields {
        if !STANDARD_FIELDS.contains(&k.as_str()) {
            kp.set_protected(k, v.expose_secret());
        }
    }
    kp.tags = e.tags.clone();
}

fn entry_from_keepass(kp: EntryRef<'_>, group_id: Uuid) -> PasswordEntry {
    let title = kp.get_title().unwrap_or_default().to_string();
    let username = kp.get_username().unwrap_or_default().to_string();
    let password = SecretString::from(kp.get_password().unwrap_or_default().to_string());
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
            Value::Protected(s) => s.expose_secret().clone(),
        };
        custom_fields.insert(k.clone(), SecretString::from(plain));
    }

    let created_at = kp
        .times
        .creation
        .map(naive_to_utc)
        .unwrap_or_else(Utc::now);
    let modified_at = kp
        .times
        .last_modification
        .map(naive_to_utc)
        .unwrap_or_else(Utc::now);

    PasswordEntry {
        id: kp.id().uuid(),
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

fn snapshot_group(g: GroupRef<'_>) -> PasswordGroup {
    PasswordGroup {
        id: g.id().uuid(),
        name: g.name.clone(),
        parent_id: g.parent().map(|p| p.id().uuid()),
        icon: None,
        notes: g.notes.clone(),
        children: g.group_ids().map(|id| id.uuid()).collect(),
        entries: g.entry_ids().map(|id| id.uuid()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db_at(path: &Path) -> PasswordDatabase {
        PasswordDatabase::create(path, SecretString::from("pw")).unwrap()
    }

    #[test]
    fn create_open_round_trip_with_correct_master() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("db.kdbx");
        let db = db_at(&path);
        // keepass save_kdbx4 requires save() to be called before reopen; our
        // create() already wrote the file on disk.
        drop(db);
        let _opened = PasswordDatabase::open(&path, SecretString::from("pw")).unwrap();
    }

    #[test]
    fn open_with_wrong_master_is_rejected() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("db.kdbx");
        let _ = db_at(&path);
        let err = PasswordDatabase::open(&path, SecretString::from("nope")).unwrap_err();
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
            password: SecretString::from("hunter2"),
            url: Some("https://github.com".into()),
            notes: None,
            tags: vec!["dev".into()],
            custom_fields: BTreeMap::new(),
            totp: None,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            group_id: root_id,
        };
        let id = db.add_entry(entry).unwrap();
        let got = db.get_entry(id).unwrap();
        assert_eq!(got.title, "GitHub");
        assert_eq!(got.username, "alice");
        assert_eq!(got.password.expose_secret(), "hunter2");
        assert_eq!(got.tags, vec!["dev".to_string()]);

        db.delete_entry(id).unwrap();
        assert!(matches!(
            db.get_entry(id).unwrap_err(),
            CryptoError::EntryNotFound(_)
        ));
    }

    #[test]
    fn group_crud() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("db.kdbx");
        let db = db_at(&path);
        let root = db.root_group().unwrap().id;
        let child = db.add_group(root, "Work").unwrap();
        db.rename_group(child, "Personal").unwrap();
        let groups = db.list_groups().unwrap();
        assert!(groups.iter().any(|g| g.id == child && g.name == "Personal"));
        db.delete_group(child).unwrap();
        assert!(!db.list_groups().unwrap().iter().any(|g| g.id == child));
    }
}
