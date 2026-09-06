//! Pack arbitrary bytes into a sealed `.orchid` (FM “Wrap as .orchid”).

use std::path::Path;

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
            embeddings: None,
        },
    )
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
        })
        .unwrap();
        let f = SealedFile::open(&out).unwrap();
        assert_eq!(f.raw(None).unwrap(), b"hello raw");
        assert_eq!(f.clean_text(None).unwrap(), b"hello clean");
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
