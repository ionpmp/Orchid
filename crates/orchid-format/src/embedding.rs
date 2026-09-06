//! Embedding region wire format (Phase 5).
//!
//! Binary layout (little-endian):
//! ```text
//! magic:        b"OREM" (4)
//! version:      u16 = 1
//! model_id_len: u16 + UTF-8 model id
//! dims:         u16
//! record_count: u32
//! for each record:
//!   level:       u8   (0=document, 1=section, 2=paragraph)
//!   span_start:  u64  (UTF-8 byte offset into Clean-Text)
//!   span_end:    u64
//!   token_count: u32
//!   vector:      dims × f32
//! ```

use crate::{FormatError, Result};

/// Embedding payload magic (`OREM`).
pub const EMBEDDING_MAGIC: &[u8; 4] = b"OREM";

/// Wire version for [`EmbeddingPayload`].
pub const EMBEDDING_WIRE_VERSION: u16 = 1;

/// TOC / region content-type for hierarchical f32 embeddings.
pub const EMBEDDING_HIER_F32_V1: &str = "orchid.embedding.hier.f32.v1";

/// Granularity of an embedding record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EmbeddingLevel {
    /// Whole Clean-Text document.
    Document = 0,
    /// Heading-bounded section.
    Section = 1,
    /// Paragraph / block.
    Paragraph = 2,
}

impl EmbeddingLevel {
    fn from_u8(v: u8) -> Result<Self> {
        match v {
            0 => Ok(Self::Document),
            1 => Ok(Self::Section),
            2 => Ok(Self::Paragraph),
            _ => Err(FormatError::RegionDecode(format!(
                "unknown embedding level {v}"
            ))),
        }
    }
}

/// One hierarchical vector + Clean-Text span + token budget.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingRecord {
    /// Granularity.
    pub level: EmbeddingLevel,
    /// Inclusive-exclusive UTF-8 byte range into Clean-Text.
    pub span_start: u64,
    /// End of span (exclusive).
    pub span_end: u64,
    /// Approximate token count for the span (model tokenizer).
    pub token_count: u32,
    /// L2-normalised (or model-native) vector; length = payload `dims`.
    pub vector: Vec<f32>,
}

/// Decoded Embedding region body.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingPayload {
    /// Model id (e.g. `orchid.stub.synonym.v1` or ORT asset id).
    pub model_id: String,
    /// Vector dimensionality.
    pub dims: u16,
    /// Hierarchical records (document first is conventional).
    pub records: Vec<EmbeddingRecord>,
}

impl EmbeddingPayload {
    /// First document-level vector, if any.
    #[must_use]
    pub fn document_vector(&self) -> Option<&[f32]> {
        self.records
            .iter()
            .find(|r| r.level == EmbeddingLevel::Document)
            .map(|r| r.vector.as_slice())
    }

    /// Encode to region plaintext bytes.
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.dims == 0 {
            return Err(FormatError::RegionDecode(
                "embedding dims must be > 0".into(),
            ));
        }
        let model_bytes = self.model_id.as_bytes();
        if model_bytes.len() > u16::MAX as usize {
            return Err(FormatError::RegionDecode("model_id too long".into()));
        }
        let mut out = Vec::with_capacity(
            4 + 2
                + 2
                + model_bytes.len()
                + 2
                + 4
                + self.records.len() * (1 + 8 + 8 + 4 + 4 * self.dims as usize),
        );
        out.extend_from_slice(EMBEDDING_MAGIC);
        out.extend_from_slice(&EMBEDDING_WIRE_VERSION.to_le_bytes());
        out.extend_from_slice(&(model_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(model_bytes);
        out.extend_from_slice(&self.dims.to_le_bytes());
        out.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        for r in &self.records {
            if r.vector.len() != self.dims as usize {
                return Err(FormatError::RegionDecode(format!(
                    "vector len {} != dims {}",
                    r.vector.len(),
                    self.dims
                )));
            }
            if r.span_end < r.span_start {
                return Err(FormatError::RegionDecode(
                    "embedding span_end < span_start".into(),
                ));
            }
            out.push(r.level as u8);
            out.extend_from_slice(&r.span_start.to_le_bytes());
            out.extend_from_slice(&r.span_end.to_le_bytes());
            out.extend_from_slice(&r.token_count.to_le_bytes());
            for f in &r.vector {
                out.extend_from_slice(&f.to_le_bytes());
            }
        }
        Ok(out)
    }

    /// Decode region plaintext.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut cur = 0usize;
        let take = |cur: &mut usize, n: usize| -> Result<&[u8]> {
            if *cur + n > bytes.len() {
                return Err(FormatError::RegionDecode(
                    "truncated embedding payload".into(),
                ));
            }
            let s = &bytes[*cur..*cur + n];
            *cur += n;
            Ok(s)
        };
        let magic = take(&mut cur, 4)?;
        if magic != EMBEDDING_MAGIC {
            return Err(FormatError::RegionDecode(format!(
                "bad embedding magic {magic:?}"
            )));
        }
        let ver = u16::from_le_bytes(take(&mut cur, 2)?.try_into().unwrap());
        if ver != EMBEDDING_WIRE_VERSION {
            return Err(FormatError::RegionDecode(format!(
                "unsupported embedding wire version {ver}"
            )));
        }
        let mid_len = u16::from_le_bytes(take(&mut cur, 2)?.try_into().unwrap()) as usize;
        let model_id = String::from_utf8(take(&mut cur, mid_len)?.to_vec())
            .map_err(|e| FormatError::RegionDecode(format!("model_id utf8: {e}")))?;
        let dims = u16::from_le_bytes(take(&mut cur, 2)?.try_into().unwrap());
        if dims == 0 {
            return Err(FormatError::RegionDecode(
                "embedding dims must be > 0".into(),
            ));
        }
        let nrec = u32::from_le_bytes(take(&mut cur, 4)?.try_into().unwrap()) as usize;
        let mut records = Vec::with_capacity(nrec);
        for _ in 0..nrec {
            let level = EmbeddingLevel::from_u8(take(&mut cur, 1)?[0])?;
            let span_start = u64::from_le_bytes(take(&mut cur, 8)?.try_into().unwrap());
            let span_end = u64::from_le_bytes(take(&mut cur, 8)?.try_into().unwrap());
            let token_count = u32::from_le_bytes(take(&mut cur, 4)?.try_into().unwrap());
            let mut vector = Vec::with_capacity(dims as usize);
            for _ in 0..dims {
                let bits = take(&mut cur, 4)?;
                vector.push(f32::from_le_bytes(bits.try_into().unwrap()));
            }
            records.push(EmbeddingRecord {
                level,
                span_start,
                span_end,
                token_count,
                vector,
            });
        }
        if cur != bytes.len() {
            return Err(FormatError::RegionDecode(format!(
                "trailing {} bytes in embedding payload",
                bytes.len() - cur
            )));
        }
        Ok(Self {
            model_id,
            dims,
            records,
        })
    }
}

/// Build a document-level payload from a single vector (token_count ≈ whitespace words).
#[must_use]
pub fn document_embedding(
    model_id: impl Into<String>,
    clean_text_len: u64,
    token_count: u32,
    vector: Vec<f32>,
) -> EmbeddingPayload {
    let dims = vector.len() as u16;
    EmbeddingPayload {
        model_id: model_id.into(),
        dims,
        records: vec![EmbeddingRecord {
            level: EmbeddingLevel::Document,
            span_start: 0,
            span_end: clean_text_len,
            token_count,
            vector,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_document_vector() {
        let p = document_embedding("orchid.stub.synonym.v1", 12, 3, vec![0.1, 0.2, 0.3, 0.4]);
        let bytes = p.encode().unwrap();
        assert_eq!(&bytes[0..4], b"OREM");
        let back = EmbeddingPayload::decode(&bytes).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.document_vector().unwrap(), &[0.1, 0.2, 0.3, 0.4]);
    }
}
