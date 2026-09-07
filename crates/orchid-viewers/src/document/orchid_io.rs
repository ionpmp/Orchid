//! Sealed `.orchid` envelope for the document editor (DOCX Raw + Clean-Text).

use std::io::Write;
use std::path::Path;

use orchid_crypto::ChunkStore;
use orchid_format::toc::RegionType;
use orchid_format::{
    capability, linked_region_plaintext, write_linked_file, write_sealed_file, LinkedCreateRequest,
    SealedCreateRequest, SealedFile, EXTENSION as ORCHID_EXT, FILE_MAGIC,
};

use crate::document::model::{Block, Document, Paragraph, Run};
use crate::document::ooxml::container::{open_document, save_document};
use crate::error::{Result, ViewerError};

/// OOXML Word MIME for Raw region metadata.
pub const DOCX_MIME: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

/// True when the path uses the `.orchid` extension.
#[must_use]
pub fn is_orchid_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("orchid"))
}

/// True when `sample` begins with the `.orchid` file magic (`ORCD`).
#[must_use]
pub fn looks_like_orchid(sample: &[u8]) -> bool {
    sample.len() >= 4 && &sample[0..4] == FILE_MAGIC.as_slice()
}

/// Serialize `doc` to OOXML ZIP bytes (temp file + read).
pub async fn document_to_docx_bytes(doc: &Document) -> Result<Vec<u8>> {
    let tmp = std::env::temp_dir().join(format!("orchid-docx-save-{}.docx", uuid::Uuid::new_v4()));
    save_document(doc, &tmp).await?;
    let bytes = tokio::fs::read(&tmp).await?;
    let _ = tokio::fs::remove_file(&tmp).await;
    Ok(bytes)
}

/// Write a sealed `.orchid` with Raw=DOCX, Clean-Text=`plain_text`.
pub async fn save_document_as_orchid(doc: &Document, output_path: &Path) -> Result<()> {
    save_document_as_orchid_named(doc, output_path, "document.docx").await
}

/// Like [`save_document_as_orchid`], with an explicit Raw TOC `name`.
///
/// Use `"original.docx"` when importing an existing OOXML file so the bytes
/// stay recoverable under that TOC name.
pub async fn save_document_as_orchid_named(
    doc: &Document,
    output_path: &Path,
    raw_name: &str,
) -> Result<()> {
    let raw = document_to_docx_bytes(doc).await?;
    let clean_text = doc.plain_text().into_bytes();
    let embeddings = stub_embeddings_for_clean(&clean_text);
    write_sealed_file(
        output_path,
        &SealedCreateRequest {
            file_uuid: None,
            created_unix_ms: None,
            raw,
            raw_content_type: Some(DOCX_MIME.into()),
            raw_name: Some(raw_name.to_string()),
            clean_text,
            structured: b"{}".to_vec(),
            structured_content_type: Some("application/json".into()),
            structured_crdt: None,
            encrypt_with: None,
            sign_c2pa: false,
            embeddings,
        },
    )
    .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

/// Open a `.orchid`: sealed inline, or linked via `store` when `CAP_LINKED`.
pub async fn open_document_from_orchid_with_store(
    path: &Path,
    store: Option<&ChunkStore>,
) -> Result<Document> {
    let path_buf = path.to_path_buf();
    let linked = {
        let file =
            SealedFile::open(&path_buf).map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        file.header().capability_flags & capability::LINKED != 0
    };
    if linked {
        let store = store.ok_or_else(|| {
            ViewerError::DocumentSave(
                "linked .orchid requires the content-addressed chunk store".into(),
            )
        })?;
        let file =
            SealedFile::open(&path_buf).map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        let raw = linked_region_plaintext(&file, store, RegionType::Raw, None)
            .await
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        if looks_like_zip(&raw) {
            return document_from_docx_bytes(&raw);
        }
        let clean = linked_region_plaintext(&file, store, RegionType::CleanText, None)
            .await
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        return Ok(document_from_plain_utf8(&clean));
    }
    open_document_from_orchid(path).await
}

/// Write a linked `.orchid` (DOCX Raw + Clean-Text chunks in `store`).
pub async fn save_document_as_linked_orchid(
    doc: &Document,
    output_path: &Path,
    store: &ChunkStore,
    file_uuid: Option<[u8; 16]>,
    generation: u64,
    parent_generation: u64,
) -> Result<()> {
    let raw = document_to_docx_bytes(doc).await?;
    let clean_text = doc.plain_text().into_bytes();
    let embeddings = stub_embeddings_for_clean(&clean_text);
    write_linked_file(
        output_path,
        store,
        &LinkedCreateRequest {
            file_uuid,
            created_unix_ms: None,
            generation: generation.max(1),
            parent_generation,
            raw,
            clean_text,
            structured: b"{}".to_vec(),
            embeddings,
            encrypt_with: None,
            chunker: orchid_crypto::ChunkerConfig::default(),
        },
    )
    .await
    .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

fn stub_embeddings_for_clean(clean_text: &[u8]) -> Option<orchid_format::EmbeddingPayload> {
    let text = String::from_utf8_lossy(clean_text);
    orchid_embed::stub_embedding_payload(&text).ok().flatten()
}

/// Read header UUID + TOC generation for linked save bumps.
pub fn orchid_identity(path: &Path) -> Result<([u8; 16], u64)> {
    let file = SealedFile::open(path).map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    let uuid = file.header().file_uuid;
    let gen = file.toc().map(|t| t.generation()).unwrap_or(1);
    Ok((uuid, gen))
}

/// Open a sealed `.orchid`: prefer Raw DOCX, else Clean-Text as paragraphs.
pub async fn open_document_from_orchid(path: &Path) -> Result<Document> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || open_document_from_orchid_sync(&path))
        .await
        .map_err(|e| ViewerError::DocumentSave(format!("join: {e}")))?
}

fn open_document_from_orchid_sync(path: &Path) -> Result<Document> {
    let file = SealedFile::open(path).map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    let raw = file
        .raw(None)
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    if looks_like_zip(&raw) {
        return document_from_docx_bytes(&raw);
    }
    let clean = file
        .clean_text(None)
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(document_from_plain_utf8(&clean))
}

fn document_from_docx_bytes(bytes: &[u8]) -> Result<Document> {
    let tmp = std::env::temp_dir().join(format!("orchid-docx-open-{}.docx", uuid::Uuid::new_v4()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
    }
    let doc = open_document(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    Ok(doc)
}

fn document_from_plain_utf8(bytes: &[u8]) -> Document {
    let text = String::from_utf8_lossy(bytes);
    let blocks = if text.is_empty() {
        vec![Block::Paragraph(Paragraph::default())]
    } else {
        text.split('\n')
            .map(|line| {
                Block::Paragraph(Paragraph {
                    runs: vec![Run {
                        text: line.to_string(),
                        ..Default::default()
                    }],
                    ..Default::default()
                })
            })
            .collect()
    };
    Document {
        blocks,
        ..Default::default()
    }
}

fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == b'P' && bytes[1] == b'K'
}

/// Extension constant for callers (includes leading dot).
#[must_use]
pub fn orchid_extension() -> &'static str {
    ORCHID_EXT
}

/// Native save dialog for a document (`.orchid` or `.docx` export).
#[must_use]
pub fn pick_document_save_path(default_name: &str) -> Option<std::path::PathBuf> {
    let mut path = rfd::FileDialog::new()
        .set_file_name(default_name)
        .add_filter("Orchid document", &["orchid"])
        .add_filter("Word document", &["docx"])
        .save_file()?;
    if path.extension().is_none() {
        path.set_extension("orchid");
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::sample::sample_document;

    #[tokio::test]
    async fn orchid_roundtrip_preserves_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.orchid");
        let doc = sample_document();
        let expected = doc.plain_text();
        save_document_as_orchid(&doc, &path).await.unwrap();
        assert!(looks_like_orchid(&std::fs::read(&path).unwrap()[..4]));
        let back = open_document_from_orchid(&path).await.unwrap();
        assert_eq!(back.plain_text(), expected);
    }

    #[tokio::test]
    async fn linked_orchid_roundtrip_preserves_plain_text() {
        use std::sync::Arc;

        use orchid_crypto::ChunkStore;
        use orchid_storage::StateStore;

        let td = tempfile::tempdir().unwrap();
        let storage = Arc::new(StateStore::open_in_memory("t").unwrap());
        let store = ChunkStore::new(td.path().join("chunks"), storage).unwrap();
        let path = td.path().join("linked.orchid");
        let doc = sample_document();
        let expected = doc.plain_text();
        save_document_as_linked_orchid(&doc, &path, &store, Some([0xAB; 16]), 1, 0)
            .await
            .unwrap();
        let back = open_document_from_orchid_with_store(&path, Some(&store))
            .await
            .unwrap();
        assert_eq!(back.plain_text(), expected);
        let (uuid, gen) = orchid_identity(&path).unwrap();
        assert_eq!(uuid, [0xAB; 16]);
        assert_eq!(gen, 1);
    }

    #[tokio::test]
    async fn sealed_import_names_raw_original_docx() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("imported.orchid");
        let doc = sample_document();
        save_document_as_orchid_named(&doc, &path, "original.docx")
            .await
            .unwrap();
        let file = SealedFile::open(&path).unwrap();
        let raw = file.find_region(RegionType::Raw).unwrap();
        assert_eq!(raw.name().unwrap(), "original.docx");
    }

    #[test]
    fn is_orchid_path_detects_extension() {
        assert!(is_orchid_path(Path::new("a.orchid")));
        assert!(is_orchid_path(Path::new("A.ORCHID")));
        assert!(!is_orchid_path(Path::new("a.docx")));
    }
}
