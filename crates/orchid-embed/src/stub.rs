//! Deterministic stub embedder with a small synonym map.
//!
//! Tokens that share a concept (e.g. `canine` / `dog`) land in the same
//! dense dimensions so ANN can retrieve paraphrases that BM25 misses.

use crate::concepts::{fnv1a64, fold_concept, l2_normalize};
use crate::{EmbedError, Embedder, Result};

/// Default stub model id (recorded in Embedding region metadata).
pub const STUB_MODEL_ID: &str = "orchid.stub.synonym.v1";

/// Vector width for [`StubEmbedder`].
pub const STUB_DIMS: usize = 64;

/// Deterministic bag-of-concepts embedder.
#[derive(Debug, Clone, Copy, Default)]
pub struct StubEmbedder;

impl StubEmbedder {
    /// Build with the built-in synonym table.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Embedder for StubEmbedder {
    fn model_id(&self) -> &str {
        STUB_MODEL_ID
    }

    fn dimensions(&self) -> usize {
        STUB_DIMS
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut v = vec![0.0f32; STUB_DIMS];
        let lower = text.to_ascii_lowercase();
        let mut any = false;
        for raw in lower.split(|c: char| !c.is_ascii_alphanumeric()) {
            if raw.is_empty() {
                continue;
            }
            any = true;
            let concept = fold_concept(raw);
            let h = fnv1a64(concept.as_bytes());
            let i0 = (h as usize) % STUB_DIMS;
            let i1 = ((h >> 32) as usize) % STUB_DIMS;
            v[i0] += 1.0;
            v[i1] += 0.5;
        }
        if !any {
            return Err(EmbedError::Failed("empty text".into()));
        }
        l2_normalize(&mut v);
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cosine_similarity;

    #[test]
    fn synonyms_are_closer_than_unrelated() {
        let e = StubEmbedder::new();
        let dog = e.embed("friendly dog plays outside").unwrap();
        let canine = e.embed("canine companion").unwrap();
        let car = e.embed("red automobile on highway").unwrap();
        assert!(cosine_similarity(&dog, &canine) > cosine_similarity(&dog, &car));
    }
}
