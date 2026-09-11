//! Brute-force ANN over document-level embedding vectors.
//!
//! Linear scan is enough for Phase 5 DONE proofs and modest libraries;
//! swap for HNSW (`instant-distance` / `hnsw_rs`) when scale demands it.

use std::collections::HashMap;
use std::path::Path;

use orchid_embed::cosine_similarity;

use crate::error::{Result, SearchError};

/// On-disk ANN snapshot next to the Tantivy index (`ann.stub.v1`).
pub const ANN_SNAPSHOT_NAME: &str = "ann.stub.v1";
const ANN_MAGIC: &[u8; 8] = b"ORANN001";

/// In-memory path → L2-normalised document vector index.
#[derive(Debug, Default, Clone)]
pub struct AnnIndex {
    dims: usize,
    /// path → vector
    vectors: HashMap<String, Vec<f32>>,
}

impl AnnIndex {
    /// Create an empty index expecting `dims`-wide vectors.
    #[must_use]
    pub fn new(dims: usize) -> Self {
        Self {
            dims,
            vectors: HashMap::new(),
        }
    }

    /// Dimensionality this index was opened for.
    #[must_use]
    pub fn dims(&self) -> usize {
        self.dims
    }

    /// Number of indexed documents.
    #[must_use]
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Whether the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// Insert or replace a document vector.
    pub fn upsert(&mut self, path: impl Into<String>, vector: Vec<f32>) -> Result<()> {
        if vector.len() != self.dims {
            return Err(SearchError::Extraction {
                path: String::new(),
                reason: format!(
                    "ANN vector len {} != index dims {}",
                    vector.len(),
                    self.dims
                ),
            });
        }
        self.vectors.insert(path.into(), vector);
        Ok(())
    }

    /// Remove a path if present.
    pub fn remove(&mut self, path: &str) {
        self.vectors.remove(path);
    }

    /// Top-k nearest neighbours by cosine similarity (descending).
    #[must_use]
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(String, f32)> {
        if query.len() != self.dims || k == 0 {
            return Vec::new();
        }
        let mut scored: Vec<(String, f32)> = self
            .vectors
            .iter()
            .map(|(path, v)| (path.clone(), cosine_similarity(query, v)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        scored
    }

    /// Write path → vector pairs to `path` (little-endian, `ORANN001`).
    ///
    /// # Errors
    ///
    /// I/O failures.
    pub fn persist(&self, path: &Path) -> Result<()> {
        let mut buf = Vec::with_capacity(16 + self.vectors.len() * (8 + self.dims * 4));
        buf.extend_from_slice(ANN_MAGIC);
        buf.extend_from_slice(&(self.dims as u32).to_le_bytes());
        buf.extend_from_slice(&(self.vectors.len() as u32).to_le_bytes());
        for (key, vector) in &self.vectors {
            let raw = key.as_bytes();
            buf.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            buf.extend_from_slice(raw);
            for f in vector {
                buf.extend_from_slice(&f.to_le_bytes());
            }
        }
        std::fs::write(path, buf)?;
        Ok(())
    }

    /// Load a snapshot written by [`Self::persist`].
    ///
    /// # Errors
    ///
    /// I/O, truncated file, magic/dims mismatch.
    pub fn load(path: &Path, expected_dims: usize) -> Result<Self> {
        let data = std::fs::read(path)?;
        if data.len() < 16 || &data[0..8] != ANN_MAGIC {
            return Err(SearchError::Extraction {
                path: path.display().to_string(),
                reason: "ANN snapshot magic mismatch".into(),
            });
        }
        let dims = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        if dims != expected_dims {
            return Err(SearchError::Extraction {
                path: path.display().to_string(),
                reason: format!("ANN snapshot dims {dims} != {expected_dims}"),
            });
        }
        let count = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        let mut off = 16usize;
        let mut index = Self::new(dims);
        for _ in 0..count {
            if off + 4 > data.len() {
                return Err(SearchError::Extraction {
                    path: path.display().to_string(),
                    reason: "ANN snapshot truncated".into(),
                });
            }
            let klen = u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as usize;
            off += 4;
            if off + klen + dims * 4 > data.len() {
                return Err(SearchError::Extraction {
                    path: path.display().to_string(),
                    reason: "ANN snapshot truncated".into(),
                });
            }
            let key = String::from_utf8(data[off..off + klen].to_vec()).map_err(|e| {
                SearchError::Extraction {
                    path: path.display().to_string(),
                    reason: format!("ANN path utf8: {e}"),
                }
            })?;
            off += klen;
            let mut vector = Vec::with_capacity(dims);
            for _ in 0..dims {
                let bits = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
                vector.push(f32::from_bits(bits));
                off += 4;
            }
            index.upsert(key, vector)?;
        }
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persist_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(ANN_SNAPSHOT_NAME);
        let mut ann = AnnIndex::new(2);
        ann.upsert("local:/a", vec![1.0, 0.0]).unwrap();
        ann.upsert("local:/b", vec![0.0, 1.0]).unwrap();
        ann.persist(&path).unwrap();
        let back = AnnIndex::load(&path, 2).unwrap();
        assert_eq!(back.len(), 2);
        let hits = back.search(&[1.0, 0.0], 1);
        assert_eq!(hits[0].0, "local:/a");
    }
}
