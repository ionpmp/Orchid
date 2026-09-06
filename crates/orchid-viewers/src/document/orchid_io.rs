//! Sealed `.orchid` envelope for the document editor (DOCX Raw + Clean-Text).

use std::io::Write;
use std::path::Path;

use orchid_format::{
    write_sealed_file, SealedCreateRequest, SealedFile, EXTENSION as ORCHID_EXT, FILE_MAGIC,
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
    let tmp = std::env::temp_dir().join(format!(
        "orchid-docx-save-{}.docx",
        uuid::Uuid::new_v4()
    ));
    save_document(doc, &tmp).await?;
    let bytes = tokio::fs::read(&tmp).await?;
    let _ = tokio::fs::remove_file(&tmp).await;
    Ok(bytes)
}

/// Write a sealed `.orchid` with Raw=DOCX, Clean-Text=`plain_text`.
pub async fn save_document_as_orchid(doc: &Document, output_path: &Path) -> Result<()> {
    let raw = document_to_docx_bytes(doc).await?;
    let clean_text = doc.plain_text().into_bytes();
    write_sealed_file(
        output_path,
        &SealedCreateRequest {
            file_uuid: None,
            created_unix_ms: None,
            raw,
            raw_content_type: Some(DOCX_MIME.into()),
            raw_name: Some("document.docx".into()),
            clean_text,
            structured: b"{}".to_vec(),
            structured_content_type: Some("application/json".into()),
            structured_crdt: None,
            encrypt_with: None,
            sign_c2pa: false,
            embeddings: None,
        },
    )
    .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
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
    let tmp = std::env::temp_dir().join(format!(
        "orchid-docx-open-{}.docx",
        uuid::Uuid::new_v4()
    ));
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

    #[test]
    fn is_orchid_path_detects_extension() {
        assert!(is_orchid_path(Path::new("a.orchid")));
        assert!(is_orchid_path(Path::new("A.ORCHID")));
        assert!(!is_orchid_path(Path::new("a.docx")));
    }
}
