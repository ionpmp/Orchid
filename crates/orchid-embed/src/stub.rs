//! Deterministic stub embedder with a small synonym map.
//!
//! Tokens that share a concept (e.g. `canine` / `dog`) land in the same
//! dense dimensions so ANN can retrieve paraphrases that BM25 misses.

use std::collections::HashMap;

use crate::{EmbedError, Embedder, Result};

/// Default stub model id (recorded in Embedding region metadata).
pub const STUB_MODEL_ID: &str = "orchid.stub.synonym.v1";

/// Vector width for [`StubEmbedder`].
pub const STUB_DIMS: usize = 64;

/// Deterministic bag-of-concepts embedder.
#[derive(Debug, Clone)]
pub struct StubEmbedder {
    synonyms: HashMap<&'static str, &'static str>,
}

impl Default for StubEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl StubEmbedder {
    /// Build with the built-in synonym table.
    #[must_use]
    pub fn new() -> Self {
        let mut synonyms = HashMap::new();
        // canine family
        for w in ["canine", "dog", "puppy", "hound", "pup"] {
            synonyms.insert(w, "dog");
        }
        // feline family
        for w in ["feline", "cat", "kitten", "kitty"] {
            synonyms.insert(w, "cat");
        }
        // vehicle
        for w in ["automobile", "car", "vehicle", "auto"] {
            synonyms.insert(w, "car");
        }
        Self { synonyms }
    }

    fn normalize_token<'a>(&'a self, tok: &'a str) -> &'a str {
        self.synonyms.get(tok).copied().unwrap_or(tok)
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
            let concept = self.normalize_token(raw);
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

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
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
