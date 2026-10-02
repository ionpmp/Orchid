//! Choose the embedder the rest of the process should use.

use std::path::Path;
use std::sync::Arc;

use crate::{Embedder, StubEmbedder};

/// Embedder for this build.
///
/// When the `ort` feature is on, a readable ONNX file at `sentence_model`
/// is loaded first. If that path is missing or the graph refuses to load,
/// the compiled-in quantized model is used. If ONNX Runtime itself fails,
/// the synonym stub is used. Builds without `ort` always use the stub.
#[must_use]
pub fn open_embedder(sentence_model: Option<&Path>) -> Arc<dyn Embedder> {
    #[cfg(feature = "ort")]
    {
        if let Some(path) = sentence_model {
            if path.is_file() {
                match crate::onnx::OnnxEmbedder::open_file(path) {
                    Ok(embedder) => return Arc::new(embedder),
                    Err(e) => tracing::warn!(
                        error = %e,
                        path = %path.display(),
                        "sentence model failed; using the bundled ONNX model"
                    ),
                }
            } else {
                tracing::warn!(
                    path = %path.display(),
                    "sentence model is missing; using the bundled ONNX model"
                );
            }
        }
        match crate::onnx::OnnxEmbedder::bundled() {
            Ok(embedder) => return Arc::new(embedder),
            Err(e) => tracing::warn!(
                error = %e,
                "bundled ONNX model failed; using the synonym stub"
            ),
        }
    }
    #[cfg(not(feature = "ort"))]
    if let Some(path) = sentence_model {
        tracing::warn!(
            path = %path.display(),
            "sentence-model is set, but this build has no ONNX Runtime; using the synonym stub"
        );
    }
    Arc::new(StubEmbedder::new())
}
