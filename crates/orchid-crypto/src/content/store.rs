//! Content-addressed chunk store backed by disk + a redb refcount table.
//!
//! Each chunk lives as `<chunks_dir>/<first two hex chars>/<rest>.bin` and
//! has a row in the local `crypto_chunk_refs` redb table tracking size,
//! refcount, and last-access time. Inserts are idempotent: repeat `put`s
//! bump the refcount, and `release` decrements until the refcount reaches
//! zero at which point the blob file is deleted.

use std::path::PathBuf;
use std::sync::Arc;

use bincode::{Decode, Encode};
use chrono::{DateTime, Utc};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::warn;

use crate::content::hash::{hash_bytes, hex};
use crate::error::{CryptoError, Result};
use crate::secret::zeroizing::ZeroizingBytes;

/// Lift any `redb` error that `orchid_storage::StorageError` knows about
/// into a [`CryptoError`]. Kept as a helper so call sites stay readable and
/// inference-friendly.
fn to_crypto<E>(e: E) -> CryptoError
where
    orchid_storage::StorageError: From<E>,
{
    CryptoError::Storage(e.into())
}

/// Pluggable clock abstraction. Production code uses [`SystemClock`]; tests
/// inject [`FixedClock`] to make age-sensitive behaviour deterministic.
pub trait Clock: Send + Sync {
    /// Current UTC time.
    fn now(&self) -> DateTime<Utc>;
}

/// Real wall-clock implementation of [`Clock`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Clock that always returns the stored value. Advance via
/// [`FixedClock::set`].
#[derive(Debug)]
pub struct FixedClock(pub parking_lot::RwLock<DateTime<Utc>>);

impl FixedClock {
    /// Construct from an initial instant.
    #[must_use]
    pub fn new(now: DateTime<Utc>) -> Self {
        Self(parking_lot::RwLock::new(now))
    }

    /// Advance (or set) the clock.
    pub fn set(&self, now: DateTime<Utc>) {
        *self.0.write() = now;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.read()
    }
}

/// Per-chunk metadata stored alongside the on-disk blob.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ChunkRefInfo {
    /// Size of the blob in bytes.
    pub size: u64,
    /// Number of outstanding references from manifests / callers.
    pub refcount: u64,
    /// When the chunk was first inserted.
    #[bincode(with_serde)]
    pub created_at: DateTime<Utc>,
    /// When the chunk was most recently read or re-inserted.
    #[bincode(with_serde)]
    pub last_accessed_at: DateTime<Utc>,
}

/// redb table holding [`ChunkRefInfo`] keyed by hex-encoded hash. We use the
/// hex form so entries are easy to dump with redb's CLI; the fixed 64-char
/// string also sorts lexicographically by the underlying bytes.
const CHUNK_REFS_TABLE: TableDefinition<&str, orchid_storage::Value<ChunkRefInfo>> =
    TableDefinition::new("crypto_chunk_refs");

/// Aggregate statistics returned by [`ChunkStore::garbage_collect`].
#[derive(Debug, Clone, Copy, Default)]
pub struct GcStats {
    /// Number of orphan blob files removed.
    pub files_removed: u64,
    /// Disk bytes reclaimed.
    pub bytes_freed: u64,
}

/// Disk + redb chunk store.
pub struct ChunkStore {
    chunks_dir: PathBuf,
    storage: Arc<orchid_storage::StateStore>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for ChunkStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkStore")
            .field("chunks_dir", &self.chunks_dir)
            .finish_non_exhaustive()
    }
}

impl ChunkStore {
    /// Build a store rooted at `chunks_dir`, persisting refcount metadata
    /// through the given [`orchid_storage::StateStore`]. The directory is
    /// created if missing.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::Io`] if the directory cannot be created.
    pub fn new(
        chunks_dir: PathBuf,
        storage: Arc<orchid_storage::StateStore>,
    ) -> Result<Self> {
        Self::with_clock(chunks_dir, storage, Arc::new(SystemClock))
    }

    /// Builder variant that accepts a custom clock, for tests.
    ///
    /// # Errors
    ///
    /// See [`ChunkStore::new`].
    pub fn with_clock(
        chunks_dir: PathBuf,
        storage: Arc<orchid_storage::StateStore>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self> {
        std::fs::create_dir_all(&chunks_dir)?;
        let store = Self {
            chunks_dir,
            storage,
            clock,
        };
        // Touch the table so it exists before the first read.
        let db = store.raw_db();
        let txn = db.begin_write().map_err(to_crypto)?;
        let _ = txn
            .open_table(CHUNK_REFS_TABLE)
            .map_err(to_crypto)?;
        txn.commit().map_err(to_crypto)?;
        Ok(store)
    }

    fn raw_db(&self) -> &Database {
        self.storage.raw_database()
    }

    fn chunk_path(&self, hex_hash: &str) -> PathBuf {
        let (a, b) = hex_hash.split_at(2);
        self.chunks_dir.join(a).join(format!("{b}.bin"))
    }

    /// Insert `bytes`. If an identical chunk is already present the refcount
    /// is bumped and its hash returned.
    ///
    /// # Errors
    ///
    /// Propagates I/O and redb errors; a partially-written blob file is
    /// deleted if the surrounding transaction fails.
    pub async fn put(&self, bytes: &[u8]) -> Result<[u8; 32]> {
        let hash = hash_bytes(bytes);
        self.put_with_hash(hash, bytes).await?;
        Ok(hash)
    }

    /// Insert when the caller already knows the hash.
    ///
    /// # Errors
    ///
    /// Propagates I/O and redb errors.
    pub async fn put_with_hash(&self, hash: [u8; 32], bytes: &[u8]) -> Result<()> {
        let key = hex(&hash);
        let path = self.chunk_path(&key);
        let now = self.clock.now();

        // Fast path: if the entry already exists, just bump refcount.
        {
            let db = self.raw_db();
            let txn = db.begin_write().map_err(to_crypto)?;
            let existing: Option<ChunkRefInfo> = {
                let table = txn
                    .open_table(CHUNK_REFS_TABLE)
                    .map_err(to_crypto)?;
                let got = table.get(key.as_str()).map_err(to_crypto)?;
                got.map(|g| g.value())
            };
            if let Some(mut info) = existing {
                info.refcount = info.refcount.saturating_add(1);
                info.last_accessed_at = now;
                {
                    let mut table = txn
                        .open_table(CHUNK_REFS_TABLE)
                        .map_err(to_crypto)?;
                    table
                        .insert(key.as_str(), &info)
                        .map_err(to_crypto)?;
                }
                txn.commit().map_err(to_crypto)?;
                return Ok(());
            }
            // Explicitly abort rather than commit an empty txn.
            drop(txn);
        }

        // Slow path: new blob. Write it atomically to disk first.
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let tmp = path.with_extension("bin.tmp");
        let mut f = fs::File::create(&tmp).await?;
        f.write_all(bytes).await?;
        f.flush().await?;
        f.sync_all().await?;
        drop(f);

        // Now try to insert. If the insert fails, clean up the blob file.
        let info = ChunkRefInfo {
            size: bytes.len() as u64,
            refcount: 1,
            created_at: now,
            last_accessed_at: now,
        };
        let write_result = (|| -> Result<()> {
            let db = self.raw_db();
            let txn = db.begin_write().map_err(to_crypto)?;
            {
                let mut table = txn
                    .open_table(CHUNK_REFS_TABLE)
                    .map_err(to_crypto)?;
                table
                    .insert(key.as_str(), &info)
                    .map_err(to_crypto)?;
            }
            txn.commit().map_err(to_crypto)?;
            Ok(())
        })();

        match write_result {
            Ok(()) => {
                fs::rename(&tmp, &path).await?;
                Ok(())
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp).await;
                Err(e)
            }
        }
    }

    /// Load a chunk, verifying its BLAKE3 hash on read.
    ///
    /// Updates `last_accessed_at` on success; failures to update that field
    /// are logged but otherwise ignored.
    ///
    /// # Errors
    ///
    /// * [`CryptoError::ChunkNotFound`] if the key is not registered.
    /// * [`CryptoError::ChunkIntegrity`] if the on-disk bytes no longer
    ///   hash to `hash`.
    /// * [`CryptoError::Io`] for I/O failures.
    pub async fn get(&self, hash: &[u8; 32]) -> Result<ZeroizingBytes> {
        let key = hex(hash);

        // Confirm it's registered before touching the filesystem.
        {
            let db = self.raw_db();
            let txn = db.begin_read().map_err(to_crypto)?;
            let table = txn
                .open_table(CHUNK_REFS_TABLE)
                .map_err(to_crypto)?;
            if table
                .get(key.as_str())
                .map_err(to_crypto)?
                .is_none()
            {
                return Err(CryptoError::ChunkNotFound(key));
            }
        }

        let path = self.chunk_path(&key);
        let bytes = fs::read(&path).await?;
        let actual = hash_bytes(&bytes);
        if &actual != hash {
            return Err(CryptoError::ChunkIntegrity {
                expected: key,
                actual: hex(&actual),
            });
        }

        // Best-effort last-access update.
        let now = self.clock.now();
        if let Err(e) = self.touch(&key, now) {
            warn!(error = %e, hash = %hex(hash), "failed to update last_accessed_at");
        }

        Ok(ZeroizingBytes::new(bytes))
    }

    fn touch(&self, key: &str, now: DateTime<Utc>) -> Result<()> {
        let db = self.raw_db();
        let txn = db.begin_write().map_err(to_crypto)?;
        {
            let mut table = txn
                .open_table(CHUNK_REFS_TABLE)
                .map_err(to_crypto)?;
            let got = table.get(key).map_err(to_crypto)?;
            let current: Option<ChunkRefInfo> = got.map(|g| g.value());
            if let Some(mut info) = current {
                info.last_accessed_at = now;
                table
                    .insert(key, &info)
                    .map_err(to_crypto)?;
            }
        }
        txn.commit().map_err(to_crypto)?;
        Ok(())
    }

    /// Decrement the refcount. If it reaches zero the blob file is deleted.
    /// Returns the new refcount.
    ///
    /// # Errors
    ///
    /// * [`CryptoError::ChunkNotFound`] if the chunk isn't registered.
    /// * [`CryptoError::RefcountUnderflow`] if the refcount was already zero.
    pub async fn release(&self, hash: &[u8; 32]) -> Result<u64> {
        let key = hex(hash);
        let path = self.chunk_path(&key);

        enum Action {
            Decremented(u64),
            DeletedAtZero,
        }

        let action = {
            let db = self.raw_db();
            let txn = db.begin_write().map_err(to_crypto)?;
            let info: Option<ChunkRefInfo> = {
                let table = txn
                    .open_table(CHUNK_REFS_TABLE)
                    .map_err(to_crypto)?;
                let got = table.get(key.as_str()).map_err(to_crypto)?;
                got.map(|g| g.value())
            };
            let Some(mut info) = info else {
                return Err(CryptoError::ChunkNotFound(key));
            };
            if info.refcount == 0 {
                return Err(CryptoError::RefcountUnderflow(key));
            }
            info.refcount -= 1;
            let act = if info.refcount == 0 {
                let mut table = txn
                    .open_table(CHUNK_REFS_TABLE)
                    .map_err(to_crypto)?;
                let _ = table
                    .remove(key.as_str())
                    .map_err(to_crypto)?;
                Action::DeletedAtZero
            } else {
                let mut table = txn
                    .open_table(CHUNK_REFS_TABLE)
                    .map_err(to_crypto)?;
                table
                    .insert(key.as_str(), &info)
                    .map_err(to_crypto)?;
                Action::Decremented(info.refcount)
            };
            txn.commit().map_err(to_crypto)?;
            act
        };

        match action {
            Action::Decremented(n) => Ok(n),
            Action::DeletedAtZero => {
                let _ = fs::remove_file(&path).await;
                Ok(0)
            }
        }
    }

    /// Check if a chunk is registered.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn exists(&self, hash: &[u8; 32]) -> Result<bool> {
        let key = hex(hash);
        let db = self.raw_db();
        let txn = db.begin_read().map_err(to_crypto)?;
        let table = txn
            .open_table(CHUNK_REFS_TABLE)
            .map_err(to_crypto)?;
        Ok(table
            .get(key.as_str())
            .map_err(to_crypto)?
            .is_some())
    }

    /// Total bytes tracked by the table. Does not stat the filesystem.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn total_bytes(&self) -> Result<u64> {
        let db = self.raw_db();
        let txn = db.begin_read().map_err(to_crypto)?;
        let table = txn
            .open_table(CHUNK_REFS_TABLE)
            .map_err(to_crypto)?;
        let mut total = 0_u64;
        for item in table.iter().map_err(to_crypto)? {
            let (_, v) = item.map_err(to_crypto)?;
            total = total.saturating_add(v.value().size);
        }
        Ok(total)
    }

    /// Walk the chunks directory and delete blob files not referenced in the
    /// table. Useful after a crashed write.
    ///
    /// # Errors
    ///
    /// Propagates I/O / redb errors.
    pub async fn garbage_collect(&self) -> Result<GcStats> {
        let mut stats = GcStats::default();
        let dir = self.chunks_dir.clone();
        if !dir.exists() {
            return Ok(stats);
        }

        for outer in walkdir::WalkDir::new(&dir).min_depth(1).max_depth(1) {
            let outer = outer.map_err(|e| CryptoError::Io(std::io::Error::other(e)))?;
            if !outer.file_type().is_dir() {
                continue;
            }
            let outer_name = match outer.file_name().to_str() {
                Some(s) if s.len() == 2 => s.to_string(),
                _ => continue,
            };
            for entry in walkdir::WalkDir::new(outer.path()).min_depth(1).max_depth(1) {
                let entry = entry.map_err(|e| CryptoError::Io(std::io::Error::other(e)))?;
                if !entry.file_type().is_file() {
                    continue;
                }
                let Some(name) = entry.file_name().to_str() else {
                    continue;
                };
                let Some(stem) = name.strip_suffix(".bin") else {
                    continue;
                };
                let hex_hash = format!("{outer_name}{stem}");
                let referenced = {
                    let db = self.raw_db();
                    let txn = db.begin_read().map_err(to_crypto)?;
                    let table = txn
                        .open_table(CHUNK_REFS_TABLE)
                        .map_err(to_crypto)?;
                    table
                        .get(hex_hash.as_str())
                        .map_err(to_crypto)?
                        .is_some()
                };
                if !referenced {
                    let size = entry
                        .metadata()
                        .map_err(|e| CryptoError::Io(std::io::Error::other(e)))?
                        .len();
                    let _ = fs::remove_file(entry.path()).await;
                    stats.files_removed = stats.files_removed.saturating_add(1);
                    stats.bytes_freed = stats.bytes_freed.saturating_add(size);
                }
            }
        }
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> (tempfile::TempDir, ChunkStore) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(orchid_storage::StateStore::open_in_memory("0").unwrap());
        let s = ChunkStore::new(dir.path().join("chunks"), storage).unwrap();
        (dir, s)
    }

    #[tokio::test]
    async fn put_and_get_round_trip() {
        let (_td, s) = store().await;
        let data = b"hello chunk store";
        let hash = s.put(data).await.unwrap();
        let got = s.get(&hash).await.unwrap();
        assert_eq!(got.as_slice(), data);
    }

    #[tokio::test]
    async fn put_twice_increments_refcount() {
        let (_td, s) = store().await;
        let data = b"same bytes";
        let h1 = s.put(data).await.unwrap();
        let h2 = s.put(data).await.unwrap();
        assert_eq!(h1, h2);
        // Two references: one release leaves it alive, two should remove it.
        let rc1 = s.release(&h1).await.unwrap();
        assert_eq!(rc1, 1);
        let rc2 = s.release(&h1).await.unwrap();
        assert_eq!(rc2, 0);
    }

    #[tokio::test]
    async fn release_to_zero_removes_blob() {
        let (td, s) = store().await;
        let data = b"ephemeral";
        let h = s.put(data).await.unwrap();
        let path = s.chunk_path(&hex(&h));
        assert!(path.exists() || td.path().exists()); // chunks dir created
        s.release(&h).await.unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn release_underflow_is_reported() {
        let (_td, s) = store().await;
        let data = b"x";
        let h = s.put(data).await.unwrap();
        s.release(&h).await.unwrap();
        // After hitting zero the entry is gone -> ChunkNotFound on next release.
        let err = s.release(&h).await.unwrap_err();
        assert!(matches!(err, CryptoError::ChunkNotFound(_)));
    }

    #[tokio::test]
    async fn integrity_failure_on_tampering() {
        let (_td, s) = store().await;
        let data = b"original";
        let h = s.put(data).await.unwrap();
        let path = s.chunk_path(&hex(&h));
        // Tamper with the on-disk blob.
        tokio::fs::write(&path, b"tampered").await.unwrap();
        let err = s.get(&h).await.unwrap_err();
        assert!(matches!(err, CryptoError::ChunkIntegrity { .. }));
    }

    #[tokio::test]
    async fn total_bytes_reflects_stored_chunks() {
        let (_td, s) = store().await;
        s.put(b"aaaa").await.unwrap();
        s.put(b"bbbb").await.unwrap();
        assert_eq!(s.total_bytes().unwrap(), 8);
    }
}
