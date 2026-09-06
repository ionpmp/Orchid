//! Sentence embedding backends for Orchid Phase 5.
//!
//! Default builds ship [`StubEmbedder`] — a deterministic, model-free
//! embedder suitable for CI and hybrid-search proofs. A real ORT-backed
//! model can land behind the `ort` feature later (bundled like `pdfium.dll`).

#![warn(missing_docs)]
#![warn(clippy::all)]

mod stub;

pub use stub::{StubEmbedder, STUB_DIMS, STUB_MODEL_ID};

use orchid_format::{document_embedding, EmbeddingPayload};

/// Errors from embedding backends.
#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    /// Backend-specific failure.
    #[error("embed failed: {0}")]
    Failed(String),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, EmbedError>;

/// Pluggable sentence / passage embedder.
pub trait Embedder: Send + Sync {
    /// Model identifier recorded in Embedding TOC `content_type`.
    fn model_id(&self) -> &str;

    /// Output dimensionality.
    fn dimensions(&self) -> usize;

    /// Embed one UTF-8 string into an L2-normalised vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed many strings (default: sequential [`Self::embed`]).
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}

/// Build a document-level [`EmbeddingPayload`] with [`StubEmbedder`], or `None` for blank text.
pub fn stub_embedding_payload(clean_text: &str) -> Result<Option<EmbeddingPayload>> {
    if clean_text.trim().is_empty() {
        return Ok(None);
    }
    let emb = StubEmbedder::new();
    let vector = emb.embed(clean_text)?;
    let tokens = clean_text.split_whitespace().count() as u32;
    Ok(Some(document_embedding(
        emb.model_id(),
        clean_text.len() as u64,
        tokens.max(1),
        vector,
    )))
}

/// Cosine similarity for L2-normalised vectors (≈ dot product).
#[must_use]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
