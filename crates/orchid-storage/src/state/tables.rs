//! redb table definitions used across the state store.
//!
//! Keys come in two flavours:
//!
//! * `&str` for paths, named singletons, and string-keyed lookups;
//! * `&[u8; 16]` for UUID-keyed tables. Storing the raw 16 bytes keeps keys
//!   compact and comparable. Use [`uuid_key`] / [`uuid_from_key`] to convert.
//!
//! A dedicated secondary index — [`HISTORY_BY_TIMESTAMP_INDEX`] — maps a
//! unix-millisecond timestamp (`i64`) to the UUID bytes of the corresponding
//! history entry, enabling ordered iteration without an expensive full table
//! scan.

use redb::TableDefinition;
use uuid::Uuid;

use crate::state::codec::Value;
use crate::state::types::{
    CacheEntry, FileTag, HistoryEntry, SchemaMeta, SessionState, WidgetInstance, Workspace,
};

/// Key under which the singleton [`SchemaMeta`] record lives in the meta table.
pub const META_KEY_CURRENT: &str = "current";

/// Key under which the singleton [`SessionState`] record lives.
pub const SESSION_KEY_CURRENT: &str = "current";

/// Singleton metadata table (schema version, timestamps, app version).
pub const META_TABLE: TableDefinition<&str, Value<SchemaMeta>> =
    TableDefinition::new("meta");

/// Primary history table, keyed by the raw 16 bytes of the entry UUID.
pub const HISTORY_TABLE: TableDefinition<&[u8; 16], Value<HistoryEntry>> =
    TableDefinition::new("history");

/// Secondary index: unix-millis timestamp -> history entry UUID bytes.
///
/// Because `i64` is an ordered redb key, iteration over this table yields
/// history entries in chronological order without scanning the primary table.
pub const HISTORY_BY_TIMESTAMP_INDEX: TableDefinition<i64, &[u8; 16]> =
    TableDefinition::new("history_by_timestamp");

/// Widget instances table, keyed by widget UUID.
pub const WIDGET_INSTANCES_TABLE: TableDefinition<&[u8; 16], Value<WidgetInstance>> =
    TableDefinition::new("widget_instances");

/// Workspaces table, keyed by workspace UUID.
pub const WORKSPACES_TABLE: TableDefinition<&[u8; 16], Value<Workspace>> =
    TableDefinition::new("workspaces");

/// File-tag table, keyed by canonical filesystem path.
pub const FILE_TAGS_TABLE: TableDefinition<&str, Value<FileTag>> =
    TableDefinition::new("file_tags");

/// Singleton table holding the last-saved [`SessionState`].
pub const SESSION_STATE_TABLE: TableDefinition<&str, Value<SessionState>> =
    TableDefinition::new("session_state");

/// Cache table, keyed by the 32-byte BLAKE3 hash in [`CacheEntry::key`].
pub const CACHE_TABLE: TableDefinition<&[u8; 32], Value<CacheEntry>> =
    TableDefinition::new("cache");

/// Convert a UUID into the fixed-size byte array used as a redb key.
///
/// # Examples
///
/// ```
/// use orchid_storage::state::tables::{uuid_key, uuid_from_key};
/// use uuid::Uuid;
///
/// let id = Uuid::nil();
/// let key = uuid_key(id);
/// assert_eq!(uuid_from_key(&key), id);
/// ```
#[must_use]
pub fn uuid_key(id: Uuid) -> [u8; 16] {
    *id.as_bytes()
}

/// Inverse of [`uuid_key`].
///
/// # Examples
///
/// ```
/// use orchid_storage::state::tables::{uuid_key, uuid_from_key};
/// use uuid::Uuid;
///
/// let id = Uuid::new_v4();
/// assert_eq!(uuid_from_key(&uuid_key(id)), id);
/// ```
#[must_use]
pub fn uuid_from_key(bytes: &[u8; 16]) -> Uuid {
    Uuid::from_bytes(*bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_key_roundtrip() {
        let id = Uuid::new_v4();
        assert_eq!(uuid_from_key(&uuid_key(id)), id);
    }
}
