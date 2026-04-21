//! High-level file dedup engine.
//!
//! Takes a file on disk, streams it through [`crate::Chunker`], stores each
//! chunk in a [`ChunkStore`], and records a compact [`FileManifest`] that
//! identifies the file by the ordered list of chunk hashes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bincode::{Decode, Encode};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::content::chunker::{Chunker, ChunkerConfig};
use crate::content::hash::{hash_file, hex};
use crate::content::store::ChunkStore;
use crate::error::{CryptoError, Result};

/// Reference to a chunk inside a [`FileManifest`].
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ChunkRef {
    /// 32-byte BLAKE3 hash of the chunk bytes.
    pub hash: [u8; 32],
    /// Byte offset at which this chunk starts in the reconstructed file.
    pub offset: u64,
    /// Length of the chunk in bytes.
    pub length: u32,
}

/// Compact description of a file as an ordered list of chunk references.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct FileManifest {
    /// Manifest identifier.
    #[bincode(with_serde)]
    pub id: Uuid,
    /// Original on-disk path at ingest time, if the caller passed one.
    pub original_path: Option<String>,
    /// Total size in bytes.
    pub total_size: u64,
    /// BLAKE3 of the entire plaintext file (not of the concatenation of
    /// chunk hashes).
    pub content_hash: [u8; 32],
    /// Ordered list of chunk references.
    pub chunks: Vec<ChunkRef>,
    /// When the manifest was created.
    #[bincode(with_serde)]
    pub created_at: chrono::DateTime<Utc>,
    /// Chunker configuration used to ingest the file. Reconstruction does
    /// not depend on it, but it's recorded for diagnostics.
    pub chunker_config: ChunkerConfig,
}

/// Per-manifest storage statistics.
#[derive(Debug, Clone, Copy)]
pub struct DedupStats {
    /// Logical size (equal to [`FileManifest::total_size`]).
    pub total_logical_bytes: u64,
    /// Physical size — sum of unique chunk sizes.
    pub total_physical_bytes: u64,
    /// Total chunk count in the manifest.
    pub chunk_count: u64,
    /// Number of distinct chunk hashes.
    pub unique_chunks: u64,
}

/// Ingest / reconstruct / release pipeline.
pub struct Deduplicator {
    store: Arc<ChunkStore>,
    chunker: Chunker,
    config: ChunkerConfig,
}

impl std::fmt::Debug for Deduplicator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Deduplicator")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Deduplicator {
    /// Construct a deduplicator bound to a chunk store.
    #[must_use]
    pub fn new(store: Arc<ChunkStore>, config: ChunkerConfig) -> Self {
        Self {
            store,
            chunker: Chunker::new(config),
            config,
        }
    }

    /// Ingest a file: chunk, store each chunk (bumping refcount on dupes),
    /// and return a manifest describing the content layout.
    ///
    /// # Errors
    ///
    /// Propagates I/O, chunker, and chunk-store errors.
    pub async fn ingest_file(&self, path: &Path) -> Result<FileManifest> {
        let content_hash = hash_file(path).await?;
        let meta = fs::metadata(path).await?;
        let total_size = meta.len();

        let store = Arc::clone(&self.store);
        let mut refs: Vec<ChunkRef> = Vec::new();
        let chunks_meta = self
            .chunker
            .chunk_file(path, |chunk, data| {
                let store = Arc::clone(&store);
                async move {
                    store.put_with_hash(chunk.hash, data.as_slice()).await?;
                    Ok(())
                }
            })
            .await?;
        for c in chunks_meta {
            refs.push(ChunkRef {
                hash: c.hash,
                offset: c.offset,
                length: c.length,
            });
        }

        Ok(FileManifest {
            id: crate::random::random_uuid(),
            original_path: path.to_str().map(ToOwned::to_owned),
            total_size,
            content_hash,
            chunks: refs,
            created_at: Utc::now(),
            chunker_config: self.config,
        })
    }

    /// Reconstruct a file described by `manifest` into `output`. Writes to
    /// a sibling tmp file and renames only after verifying the reconstructed
    /// content's BLAKE3 matches `manifest.content_hash`.
    ///
    /// # Errors
    ///
    /// Propagates chunk-store and I/O errors. Returns
    /// [`CryptoError::ChunkIntegrity`] if the reconstructed file's hash
    /// does not match the manifest (tmp file is deleted in that case).
    pub async fn reconstruct_to(&self, manifest: &FileManifest, output: &Path) -> Result<()> {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).await?;
        }
        let tmp: PathBuf = output.with_extension("reconstructing");
        let mut file = fs::File::create(&tmp).await?;

        let mut hasher = blake3::Hasher::new();
        for chunk in &manifest.chunks {
            let bytes = self.store.get(&chunk.hash).await?;
            if bytes.as_slice().len() as u32 != chunk.length {
                let _ = fs::remove_file(&tmp).await;
                return Err(CryptoError::ChunkIntegrity {
                    expected: hex(&chunk.hash),
                    actual: format!("length {}", bytes.as_slice().len()),
                });
            }
            hasher.update(bytes.as_slice());
            file.write_all(bytes.as_slice()).await?;
        }
        file.flush().await?;
        file.sync_all().await?;
        drop(file);

        let reconstructed: [u8; 32] = *hasher.finalize().as_bytes();
        if reconstructed != manifest.content_hash {
            let _ = fs::remove_file(&tmp).await;
            return Err(CryptoError::ChunkIntegrity {
                expected: hex(&manifest.content_hash),
                actual: hex(&reconstructed),
            });
        }

        fs::rename(&tmp, output).await?;
        Ok(())
    }

    /// Release every chunk referenced by `manifest`.
    ///
    /// # Errors
    ///
    /// Propagates chunk-store errors.
    pub async fn release(&self, manifest: &FileManifest) -> Result<()> {
        for c in &manifest.chunks {
            // release can return ChunkNotFound if the caller tries to release
            // twice; propagate for correctness.
            let _ = self.store.release(&c.hash).await?;
        }
        Ok(())
    }

    /// Compute logical vs physical storage statistics for a manifest.
    ///
    /// "Physical" here counts unique chunk sizes by summing the lengths of
    /// each chunk the first time its hash is seen.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation; the signature reserves
    /// room for cases that need to query the chunk store.
    pub async fn stats(&self, manifest: &FileManifest) -> Result<DedupStats> {
        let mut unique: HashSet<[u8; 32]> = HashSet::new();
        let mut physical: u64 = 0;
        for c in &manifest.chunks {
            if unique.insert(c.hash) {
                physical = physical.saturating_add(c.length as u64);
            }
        }
        Ok(DedupStats {
            total_logical_bytes: manifest.total_size,
            total_physical_bytes: physical,
            chunk_count: manifest.chunks.len() as u64,
            unique_chunks: unique.len() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::hash::hash_bytes;

    #[test]
    fn stats_counts_unique_chunks() {
        // Hand-craft a manifest with one duplicate hash.
        let h1 = hash_bytes(b"aaaa");
        let h2 = hash_bytes(b"bbbb");
        let m = FileManifest {
            id: Uuid::nil(),
            original_path: None,
            total_size: 12,
            content_hash: [0u8; 32],
            chunks: vec![
                ChunkRef {
                    hash: h1,
                    offset: 0,
                    length: 4,
                },
                ChunkRef {
                    hash: h2,
                    offset: 4,
                    length: 4,
                },
                ChunkRef {
                    hash: h1,
                    offset: 8,
                    length: 4,
                },
            ],
            created_at: Utc::now(),
            chunker_config: ChunkerConfig::default(),
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let d = Deduplicator {
            store: Arc::new(
                ChunkStore::new(
                    tempfile::tempdir().unwrap().path().join("chunks"),
                    Arc::new(orchid_storage::StateStore::open_in_memory("0").unwrap()),
                )
                .unwrap(),
            ),
            chunker: Chunker::new(ChunkerConfig::default()),
            config: ChunkerConfig::default(),
        };
        let s = rt.block_on(d.stats(&m)).unwrap();
        assert_eq!(s.chunk_count, 3);
        assert_eq!(s.unique_chunks, 2);
        assert_eq!(s.total_physical_bytes, 8);
    }
}
