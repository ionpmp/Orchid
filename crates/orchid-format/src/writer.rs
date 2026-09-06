//! Sealed `.orchid` writer (Phase 1 + Phase 2 encryption).

use std::fs::File;
use std::io::Write;
use std::path::Path;

use orchid_crypto::content::hash_bytes;
use orchid_crypto::Identity;
use uuid::Uuid;

use crate::capability;
use crate::content_type::{STRUCTURED_CRDT_V1, STRUCTURED_SNAPSHOT_V1};
use crate::crdt::{encode_crdt_payload, CrdtDocument};
use crate::crypto_region::prepare_region_body;
use crate::embedding::{EmbeddingPayload, EMBEDDING_HIER_F32_V1};
use crate::framing::{pad_to_alignment, Footer, Header, RegionHeader};
use crate::provenance::{sign_clean_text_provenance, PROVENANCE_CONTENT_TYPE};
use crate::region_type::{CLEAN_TEXT, EMBEDDING, PROVENANCE, RAW, STRUCTURED};
use crate::toc::{CompressionCodec, RegionType, StorageMode};
use crate::toc_build::{build_toc, TocRegionSpec, TocSpec};
use crate::{Result, FOOTER_SIZE, HEADER_SIZE};

/// Inputs for a sealed file with Raw + Clean-Text + Structured.
#[derive(Debug, Clone)]
pub struct SealedCreateRequest {
    /// Optional document UUID; allocated when `None`.
    pub file_uuid: Option<[u8; 16]>,
    /// UTC ms; wall clock when `None`.
    pub created_unix_ms: Option<u64>,
    /// Raw region plaintext (stored uncompressed unless encrypted).
    pub raw: Vec<u8>,
    /// Optional Raw MIME hint.
    pub raw_content_type: Option<String>,
    /// Optional Raw display name.
    pub raw_name: Option<String>,
    /// Clean-Text UTF-8 (zstd-compressed on disk).
    pub clean_text: Vec<u8>,
    /// Structured snapshot bytes (ignored when [`Self::structured_crdt`] is set).
    pub structured: Vec<u8>,
    /// Optional Structured content-type / schema id (snapshot path).
    pub structured_content_type: Option<String>,
    /// When set, encodes a CRDT Structured region (`orchid.structured.crdt.v1`).
    pub structured_crdt: Option<CrdtDocument>,
    /// When set, private regions are age-encrypted to this identity.
    pub encrypt_with: Option<Identity>,
    /// When true, append a public C2PA Provenance region (signed PNG carrier).
    pub sign_c2pa: bool,
    /// Optional hierarchical Embedding region (sets [`capability::EMBEDDINGS`]).
    pub embeddings: Option<EmbeddingPayload>,
}

/// Write a sealed `.orchid` to `path`.
pub fn write_sealed_file(path: &Path, req: &SealedCreateRequest) -> Result<()> {
    let bytes = build_sealed_bytes(req)?;
    let mut file = File::create(path)?;
    file.write_all(&bytes)?;
    Ok(())
}

/// Build a sealed `.orchid` image in memory.
pub fn build_sealed_bytes(req: &SealedCreateRequest) -> Result<Vec<u8>> {
    let file_uuid = req.file_uuid.unwrap_or_else(|| *Uuid::new_v4().as_bytes());
    let created_unix_ms = req.created_unix_ms.unwrap_or_else(now_unix_ms);
    let identity = req.encrypt_with.as_ref();
    let mut caps = 0u64;
    if identity.is_some() {
        caps |= capability::ENCRYPTED;
    }
    let provenance = if req.sign_c2pa {
        caps |= capability::C2PA;
        Some(sign_clean_text_provenance(
            &req.clean_text,
            "Orchid document",
        )?)
    } else {
        None
    };
    let embedding_plain = if let Some(emb) = &req.embeddings {
        caps |= capability::EMBEDDINGS;
        Some(emb.encode()?)
    } else {
        None
    };

    let (structured_plain, structured_ctype) = if let Some(doc) = &req.structured_crdt {
        caps |= capability::CRDT;
        (encode_crdt_payload(doc)?, STRUCTURED_CRDT_V1.to_string())
    } else {
        (
            req.structured.clone(),
            req.structured_content_type
                .clone()
                .unwrap_or_else(|| STRUCTURED_SNAPSHOT_V1.to_string()),
        )
    };

    let raw_body = prepare_region_body(&req.raw, CompressionCodec::None, identity)?;
    let clean_body = prepare_region_body(&req.clean_text, CompressionCodec::Zstd, identity)?;
    let structured_body =
        prepare_region_body(&structured_plain, CompressionCodec::Zstd, identity)?;

    let mut buf = Vec::new();
    let header = Header::new(file_uuid, created_unix_ms, caps);
    buf.extend_from_slice(&header.encode());
    debug_assert_eq!(buf.len() as u64, HEADER_SIZE);

    let mut toc_regions = Vec::with_capacity(3);

    append_region(
        &mut buf,
        &mut toc_regions,
        RegionWrite {
            type_id: RAW,
            fb_type: RegionType::Raw,
            payload: &raw_body.stored,
            compression: raw_body.compression,
            payload_blake3: raw_body.payload_blake3,
            encryption: raw_body.encryption,
            name: req.raw_name.clone(),
            ordinal: 0,
            content_type: req.raw_content_type.clone(),
        },
    )?;
    append_region(
        &mut buf,
        &mut toc_regions,
        RegionWrite {
            type_id: CLEAN_TEXT,
            fb_type: RegionType::CleanText,
            payload: &clean_body.stored,
            compression: clean_body.compression,
            payload_blake3: clean_body.payload_blake3,
            encryption: clean_body.encryption,
            name: Some("clean-text".into()),
            ordinal: 0,
            content_type: Some("text/plain; charset=utf-8".into()),
        },
    )?;
    append_region(
        &mut buf,
        &mut toc_regions,
        RegionWrite {
            type_id: STRUCTURED,
            fb_type: RegionType::Structured,
            payload: &structured_body.stored,
            compression: structured_body.compression,
            payload_blake3: structured_body.payload_blake3,
            encryption: structured_body.encryption,
            name: Some("structured".into()),
            ordinal: 0,
            content_type: Some(structured_ctype),
        },
    )?;

    if let Some(emb_bytes) = &embedding_plain {
        // Embeddings: uncompressed (vectors already dense); may be encrypted.
        let emb_body = prepare_region_body(emb_bytes, CompressionCodec::None, identity)?;
        append_region(
            &mut buf,
            &mut toc_regions,
            RegionWrite {
                type_id: EMBEDDING,
                fb_type: RegionType::Embedding,
                payload: &emb_body.stored,
                compression: emb_body.compression,
                payload_blake3: emb_body.payload_blake3,
                encryption: emb_body.encryption,
                name: Some("embeddings".into()),
                ordinal: 0,
                content_type: Some(EMBEDDING_HIER_F32_V1.into()),
            },
        )?;
    }

    if let Some(prov) = &provenance {
        // Public Provenance: never age-encrypted; uncompressed PNG carrier.
        append_region(
            &mut buf,
            &mut toc_regions,
            RegionWrite {
                type_id: PROVENANCE,
                fb_type: RegionType::Provenance,
                payload: &prov.carrier_png,
                compression: CompressionCodec::None,
                payload_blake3: hash_bytes(&prov.carrier_png),
                encryption: None,
                name: Some("provenance".into()),
                ordinal: 0,
                content_type: Some(PROVENANCE_CONTENT_TYPE.into()),
            },
        )?;
    }

    pad_to_alignment(&mut buf);
    let toc_start = buf.len() as u64;
    let toc_bytes = build_toc(&TocSpec {
        generation: 1,
        parent_generation: 0,
        file_uuid,
        default_storage: StorageMode::Inline,
        regions: toc_regions,
    })?;
    buf.extend_from_slice(&toc_bytes);

    let toc_blake3 = hash_bytes(&toc_bytes);
    let file_len_with_footer = (buf.len() + FOOTER_SIZE) as u64;
    let toc_offset_from_end = file_len_with_footer - toc_start;
    let footer = Footer {
        toc_offset_from_end,
        toc_blake3,
    };
    buf.extend_from_slice(&footer.encode());
    Ok(buf)
}

struct RegionWrite<'a> {
    type_id: u16,
    fb_type: RegionType,
    payload: &'a [u8],
    compression: CompressionCodec,
    payload_blake3: [u8; 32],
    encryption: Option<crate::crypto_region::RegionEncryptionSpec>,
    name: Option<String>,
    ordinal: u32,
    content_type: Option<String>,
}

fn append_region(
    buf: &mut Vec<u8>,
    toc_regions: &mut Vec<TocRegionSpec>,
    region: RegionWrite<'_>,
) -> Result<()> {
    pad_to_alignment(buf);
    let offset = buf.len() as u64;
    let length = region.payload.len() as u64;
    let header = RegionHeader {
        type_id: region.type_id,
        length,
    };
    buf.extend_from_slice(&header.encode());
    buf.extend_from_slice(region.payload);
    toc_regions.push(TocRegionSpec {
        type_: region.fb_type,
        offset,
        length,
        name: region.name,
        ordinal: region.ordinal,
        compression: region.compression,
        storage: StorageMode::Inline,
        chunks: vec![],
        encryption: region.encryption,
        content_type: region.content_type,
        payload_blake3: region.payload_blake3,
    });
    Ok(())
}

fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Convenience: empty Raw for text-only sealed docs still satisfies Phase 1
/// “exactly three regions”.
pub fn empty_raw() -> Vec<u8> {
    Vec::new()
}

impl Default for SealedCreateRequest {
    fn default() -> Self {
        Self {
            file_uuid: None,
            created_unix_ms: None,
            raw: empty_raw(),
            raw_content_type: None,
            raw_name: None,
            clean_text: Vec::new(),
            structured: Vec::new(),
            structured_content_type: None,
            structured_crdt: None,
            encrypt_with: None,
            sign_c2pa: false,
            embeddings: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::{Footer, Header, RegionHeader};
    use crate::{FILE_MAGIC, FOOTER_SIZE, REGION_MAGIC};

    #[test]
    fn sealed_bytes_have_magic_and_three_regions() {
        let bytes = build_sealed_bytes(&SealedCreateRequest {
            file_uuid: Some([3u8; 16]),
            created_unix_ms: Some(42),
            raw: b"RAW".to_vec(),
            raw_content_type: Some("application/octet-stream".into()),
            raw_name: Some("blob.bin".into()),
            clean_text: b"hello\n".to_vec(),
            structured: b"{\"v\":1}".to_vec(),
            structured_content_type: Some("application/json".into()),
            structured_crdt: None,
            encrypt_with: None,
            sign_c2pa: false,
            embeddings: None,
        })
        .unwrap();
        assert!(bytes.len() > 4096 + FOOTER_SIZE);
        assert_eq!(&bytes[0..4], FILE_MAGIC.as_slice());
        let header = Header::decode(&bytes).unwrap();
        assert_eq!(header.file_uuid, [3u8; 16]);
        assert_eq!(header.created_unix_ms, 42);
        assert_eq!(header.capability_flags, 0);

        let footer = Footer::decode(&bytes).unwrap();
        let toc_start = bytes.len() as u64 - footer.toc_offset_from_end;
        assert_eq!(toc_start % 4096, 0);
        let toc = &bytes[toc_start as usize..bytes.len() - FOOTER_SIZE];
        assert_eq!(hash_bytes(toc), footer.toc_blake3);

        let mut found = 0u32;
        let mut off = 4096usize;
        while off + 14 < toc_start as usize {
            if &bytes[off..off + 4] == REGION_MAGIC.as_slice() {
                let rh = RegionHeader::decode(&bytes[off..], off as u64).unwrap();
                found += 1;
                off += 14 + rh.length as usize;
                let aligned = crate::framing::align_up(off as u64) as usize;
                off = aligned;
            } else {
                off += 4096;
            }
        }
        assert_eq!(found, 3);
    }
}
