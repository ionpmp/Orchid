//! Public facade over the redb state database.
//!
//! [`StateStore`] owns the underlying [`redb::Database`] and exposes typed
//! read / write transactions. All persistent operations in Orchid go through
//! this type.
//!
//! `StateStore` is deliberately synchronous — redb itself is synchronous, and
//! wrapping it in async would only add indirection without a throughput
//! benefit.

use std::fs;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use redb::{Database, ReadableTable};
use uuid::Uuid;

use crate::error::{Result, StorageError};
use crate::state::migrations;
use crate::state::tables::{
    uuid_key, CACHE_TABLE, FILE_TAGS_TABLE, HISTORY_BY_TIMESTAMP_INDEX, HISTORY_TABLE,
    SESSION_KEY_CURRENT, SESSION_STATE_TABLE, WIDGET_INSTANCES_TABLE, WORKSPACES_TABLE,
};
use crate::state::types::{
    CacheEntry, CacheKind, FileTag, HistoryEntry, SessionState, WidgetInstance, Workspace,
};

/// Typed wrapper around a [`redb::Database`] carrying the Orchid state tables.
#[derive(Debug, Clone)]
pub struct StateStore {
    db: Arc<Database>,
    schema_version: u32,
}

impl StateStore {
    /// Open (or create) the state database at `path`, running migrations as
    /// necessary.
    ///
    /// The `orchid_version` string is written into [`crate::SchemaMeta`] so
    /// that diagnostic tooling can tell which build last touched this DB.
    ///
    /// # Errors
    ///
    /// Propagates any redb open / transaction / table error, and any
    /// [`crate::StorageError::UnsupportedSchemaVersion`] /
    /// [`crate::StorageError::MigrationFailed`] raised by the migration
    /// engine.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_storage::StateStore;
    /// let tmp = tempfile::tempdir().unwrap();
    /// let path = tmp.path().join("state.redb");
    /// let store = StateStore::open(&path, "0.1.0-test").unwrap();
    /// assert_eq!(store.schema_version(), orchid_storage::CURRENT_SCHEMA_VERSION);
    /// ```
    pub fn open(path: &Path, orchid_version: &str) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let db = Database::create(path)?;
        let schema_version = migrations::initialise(&db, orchid_version)?;
        Ok(Self {
            db: Arc::new(db),
            schema_version,
        })
    }

    /// Open an ephemeral, in-memory state database. Intended for tests.
    ///
    /// # Errors
    ///
    /// Propagates redb errors during backend creation and migration.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_storage::StateStore;
    /// let store = StateStore::open_in_memory("0.0-test").unwrap();
    /// let _ = store.read().unwrap();
    /// ```
    pub fn open_in_memory(orchid_version: &str) -> Result<Self> {
        let db = Database::builder()
            .create_with_backend(redb::backends::InMemoryBackend::new())?;
        let schema_version = migrations::initialise(&db, orchid_version)?;
        Ok(Self {
            db: Arc::new(db),
            schema_version,
        })
    }

    /// Begin a read transaction.
    ///
    /// # Errors
    ///
    /// Propagates [`StorageError::RedbTransaction`].
    pub fn read(&self) -> Result<ReadTransaction<'_>> {
        let inner = self.db.begin_read()?;
        Ok(ReadTransaction {
            inner,
            _lt: PhantomData,
        })
    }

    /// Begin a write transaction.
    ///
    /// redb serialises write transactions, so if another writer is already
    /// active this call blocks until it completes.
    ///
    /// # Errors
    ///
    /// Propagates [`StorageError::RedbTransaction`].
    pub fn write(&self) -> Result<WriteTransaction<'_>> {
        let inner = self.db.begin_write()?;
        Ok(WriteTransaction {
            inner: Some(inner),
            _lt: PhantomData,
        })
    }

    /// Run redb's space-reclaiming compaction. Expensive: acquires an
    /// exclusive lock and rewrites live pages.
    ///
    /// Requires exclusive ownership of the [`StateStore`] — if any clone of
    /// the handle is still alive on another thread this method returns
    /// [`StorageError::PathResolution`].
    ///
    /// # Errors
    ///
    /// Propagates [`StorageError::RedbCompaction`] and
    /// [`StorageError::PathResolution`].
    pub fn compact(&mut self) -> Result<()> {
        let db = Arc::get_mut(&mut self.db).ok_or_else(|| {
            StorageError::PathResolution(
                "compact() requires exclusive ownership of the StateStore".into(),
            )
        })?;
        db.compact()?;
        Ok(())
    }

    /// Schema version detected / migrated to when this store was opened.
    #[must_use]
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Raw access to the underlying [`redb::Database`] for diagnostics and
    /// administrative tooling. Use sparingly: typed transactions on
    /// `StateStore` are the supported API.
    #[must_use]
    pub fn raw_database(&self) -> &Database {
        &self.db
    }
}

// =============================================================================
// Read transactions
// =============================================================================

/// A read-only transaction against the state database.
pub struct ReadTransaction<'a> {
    inner: redb::ReadTransaction,
    _lt: PhantomData<&'a Database>,
}

impl std::fmt::Debug for ReadTransaction<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadTransaction").finish_non_exhaustive()
    }
}

impl ReadTransaction<'_> {
    /// Fetch a history entry by its id.
    ///
    /// # Errors
    ///
    /// Propagates redb errors if the table cannot be opened or read.
    pub fn get_history(&self, id: Uuid) -> Result<Option<HistoryEntry>> {
        let table = match self.inner.open_table(HISTORY_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(table.get(&uuid_key(id))?.map(|g| g.value()))
    }

    /// Return the `limit` most recent history entries, newest first, using
    /// the timestamp secondary index.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn iter_history_recent(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let index = match self.inner.open_table(HISTORY_BY_TIMESTAMP_INDEX) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let primary = self.inner.open_table(HISTORY_TABLE)?;

        let mut out = Vec::with_capacity(limit);
        for entry in index.iter()?.rev() {
            let (_ts, key) = entry?;
            let key_bytes = key.value();
            if let Some(g) = primary.get(&key_bytes)? {
                out.push(g.value());
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Return history entries whose timestamps fall in `[from, to)`, in
    /// chronological order.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn iter_history_range(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<HistoryEntry>> {
        let index = match self.inner.open_table(HISTORY_BY_TIMESTAMP_INDEX) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let primary = self.inner.open_table(HISTORY_TABLE)?;

        let from_ms = from.timestamp_millis();
        let to_ms = to.timestamp_millis();

        let mut out = Vec::new();
        for entry in index.range(from_ms..to_ms)? {
            let (_ts, key) = entry?;
            let key_bytes = key.value();
            if let Some(g) = primary.get(&key_bytes)? {
                out.push(g.value());
            }
        }
        Ok(out)
    }

    /// Fetch a widget instance by id.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn get_widget(&self, id: Uuid) -> Result<Option<WidgetInstance>> {
        let table = match self.inner.open_table(WIDGET_INSTANCES_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(table.get(&uuid_key(id))?.map(|g| g.value()))
    }

    /// Return every widget bound to the given workspace.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn widgets_for_workspace(&self, workspace_id: Uuid) -> Result<Vec<WidgetInstance>> {
        let table = match self.inner.open_table(WIDGET_INSTANCES_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for entry in table.iter()? {
            let (_k, v) = entry?;
            let w = v.value();
            if w.workspace_id == workspace_id {
                out.push(w);
            }
        }
        Ok(out)
    }

    /// Return every widget instance in the database (unordered).
    ///
    /// Prefer this over walking workspaces when loading instances: rows must
    /// round-trip even if workspace metadata is temporarily inconsistent.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn list_all_widgets(&self) -> Result<Vec<WidgetInstance>> {
        let table = match self.inner.open_table(WIDGET_INSTANCES_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for entry in table.iter()? {
            let (_k, v) = entry?;
            out.push(v.value());
        }
        Ok(out)
    }

    /// Fetch a workspace by id.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn get_workspace(&self, id: Uuid) -> Result<Option<Workspace>> {
        let table = match self.inner.open_table(WORKSPACES_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(table.get(&uuid_key(id))?.map(|g| g.value()))
    }

    /// Return every workspace, unordered.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        let table = match self.inner.open_table(WORKSPACES_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for entry in table.iter()? {
            let (_k, v) = entry?;
            out.push(v.value());
        }
        Ok(out)
    }

    /// Fetch a file tag record by canonical path.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn get_file_tag(&self, path: &str) -> Result<Option<FileTag>> {
        let table = match self.inner.open_table(FILE_TAGS_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(table.get(path)?.map(|g| g.value()))
    }

    /// Return the last saved session snapshot, if any.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn get_session_state(&self) -> Result<Option<SessionState>> {
        let table = match self.inner.open_table(SESSION_STATE_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(table.get(SESSION_KEY_CURRENT)?.map(|g| g.value()))
    }

    /// Fetch a cache entry by its 32-byte hash key.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn get_cache(&self, key: &[u8; 32]) -> Result<Option<CacheEntry>> {
        let table = match self.inner.open_table(CACHE_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(table.get(key)?.map(|g| g.value()))
    }

    /// Load the persisted notification-center list, if any.
    ///
    /// # Errors
    ///
    /// Propagates redb / bincode errors.
    pub fn get_notification_center(&self) -> Result<Option<crate::NotificationCenterState>> {
        let Some(entry) = self.get_cache(&NOTIFICATION_CENTER_CACHE_KEY)? else {
            return Ok(None);
        };
        if entry.kind != CacheKind::NotificationCenter {
            return Ok(None);
        }
        let state = crate::state::codec::bincode_decode(&entry.data)?;
        Ok(Some(state))
    }

    /// Total bytes stored in the cache table (sum of `size_bytes`).
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn total_cache_bytes(&self) -> Result<u64> {
        let table = match self.inner.open_table(CACHE_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(0),
            Err(e) => return Err(e.into()),
        };
        let mut total: u64 = 0;
        for entry in table.iter()? {
            let (_k, v) = entry?;
            total = total.saturating_add(v.value().size_bytes);
        }
        Ok(total)
    }
}

/// Stable cache key for [`crate::NotificationCenterState`].
pub const NOTIFICATION_CENTER_CACHE_KEY: [u8; 32] = *b"orchid.notif.center.v1\0\0\0\0\0\0\0\0\0\0";

// =============================================================================
// Write transactions
// =============================================================================

/// A write transaction against the state database.
///
/// Call [`WriteTransaction::commit`] to make your changes durable. Dropping
/// the transaction without committing rolls back.
pub struct WriteTransaction<'a> {
    // `Option` so we can `take()` the inner transaction in `commit()`; after
    // commit we are an empty shell and the value is consumed.
    inner: Option<redb::WriteTransaction>,
    _lt: PhantomData<&'a Database>,
}

impl std::fmt::Debug for WriteTransaction<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriteTransaction")
            .field("committed", &self.inner.is_none())
            .finish()
    }
}

impl WriteTransaction<'_> {
    fn inner(&self) -> Result<&redb::WriteTransaction> {
        self.inner.as_ref().ok_or_else(|| {
            StorageError::PathResolution("write transaction already committed".into())
        })
    }

    /// Insert or overwrite a history entry, updating the timestamp index in
    /// the same transaction.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn put_history(&mut self, entry: &HistoryEntry) -> Result<()> {
        let txn = self.inner()?;
        let key = uuid_key(entry.id);
        let ts_ms = entry.timestamp.timestamp_millis();

        {
            let mut primary = txn.open_table(HISTORY_TABLE)?;
            // Remove the old timestamp index row if the entry already exists
            // under a different timestamp.
            let prior_ts = primary.get(&key)?.map(|g| g.value().timestamp.timestamp_millis());
            primary.insert(&key, entry)?;
            drop(primary);

            let mut index = txn.open_table(HISTORY_BY_TIMESTAMP_INDEX)?;
            if let Some(prev) = prior_ts {
                if prev != ts_ms {
                    let _ = index.remove(prev)?;
                }
            }
            index.insert(ts_ms, &key)?;
        }
        Ok(())
    }

    /// Remove a history entry (and its timestamp index row). Returns `true`
    /// if anything was deleted.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn delete_history(&mut self, id: Uuid) -> Result<bool> {
        let txn = self.inner()?;
        let key = uuid_key(id);

        // Look the entry up first so we can free the access guard before
        // mutating the table; borrow-checker gymnastics around the temporary
        // produced by `?` force this shape.
        let mut primary = txn.open_table(HISTORY_TABLE)?;
        let ts_ms_opt: Option<i64> = primary
            .get(&key)?
            .map(|g| g.value().timestamp.timestamp_millis());
        let Some(ts_ms) = ts_ms_opt else {
            return Ok(false);
        };
        let _ = primary.remove(&key)?;
        drop(primary);

        let mut index = txn.open_table(HISTORY_BY_TIMESTAMP_INDEX)?;
        let _ = index.remove(ts_ms)?;
        Ok(true)
    }

    /// Insert or overwrite a widget instance.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn put_widget(&mut self, widget: &WidgetInstance) -> Result<()> {
        let txn = self.inner()?;
        let mut table = txn.open_table(WIDGET_INSTANCES_TABLE)?;
        table.insert(&uuid_key(widget.id), widget)?;
        Ok(())
    }

    /// Remove a widget instance. Returns whether a row was deleted.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn delete_widget(&mut self, id: Uuid) -> Result<bool> {
        let txn = self.inner()?;
        let mut table = txn.open_table(WIDGET_INSTANCES_TABLE)?;
        let existed = table.remove(&uuid_key(id))?.is_some();
        Ok(existed)
    }

    /// Insert or overwrite a workspace.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn put_workspace(&mut self, ws: &Workspace) -> Result<()> {
        let txn = self.inner()?;
        let mut table = txn.open_table(WORKSPACES_TABLE)?;
        table.insert(&uuid_key(ws.id), ws)?;
        Ok(())
    }

    /// Remove a workspace. Returns whether a row was deleted.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn delete_workspace(&mut self, id: Uuid) -> Result<bool> {
        let txn = self.inner()?;
        let mut table = txn.open_table(WORKSPACES_TABLE)?;
        let existed = table.remove(&uuid_key(id))?.is_some();
        Ok(existed)
    }

    /// Insert or overwrite a file tag record. The record's own `path` field
    /// is used as the key.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn put_file_tag(&mut self, tag: &FileTag) -> Result<()> {
        let txn = self.inner()?;
        let mut table = txn.open_table(FILE_TAGS_TABLE)?;
        table.insert(tag.path.as_str(), tag)?;
        Ok(())
    }

    /// Delete a file tag record by path. Returns whether a row existed.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn delete_file_tag(&mut self, path: &str) -> Result<bool> {
        let txn = self.inner()?;
        let mut table = txn.open_table(FILE_TAGS_TABLE)?;
        let existed = table.remove(path)?.is_some();
        Ok(existed)
    }

    /// Replace the singleton session state record.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn set_session_state(&mut self, state: &SessionState) -> Result<()> {
        let txn = self.inner()?;
        let mut table = txn.open_table(SESSION_STATE_TABLE)?;
        table.insert(SESSION_KEY_CURRENT, state)?;
        Ok(())
    }

    /// Insert or overwrite a cache entry.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn put_cache(&mut self, entry: &CacheEntry) -> Result<()> {
        let txn = self.inner()?;
        let mut table = txn.open_table(CACHE_TABLE)?;
        table.insert(&entry.key, entry)?;
        Ok(())
    }

    /// Persist the notification-center list (replaces any previous snapshot).
    ///
    /// # Errors
    ///
    /// Propagates redb / bincode errors.
    pub fn put_notification_center(
        &mut self,
        state: &crate::NotificationCenterState,
    ) -> Result<()> {
        let data = crate::state::codec::bincode_encode(state)?;
        let now = Utc::now();
        let entry = CacheEntry {
            key: NOTIFICATION_CENTER_CACHE_KEY,
            kind: CacheKind::NotificationCenter,
            created_at: now,
            last_access_at: now,
            size_bytes: data.len() as u64,
            data,
        };
        self.put_cache(&entry)
    }

    /// Evict every history entry whose `timestamp` is strictly before
    /// `older_than`, removing both the primary row and its timestamp index
    /// entry. Returns the number of rows removed.
    ///
    /// Uses [`HISTORY_BY_TIMESTAMP_INDEX`] so pruning does not scan the full
    /// history table.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn evict_history_older_than(
        &mut self,
        older_than: DateTime<Utc>,
    ) -> Result<u64> {
        let txn = self.inner()?;
        let cutoff_ms = older_than.timestamp_millis();

        let mut index = match txn.open_table(HISTORY_BY_TIMESTAMP_INDEX) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(0),
            Err(e) => return Err(e.into()),
        };
        let mut primary = txn.open_table(HISTORY_TABLE)?;

        let mut to_evict: Vec<(i64, [u8; 16])> = Vec::new();
        for entry in index.range(..cutoff_ms)? {
            let (ts, key) = entry?;
            to_evict.push((ts.value(), *key.value()));
        }

        let mut removed: u64 = 0;
        for (ts, key) in to_evict {
            if primary.remove(&key)?.is_some() {
                let _ = index.remove(ts)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Evict every cache entry whose `last_access_at` is older than
    /// `older_than`. Returns the number of rows removed.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn evict_cache_older_than(
        &mut self,
        older_than: DateTime<Utc>,
    ) -> Result<u64> {
        let txn = self.inner()?;
        let mut table = txn.open_table(CACHE_TABLE)?;

        // Collect keys to evict first to avoid iterating and mutating at the
        // same time.
        let mut to_evict: Vec<[u8; 32]> = Vec::new();
        for entry in table.iter()? {
            let (k, v) = entry?;
            if v.value().last_access_at < older_than {
                to_evict.push(*k.value());
            }
        }

        let mut removed: u64 = 0;
        for k in &to_evict {
            if table.remove(k)?.is_some() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Commit the transaction, making all staged changes durable.
    ///
    /// # Errors
    ///
    /// Propagates [`StorageError::RedbCommit`].
    pub fn commit(mut self) -> Result<()> {
        let Some(inner) = self.inner.take() else {
            return Ok(());
        };
        inner.commit()?;
        Ok(())
    }
}

// We implement `Drop` (implicitly, via `Option::drop`) letting redb's own
// `Drop` roll back any uncommitted changes.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::types::{
        ColorLabel, GridPosition, HistoryEntry, LifecycleState, NotificationCenterItem,
        NotificationCenterState, WidgetSize,
    };

    fn new_store() -> StateStore {
        StateStore::open_in_memory("0.0-test").unwrap()
    }

    #[test]
    fn put_and_get_workspace() {
        let store = new_store();
        let ws = Workspace {
            id: Uuid::new_v4(),
            name: "Main".into(),
            ordinal: 1,
            wallpaper: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        {
            let mut w = store.write().unwrap();
            w.put_workspace(&ws).unwrap();
            w.commit().unwrap();
        }
        let r = store.read().unwrap();
        let back = r.get_workspace(ws.id).unwrap().unwrap();
        assert_eq!(back.name, ws.name);
    }

    #[test]
    fn file_tag_crud() {
        let store = new_store();
        let tag = FileTag {
            path: "C:/x.txt".into(),
            tags: vec!["one".into()],
            color_label: Some(ColorLabel::Blue),
            starred: false,
            updated_at: Utc::now(),
        };
        {
            let mut w = store.write().unwrap();
            w.put_file_tag(&tag).unwrap();
            w.commit().unwrap();
        }
        {
            let r = store.read().unwrap();
            assert!(r.get_file_tag("C:/x.txt").unwrap().is_some());
        }
        {
            let mut w = store.write().unwrap();
            assert!(w.delete_file_tag("C:/x.txt").unwrap());
            w.commit().unwrap();
        }
        let r = store.read().unwrap();
        assert!(r.get_file_tag("C:/x.txt").unwrap().is_none());
    }

    #[test]
    fn widget_crud_and_workspace_filter() {
        let store = new_store();
        let ws_id = Uuid::new_v4();
        let other_ws = Uuid::new_v4();
        let w1 = WidgetInstance {
            id: Uuid::new_v4(),
            widget_type: "weather".into(),
            workspace_id: ws_id,
            position: GridPosition { col: 0, row: 0 },
            size: WidgetSize::Small,
            lifecycle: LifecycleState::Active,
            config: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let w2 = WidgetInstance {
            id: Uuid::new_v4(),
            widget_type: "moon".into(),
            workspace_id: other_ws,
            ..w1.clone()
        };
        {
            let mut w = store.write().unwrap();
            w.put_widget(&w1).unwrap();
            w.put_widget(&w2).unwrap();
            w.commit().unwrap();
        }
        let r = store.read().unwrap();
        let for_ws = r.widgets_for_workspace(ws_id).unwrap();
        assert_eq!(for_ws.len(), 1);
        assert_eq!(for_ws[0].widget_type, "weather");
    }

    #[test]
    fn history_index_roundtrip() {
        let store = new_store();
        let base = Utc::now();
        for i in 0..5 {
            let entry = HistoryEntry {
                id: Uuid::new_v4(),
                timestamp: base + chrono::Duration::milliseconds(i),
                action_id: format!("act.{i}"),
                command_text: format!("cmd {i}"),
                target: None,
                reversible_until: None,
                reverse_command: None,
                metadata: vec![],
            };
            let mut w = store.write().unwrap();
            w.put_history(&entry).unwrap();
            w.commit().unwrap();
        }
        let r = store.read().unwrap();
        let recent = r.iter_history_recent(10).unwrap();
        assert_eq!(recent.len(), 5);
        // Newest first
        for pair in recent.windows(2) {
            assert!(pair[0].timestamp >= pair[1].timestamp);
        }
    }

    #[test]
    fn evict_history_older_than_removes_correct_rows() {
        let store = new_store();
        let cutoff = Utc::now();
        let old_ts = cutoff - chrono::Duration::hours(1);
        let fresh_ts = cutoff + chrono::Duration::hours(1);

        let mut old_ids = Vec::new();
        let mut fresh_ids = Vec::new();

        {
            let mut w = store.write().unwrap();
            for i in 0..3_u8 {
                let id = Uuid::new_v4();
                old_ids.push(id);
                w.put_history(&HistoryEntry {
                    id,
                    timestamp: old_ts + chrono::Duration::milliseconds(i64::from(i)),
                    action_id: format!("old.{i}"),
                    command_text: format!("old cmd {i}"),
                    target: None,
                    reversible_until: None,
                    reverse_command: None,
                    metadata: vec![],
                })
                .unwrap();
            }
            for i in 0..2_u8 {
                let id = Uuid::new_v4();
                fresh_ids.push(id);
                w.put_history(&HistoryEntry {
                    id,
                    timestamp: fresh_ts + chrono::Duration::milliseconds(i64::from(i)),
                    action_id: format!("fresh.{i}"),
                    command_text: format!("fresh cmd {i}"),
                    target: None,
                    reversible_until: None,
                    reverse_command: None,
                    metadata: vec![],
                })
                .unwrap();
            }
            w.commit().unwrap();
        }

        {
            let mut w = store.write().unwrap();
            let removed = w.evict_history_older_than(cutoff).unwrap();
            assert_eq!(removed, 3);
            w.commit().unwrap();
        }

        let r = store.read().unwrap();
        for id in &old_ids {
            assert!(
                r.get_history(*id).unwrap().is_none(),
                "expired history entry was not evicted"
            );
        }
        for id in &fresh_ids {
            assert!(
                r.get_history(*id).unwrap().is_some(),
                "fresh history entry should remain"
            );
        }
        assert_eq!(r.iter_history_recent(10).unwrap().len(), 2);
    }

    #[test]
    fn notification_center_persists_across_transactions() {
        let store = new_store();
        let state = NotificationCenterState {
            items: vec![NotificationCenterItem {
                id: "n1".into(),
                title: "Hello".into(),
                body: "World".into(),
                time_label: "09:00".into(),
                severity: 0,
            }],
        };
        {
            let mut w = store.write().unwrap();
            w.put_notification_center(&state).unwrap();
            w.commit().unwrap();
        }
        let loaded = store.read().unwrap().get_notification_center().unwrap();
        assert_eq!(loaded.as_ref(), Some(&state));
    }
}
