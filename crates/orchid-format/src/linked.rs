//! Linked-mode helpers: FastCDC + [`ChunkStore`] payloads (Phase 2).

use std::fs::File;
use std::io::Write;
use std::path::Path;

use orchid_crypto::content::hash_bytes;
use orchid_crypto::{ChunkStore, Chunker, ChunkerConfig, Identity};
use uuid::Uuid;

use crate::capability;
use crate::crypto_region::{decode_region_body, prepare_region_body, RegionEncryptionSpec};
use crate::framing::{pad_to_alignment, Footer, Header, RegionHeader};
use crate::reader::SealedFile;
use crate::region_type::{CLEAN_TEXT, RAW, STRUCTURED, VERSION_HISTORY};
use crate::toc::{CompressionCodec, RegionType, StorageMode};
use crate::toc_build::{build_toc, TocChunkSpec, TocRegionSpec, TocSpec};
use crate::{FormatError, Result, FOOTER_SIZE, HEADER_SIZE};

/// Inputs for writing a linked `.orchid` (payloads live in `ChunkStore`).
#[derive(Debug, Clone)]
pub struct LinkedCreateRequest {
    /// Optional document UUID.
    pub file_uuid: Option<[u8; 16]>,
    /// UTC ms; wall clock when `None`.
    pub created_unix_ms: Option<u64>,
    /// Generation (defaults to 1).
    pub generation: u64,
    /// Parent generation.
    pub parent_generation: u64,
    /// Raw plaintext.
    pub raw: Vec<u8>,
    /// Clean-Text UTF-8.
    pub clean_text: Vec<u8>,
    /// Structured snapshot.
    pub structured: Vec<u8>,
    /// Optional age identity for private regions.
    pub encrypt_with: Option<Identity>,
    /// Chunker tunables (tests may use smaller sizes).
    pub chunker: ChunkerConfig,
}

impl Default for LinkedCreateRequest {
    fn default() -> Self {
        Self {
            file_uuid: None,
            created_unix_ms: None,
            generation: 1,
            parent_generation: 0,
            raw: Vec::new(),
            clean_text: Vec::new(),
            structured: Vec::new(),
            encrypt_with: None,
            chunker: ChunkerConfig::default(),
        }
    }
}

/// Write a linked `.orchid` to `path`, storing region bodies in `store`.
pub async fn write_linked_file(
    path: &Path,
    store: &ChunkStore,
    req: &LinkedCreateRequest,
) -> Result<()> {
    let bytes = build_linked_bytes(store, req).await?;
    let mut file = File::create(path)?;
    file.write_all(&bytes)?;
    Ok(())
}

/// Build a linked `.orchid` image; region payloads are chunked into `store`.
pub async fn build_linked_bytes(store: &ChunkStore, req: &LinkedCreateRequest) -> Result<Vec<u8>> {
    let file_uuid = req.file_uuid.unwrap_or_else(|| *Uuid::new_v4().as_bytes());
    let created_unix_ms = req.created_unix_ms.unwrap_or_else(now_unix_ms);
    let identity = req.encrypt_with.as_ref();
    let mut caps = capability::LINKED;
    if identity.is_some() {
        caps |= capability::ENCRYPTED;
    }

    let prepared = [
        (
            RAW,
            RegionType::Raw,
            prepare_region_body(&req.raw, CompressionCodec::None, identity)?,
            None,
            0u32,
        ),
        (
            CLEAN_TEXT,
            RegionType::CleanText,
            prepare_region_body(&req.clean_text, CompressionCodec::Zstd, identity)?,
            Some("clean-text".into()),
            0,
        ),
        (
            STRUCTURED,
            RegionType::Structured,
            prepare_region_body(&req.structured, CompressionCodec::Zstd, identity)?,
            Some("structured".into()),
            0,
        ),
    ];

    let chunker = Chunker::new(req.chunker);
    let mut buf = Vec::new();
    let header = Header::new(file_uuid, created_unix_ms, caps);
    buf.extend_from_slice(&header.encode());
    debug_assert_eq!(buf.len() as u64, HEADER_SIZE);

    let mut toc_regions = Vec::new();
    let mut all_chunk_hashes: Vec<[u8; 32]> = Vec::new();

    for (type_id, fb_type, body, name, ordinal) in prepared {
        let chunks = put_chunks(store, &chunker, &body.stored).await?;
        for c in &chunks {
            all_chunk_hashes.push(c.blake3);
        }
        pad_to_alignment(&mut buf);
        let offset = buf.len() as u64;
        let rh = RegionHeader { type_id, length: 0 };
        buf.extend_from_slice(&rh.encode());
        toc_regions.push(TocRegionSpec {
            type_: fb_type,
            offset,
            length: 0,
            name,
            ordinal,
            compression: body.compression,
            storage: StorageMode::Linked,
            chunks,
            encryption: body.encryption,
            content_type: None,
            payload_blake3: body.payload_blake3,
        });
    }

    // Version-history stub listing this generation's chunk hashes.
    let history = encode_version_history_stub(
        req.generation,
        req.parent_generation,
        file_uuid,
        &all_chunk_hashes,
    );
    pad_to_alignment(&mut buf);
    let vh_offset = buf.len() as u64;
    let vh_header = RegionHeader {
        type_id: VERSION_HISTORY,
        length: history.len() as u64,
    };
    buf.extend_from_slice(&vh_header.encode());
    buf.extend_from_slice(&history);
    toc_regions.push(TocRegionSpec {
        type_: RegionType::VersionHistory,
        offset: vh_offset,
        length: history.len() as u64,
        name: Some("version-history".into()),
        ordinal: 0,
        compression: CompressionCodec::None,
        storage: StorageMode::Inline,
        chunks: vec![],
        encryption: None,
        content_type: Some("application/vnd.orchid.version-history+bin".into()),
        payload_blake3: hash_bytes(&history),
    });

    pad_to_alignment(&mut buf);
    let toc_start = buf.len() as u64;
    let toc_bytes = build_toc(&TocSpec {
        generation: req.generation,
        parent_generation: req.parent_generation,
        file_uuid,
        default_storage: StorageMode::Linked,
        regions: toc_regions,
    })?;
    buf.extend_from_slice(&toc_bytes);
    let toc_blake3 = hash_bytes(&toc_bytes);
    let file_len_with_footer = (buf.len() + FOOTER_SIZE) as u64;
    let footer = Footer {
        toc_offset_from_end: file_len_with_footer - toc_start,
        toc_blake3,
    };
    buf.extend_from_slice(&footer.encode());
    Ok(buf)
}

/// Reassemble + decode a linked region from `store`.
pub async fn linked_region_plaintext(
    file: &SealedFile,
    store: &ChunkStore,
    type_: RegionType,
    identity: Option<&Identity>,
) -> Result<Vec<u8>> {
    let entry = file.find_region(type_)?;
    if entry.storage() != StorageMode::Linked {
        return file.region_plaintext(&entry, identity);
    }
    let stored = gather_chunks(store, &entry).await?;
    let payload_blake3 = entry.payload_blake3().and_then(|v| {
        if v.len() == 32 {
            let mut h = [0u8; 32];
            h.copy_from_slice(v.bytes());
            Some(h)
        } else {
            None
        }
    });
    let enc = entry.encryption().map(|e| RegionEncryptionSpec {
        age_header: e
            .age_header()
            .map(|v| v.bytes().to_vec())
            .unwrap_or_default(),
        plaintext_blake3: {
            let mut h = [0u8; 32];
            if let Some(v) = e.plaintext_blake3() {
                if v.len() == 32 {
                    h.copy_from_slice(v.bytes());
                }
            }
            h
        },
        identity_kind: e.identity_kind(),
        is_public: e.is_public(),
    });
    decode_region_body(
        &stored,
        entry.compression(),
        payload_blake3,
        enc.as_ref(),
        identity,
    )
}

/// Repack a sealed file into a linked file in `store`.
pub async fn sealed_to_linked(
    sealed_path: &Path,
    linked_path: &Path,
    store: &ChunkStore,
    chunker: ChunkerConfig,
    identity: Option<&Identity>,
) -> Result<()> {
    let sealed = SealedFile::open(sealed_path)?;
    let header = sealed.header().clone();
    // Need plaintext to re-run prepare (or copy stored bodies). Prefer
    // re-encoding from plaintext so payload_blake3 semantics stay correct.
    let raw = sealed.raw(identity)?;
    let clean = sealed.clean_text(identity)?;
    let structured = sealed.structured(identity)?;
    let req = LinkedCreateRequest {
        file_uuid: Some(header.file_uuid),
        created_unix_ms: Some(header.created_unix_ms),
        generation: sealed.toc()?.generation().saturating_add(1).max(2),
        parent_generation: sealed.toc()?.generation(),
        raw,
        clean_text: clean,
        structured,
        encrypt_with: identity.cloned(),
        chunker,
    };
    write_linked_file(linked_path, store, &req).await
}

/// Repack a linked file back to a sealed inline image.
pub async fn linked_to_sealed(
    linked_path: &Path,
    sealed_path: &Path,
    store: &ChunkStore,
    identity: Option<&Identity>,
) -> Result<()> {
    let linked = SealedFile::open(linked_path)?;
    let header = linked.header().clone();
    let raw = linked_region_plaintext(&linked, store, RegionType::Raw, identity).await?;
    let clean = linked_region_plaintext(&linked, store, RegionType::CleanText, identity).await?;
    let structured =
        linked_region_plaintext(&linked, store, RegionType::Structured, identity).await?;
    crate::writer::write_sealed_file(
        sealed_path,
        &crate::writer::SealedCreateRequest {
            file_uuid: Some(header.file_uuid),
            created_unix_ms: Some(header.created_unix_ms),
            raw,
            raw_content_type: None,
            raw_name: None,
            clean_text: clean,
            structured,
            structured_content_type: None,
            structured_crdt: None,
            encrypt_with: identity.cloned(),
            sign_c2pa: false,
            embeddings: None,
        },
    )
}

async fn put_chunks(
    store: &ChunkStore,
    chunker: &Chunker,
    data: &[u8],
) -> Result<Vec<TocChunkSpec>> {
    if data.is_empty() {
        // Still register an empty chunk so TOC has a stable list.
        let hash = store.put(&[]).await?;
        return Ok(vec![TocChunkSpec {
            blake3: hash,
            size: 0,
        }]);
    }
    let mut out = Vec::new();
    for (meta, slice) in chunker.chunk_bytes(data) {
        let hash = store.put(slice).await?;
        debug_assert_eq!(hash, meta.hash);
        out.push(TocChunkSpec {
            blake3: hash,
            size: slice.len() as u64,
        });
    }
    Ok(out)
}

async fn gather_chunks(store: &ChunkStore, entry: &crate::toc::RegionEntry<'_>) -> Result<Vec<u8>> {
    let chunks = entry
        .chunks()
        .ok_or_else(|| FormatError::InvalidToc("linked region missing chunks".into()))?;
    let mut buf = Vec::new();
    for i in 0..chunks.len() {
        let c = chunks.get(i);
        let blake = c
            .blake3()
            .ok_or_else(|| FormatError::InvalidToc("chunk missing blake3".into()))?;
        if blake.len() != 32 {
            return Err(FormatError::InvalidToc("chunk blake3 len != 32".into()));
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(blake.bytes());
        let bytes = store.get(&hash).await?;
        buf.extend_from_slice(bytes.as_slice());
    }
    Ok(buf)
}

fn encode_version_history_stub(
    generation: u64,
    parent_generation: u64,
    file_uuid: [u8; 16],
    chunk_hashes: &[[u8; 32]],
) -> Vec<u8> {
    // Simple little-endian blob: magic + fields (not FlatBuffers yet).
    let mut v = Vec::new();
    v.extend_from_slice(b"ORVH");
    v.extend_from_slice(&1u32.to_le_bytes()); // schema version
    v.extend_from_slice(&generation.to_le_bytes());
    v.extend_from_slice(&parent_generation.to_le_bytes());
    v.extend_from_slice(&file_uuid);
    v.extend_from_slice(&(chunk_hashes.len() as u32).to_le_bytes());
    for h in chunk_hashes {
        v.extend_from_slice(h);
    }
    v
}

fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
