//! Schema versioning and migration engine.
//!
//! Migrations are declared in a static list returned by
//! [`available_migrations`]. On open the engine reads the on-disk schema
//! version from the `meta` table, compares it to [`CURRENT_SCHEMA_VERSION`],
//! and runs every applicable migration in ascending order inside a single
//! write transaction per step.
//!
//! **Adding a migration.** Append a new entry with `from_version = N, to_version
//! = N+1` to the internal `AVAILABLE` list, bump [`CURRENT_SCHEMA_VERSION`],
//! and write the migration body as a plain function taking
//! `&redb::WriteTransaction`.

use chrono::Utc;
use redb::{Database, WriteTransaction};

use crate::error::{Result, StorageError};
use crate::state::tables::{META_KEY_CURRENT, META_TABLE};
use crate::state::types::SchemaMeta;

/// The current supported schema version.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// A single migration step.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Version this migration starts from.
    pub from_version: u32,
    /// Version this migration produces.
    pub to_version: u32,
    /// Short human-readable description.
    pub description: &'static str,
    /// Function pointer that performs the migration inside the given
    /// write transaction. The transaction is committed by the engine.
    pub run: fn(&WriteTransaction) -> Result<()>,
}

/// Ordered list of migrations. Must be non-decreasing in `from_version` and
/// contiguous (every `to_version` becomes the next `from_version`).
static AVAILABLE: &[Migration] = &[
    // v0 -> v1: the initial schema. The migration body is a no-op because a
    // fresh database is already at v1; this entry exists so that an engine
    // opening a hypothetical v0 database (created by tests or by a future
    // downgrade-handling tool) would advance it cleanly.
    Migration {
        from_version: 0,
        to_version: 1,
        description: "initial schema",
        run: |_txn| Ok(()),
    },
];

/// Returns the registered migration list.
///
/// # Examples
///
/// ```
/// use orchid_storage::state::migrations::available_migrations;
/// assert!(!available_migrations().is_empty());
/// ```
#[must_use]
pub fn available_migrations() -> &'static [Migration] {
    AVAILABLE
}

/// Walk `current -> CURRENT_SCHEMA_VERSION` applying migrations in sequence.
///
/// Returns the new version on success (always equal to
/// [`CURRENT_SCHEMA_VERSION`]).
///
/// # Errors
///
/// * [`StorageError::UnsupportedSchemaVersion`] if the DB is newer than we
///   know about.
/// * [`StorageError::MigrationFailed`] if a migration body returns an error
///   or a required step is missing from the registered list.
pub fn migrate(db: &Database, current: u32) -> Result<u32> {
    if current > CURRENT_SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSchemaVersion {
            found: current,
            supported_max: CURRENT_SCHEMA_VERSION,
        });
    }

    let mut version = current;
    while version < CURRENT_SCHEMA_VERSION {
        let step = AVAILABLE
            .iter()
            .find(|m| m.from_version == version)
            .ok_or_else(|| StorageError::MigrationFailed {
                from: version,
                to: version + 1,
                reason: format!("no migration registered from v{version}"),
            })?;

        let txn = db.begin_write()?;
        (step.run)(&txn).map_err(|e| StorageError::MigrationFailed {
            from: step.from_version,
            to: step.to_version,
            reason: e.to_string(),
        })?;
        txn.commit()?;

        tracing::info!(
            from = step.from_version,
            to = step.to_version,
            description = step.description,
            "applied migration"
        );
        version = step.to_version;
    }
    Ok(version)
}

/// Read the current [`SchemaMeta`] from the DB, or return `None` if the meta
/// table has no `current` row yet (fresh database).
///
/// # Errors
///
/// Bubbles up transaction / table / storage errors from redb.
pub fn read_schema_meta(db: &Database) -> Result<Option<SchemaMeta>> {
    let txn = db.begin_read()?;
    // Opening a table that does not exist in a read transaction is an error,
    // which here simply means "the database is empty". Map that to `None`.
    let table = match txn.open_table(META_TABLE) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let Some(access) = table.get(META_KEY_CURRENT)? else {
        return Ok(None);
    };
    Ok(Some(access.value()))
}

/// Overwrite the singleton [`SchemaMeta`] record. Used both on first-time DB
/// initialisation and to refresh `last_opened_at` / `orchid_version` on every
/// open.
///
/// # Errors
///
/// Bubbles up transaction / table / commit errors from redb.
pub fn write_schema_meta(db: &Database, meta: &SchemaMeta) -> Result<()> {
    let txn = db.begin_write()?;
    {
        let mut table = txn.open_table(META_TABLE)?;
        table.insert(META_KEY_CURRENT, meta)?;
    }
    txn.commit()?;
    Ok(())
}

/// Initialise or refresh the [`SchemaMeta`] record. Returns the version this
/// database is now at (always [`CURRENT_SCHEMA_VERSION`] on success).
///
/// # Errors
///
/// Propagates the same errors as [`migrate`], [`read_schema_meta`], and
/// [`write_schema_meta`].
pub fn initialise(db: &Database, orchid_version: &str) -> Result<u32> {
    match read_schema_meta(db)? {
        None => {
            // Fresh DB: jump straight to the current schema version.
            let now = Utc::now();
            let meta = SchemaMeta {
                version: CURRENT_SCHEMA_VERSION,
                created_at: now,
                last_opened_at: now,
                orchid_version: orchid_version.to_string(),
            };
            write_schema_meta(db, &meta)?;
            Ok(CURRENT_SCHEMA_VERSION)
        }
        Some(mut meta) => {
            let version = migrate(db, meta.version)?;
            meta.version = version;
            meta.last_opened_at = Utc::now();
            meta.orchid_version = orchid_version.to_string();
            write_schema_meta(db, &meta)?;
            Ok(version)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_db() -> Database {
        Database::builder()
            .create_with_backend(redb::backends::InMemoryBackend::new())
            .unwrap()
    }

    #[test]
    fn migrate_is_noop_for_current_version() {
        let db = new_db();
        // No meta written yet; migrate() should accept and do nothing.
        let v = migrate(&db, CURRENT_SCHEMA_VERSION).unwrap();
        assert_eq!(v, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn migrate_rejects_future_versions() {
        let db = new_db();
        let err = migrate(&db, CURRENT_SCHEMA_VERSION + 5).unwrap_err();
        assert!(matches!(err, StorageError::UnsupportedSchemaVersion { .. }));
    }

    #[test]
    fn initialise_creates_schema_meta_on_fresh_db() {
        let db = new_db();
        assert!(read_schema_meta(&db).unwrap().is_none());
        initialise(&db, "test-0.0").unwrap();
        let meta = read_schema_meta(&db).unwrap().unwrap();
        assert_eq!(meta.version, CURRENT_SCHEMA_VERSION);
        assert_eq!(meta.orchid_version, "test-0.0");
    }

    #[test]
    fn initialise_refreshes_last_opened_at() {
        let db = new_db();
        initialise(&db, "0.1").unwrap();
        let first = read_schema_meta(&db).unwrap().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        initialise(&db, "0.2").unwrap();
        let second = read_schema_meta(&db).unwrap().unwrap();
        assert!(second.last_opened_at >= first.last_opened_at);
        assert_eq!(second.orchid_version, "0.2");
    }
}
