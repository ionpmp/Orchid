//! Bundled quantized ONNX encoder.
//!
//! The graph is `models/hash-q.onnx` (opset 13, about 2 KiB). Input
//! `features` is `float32[1, 32]`: a bag of concept hashes. An int8 weight
//! matrix is dequantized with scale `1/127`, multiplied, and L2-normalised
//! to `embedding` `float32[1, 64]`. This is a hash projection, not a
//! transformer. A replacement file must use the same input and output names.

use std::path::Path;
use std::sync::Mutex;

use ort::session::Session;
use ort::value::TensorRef;

use crate::concepts::{fnv1a64, fold_concept, l2_normalize};
use crate::{EmbedError, Embedder, Result};

/// Model id recorded for the compiled-in graph.
pub const BUNDLED_ONNX_MODEL_ID: &str = "orchid.onnx.hash.q.v1";

/// Output width of the compiled-in graph.
pub const BUNDLED_ONNX_DIMS: usize = 64;

const FEATURES: usize = 32;

const BUNDLED_MODEL: &[u8] = include_bytes!("../models/hash-q.onnx");

/// ONNX Runtime session around a hash-projection graph.
pub struct OnnxEmbedder {
    session: Mutex<Session>,
    dims: usize,
    model_id: String,
}

impl OnnxEmbedder {
    /// Load the compiled-in quantized graph.
    pub fn bundled() -> Result<Self> {
        Self::from_bytes(BUNDLED_MODEL, BUNDLED_ONNX_MODEL_ID.to_string())
    }

    /// Load a replacement graph from disk.
    ///
    /// The file must accept `features` and return `embedding`. The model id
    /// is `onnx.file.` plus the file stem, so it does not collide with the
    /// compiled-in id.
    pub fn open_file(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .map_err(|e| EmbedError::Failed(format!("read {}: {e}", path.display())))?;
        Self::from_bytes(&bytes, model_id_from_path(path))
    }

    fn from_bytes(bytes: &[u8], model_id: String) -> Result<Self> {
        let mut session = Session::builder()
            .map_err(map_ort)?
            .with_intra_threads(1)
            .map_err(map_ort)?
            .commit_from_memory(bytes)
            .map_err(map_ort)?;
        let dims = probe_dims(&mut session)?;
        Ok(Self {
            session: Mutex::new(session),
            dims,
            model_id,
        })
    }
}

impl Embedder for OnnxEmbedder {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let features = hash_features(text)?;
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        run(&mut session, &features)
    }
}

fn hash_features(text: &str) -> Result<Vec<f32>> {
    let mut features = vec![0.0f32; FEATURES];
    let lower = text.to_ascii_lowercase();
    let mut any = false;
    for raw in lower.split(|c: char| !c.is_ascii_alphanumeric()) {
        if raw.is_empty() {
            continue;
        }
        any = true;
        let concept = fold_concept(raw);
        let h = fnv1a64(concept.as_bytes());
        let i0 = (h as usize) % FEATURES;
        let i1 = ((h >> 32) as usize) % FEATURES;
        features[i0] += 1.0;
        features[i1] += 0.5;
    }
    if !any {
        return Err(EmbedError::Failed("empty text".into()));
    }
    Ok(features)
}

fn probe_dims(session: &mut Session) -> Result<usize> {
    let mut features = vec![0.0f32; FEATURES];
    features[0] = 1.0;
    Ok(run(session, &features)?.len())
}

fn run(session: &mut Session, features: &[f32]) -> Result<Vec<f32>> {
    let tensor = TensorRef::from_array_view(([1usize, FEATURES], features)).map_err(map_ort)?;
    let outputs = session
        .run(ort::inputs!["features" => tensor])
        .map_err(map_ort)?;
    let (_shape, data) = outputs["embedding"]
        .try_extract_tensor::<f32>()
        .map_err(map_ort)?;
    let mut vector = data.to_vec();
    l2_normalize(&mut vector);
    if vector.iter().all(|x| *x == 0.0) {
        return Err(EmbedError::Failed("model returned a zero embedding".into()));
    }
    Ok(vector)
}

fn model_id_from_path(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("model");
    let mut id = String::from("onnx.file.");
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
            id.push(c.to_ascii_lowercase());
        } else {
            id.push('_');
        }
    }
    id
}

fn map_ort(err: impl std::fmt::Display) -> EmbedError {
    EmbedError::Failed(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cosine_similarity;

    #[test]
    fn bundled_model_is_unit_and_keeps_synonyms_close() {
        let embedder = OnnxEmbedder::bundled().unwrap();
        assert_eq!(embedder.model_id(), BUNDLED_ONNX_MODEL_ID);
        assert_eq!(embedder.dimensions(), BUNDLED_ONNX_DIMS);
        let dog = embedder.embed("friendly dog plays outside").unwrap();
        let canine = embedder.embed("canine companion").unwrap();
        let car = embedder.embed("red automobile on highway").unwrap();
        assert_eq!(dog.len(), BUNDLED_ONNX_DIMS);
        let norm = dog.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "norm {norm}");
        assert!(cosine_similarity(&dog, &canine) > cosine_similarity(&dog, &car));
        assert!(embedder.embed("   ").is_err());
    }

    #[test]
    fn replacement_file_uses_its_own_model_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Hash-Q.onnx");
        std::fs::write(&path, BUNDLED_MODEL).unwrap();
        let embedder = OnnxEmbedder::open_file(&path).unwrap();
        assert_eq!(embedder.model_id(), "onnx.file.hash-q");
        assert_eq!(embedder.dimensions(), BUNDLED_ONNX_DIMS);
    }
}
