//! Brute-force ANN over document-level embedding vectors.
//!
//! Linear scan is enough for Phase 5 DONE proofs and modest libraries;
//! swap for HNSW (`instant-distance` / `hnsw_rs`) when scale demands it.

use std::collections::HashMap;

use orchid_embed::cosine_similarity;

use crate::error::{Result, SearchError};

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
}
