//! Pack arbitrary bytes into a `.orchid` (FM “Wrap as .orchid”).

use std::path::Path;

use orchid_crypto::ChunkStore;

use crate::embedding::EmbeddingPayload;
use crate::linked::{write_linked_file, LinkedCreateRequest};
use crate::writer::{write_sealed_file, SealedCreateRequest};
use crate::Result;

/// Inputs for wrapping a payload as Raw + derived Clean-Text.
#[derive(Debug, Clone)]
pub struct WrapAsOrchidRequest {
    /// Destination `.orchid` path.
    pub output: std::path::PathBuf,
    /// Original file bytes (Raw region).
    pub raw: Vec<u8>,
    /// Original display name for Raw TOC.
    pub raw_name: Option<String>,
    /// Optional MIME for Raw.
    pub raw_content_type: Option<String>,
    /// UTF-8 Clean-Text for search (may be empty).
    pub clean_text: Vec<u8>,
    /// Optional hierarchical Embedding region for hybrid search.
    pub embeddings: Option<EmbeddingPayload>,
}

/// Write a sealed `.orchid` with Raw + Clean-Text + empty Structured snapshot.
pub fn wrap_as_sealed(req: &WrapAsOrchidRequest) -> Result<()> {
    write_sealed_file(
        Path::new(&req.output),
        &SealedCreateRequest {
            file_uuid: None,
            created_unix_ms: None,
            raw: req.raw.clone(),
            raw_content_type: req.raw_content_type.clone(),
            raw_name: req.raw_name.clone(),
            clean_text: req.clean_text.clone(),
            structured: b"{}".to_vec(),
            structured_content_type: Some("application/json".into()),
            structured_crdt: None,
            encrypt_with: None,
            sign_c2pa: false,
            embeddings: req.embeddings.clone(),
        },
    )
}

/// Write a linked `.orchid` (payloads in `store`) for library / managed wraps.
pub async fn wrap_as_linked(req: &WrapAsOrchidRequest, store: &ChunkStore) -> Result<()> {
    write_linked_file(
        Path::new(&req.output),
        store,
        &LinkedCreateRequest {
            file_uuid: None,
            created_unix_ms: None,
            generation: 1,
            parent_generation: 0,
            raw: req.raw.clone(),
            clean_text: req.clean_text.clone(),
            structured: b"{}".to_vec(),
            embeddings: req.embeddings.clone(),
            encrypt_with: None,
            chunker: orchid_crypto::ChunkerConfig::default(),
        },
    )
    .await
}

/// Suggest `stem.orchid` beside `source` (keeps multi-dot stems: `a.tar.gz` → `a.tar.gz.orchid`).
#[must_use]
pub fn default_wrap_output(source: &Path) -> std::path::PathBuf {
    let mut out = source.to_path_buf();
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("wrapped");
    out.set_file_name(format!("{name}.orchid"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use orchid_crypto::ChunkStore;
    use orchid_storage::StateStore;

    use crate::capability;
    use crate::document_embedding;
    use crate::SealedFile;

    #[test]
    fn wrap_roundtrip_raw_and_clean() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("note.txt.orchid");
        wrap_as_sealed(&WrapAsOrchidRequest {
            output: out.clone(),
            raw: b"hello raw".to_vec(),
            raw_name: Some("note.txt".into()),
            raw_content_type: Some("text/plain".into()),
            clean_text: b"hello clean".to_vec(),
            embeddings: None,
        })
        .unwrap();
        let f = SealedFile::open(&out).unwrap();
        assert_eq!(f.raw(None).unwrap(), b"hello raw");
        assert_eq!(f.clean_text(None).unwrap(), b"hello clean");
    }

    #[test]
    fn wrap_sealed_with_embeddings_sets_cap() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("emb.txt.orchid");
        let emb = document_embedding("orchid.stub.synonym.v1", 5, 1, vec![0.1; 4]);
        wrap_as_sealed(&WrapAsOrchidRequest {
            output: out.clone(),
            raw: b"raw".to_vec(),
            raw_name: Some("emb.txt".into()),
            raw_content_type: Some("text/plain".into()),
            clean_text: b"hello".to_vec(),
            embeddings: Some(emb.clone()),
        })
        .unwrap();
        let f = SealedFile::open(&out).unwrap();
        assert!(f.header().capability_flags & capability::EMBEDDINGS != 0);
        assert_eq!(f.embeddings(None).unwrap(), emb);
    }

    #[tokio::test]
    async fn wrap_linked_roundtrip_raw_and_clean() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(StateStore::open_in_memory("wrap").unwrap());
        let store = ChunkStore::new(dir.path().join("chunks"), storage).unwrap();
        let out = dir.path().join("note.txt.orchid");
        wrap_as_linked(
            &WrapAsOrchidRequest {
                output: out.clone(),
                raw: b"hello raw".to_vec(),
                raw_name: Some("note.txt".into()),
                raw_content_type: Some("text/plain".into()),
                clean_text: b"hello clean".to_vec(),
                embeddings: None,
            },
            &store,
        )
        .await
        .unwrap();
        let f = SealedFile::open(&out).unwrap();
        assert!(f.header().capability_flags & capability::LINKED != 0);
        let raw = crate::linked_region_plaintext(&f, &store, crate::toc::RegionType::Raw, None)
            .await
            .unwrap();
        let clean =
            crate::linked_region_plaintext(&f, &store, crate::toc::RegionType::CleanText, None)
                .await
                .unwrap();
        assert_eq!(raw, b"hello raw");
        assert_eq!(clean, b"hello clean");
    }

    #[test]
    fn default_output_appends_orchid() {
        let p = Path::new(r"C:\docs\report.pdf");
        assert_eq!(
            default_wrap_output(p)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            "report.pdf.orchid"
        );
    }
}
