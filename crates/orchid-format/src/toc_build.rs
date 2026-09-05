//! Build FlatBuffers TOC blobs for sealed / linked writers.

use flatbuffers::FlatBufferBuilder;

use crate::crypto_region::RegionEncryptionSpec;
use crate::toc::{
    finish_toc_buffer, ChunkRef, ChunkRefArgs, CompressionCodec, EncryptionInfo,
    EncryptionInfoArgs, RegionEntry, RegionEntryArgs, RegionType, StorageMode, Toc, TocArgs,
};
use crate::Result;

/// One CAS chunk listed in a linked region.
#[derive(Debug, Clone)]
pub struct TocChunkSpec {
    /// BLAKE3 of the chunk bytes.
    pub blake3: [u8; 32],
    /// Chunk size in bytes.
    pub size: u64,
}

/// One region row to encode into the TOC.
#[derive(Debug, Clone)]
pub struct TocRegionSpec {
    /// Taxonomy / FlatBuffers region type.
    pub type_: RegionType,
    /// Absolute file offset of `ORCR`.
    pub offset: u64,
    /// Mini-header length field.
    pub length: u64,
    /// Optional human name.
    pub name: Option<String>,
    /// Sibling ordinal.
    pub ordinal: u32,
    /// Compression applied before encryption.
    pub compression: CompressionCodec,
    /// Inline vs linked storage.
    pub storage: StorageMode,
    /// Linked FastCDC chunk list (empty when inline).
    pub chunks: Vec<TocChunkSpec>,
    /// Optional per-region encryption metadata.
    pub encryption: Option<RegionEncryptionSpec>,
    /// MIME / schema hint.
    pub content_type: Option<String>,
    /// BLAKE3 of compressed plaintext before encryption.
    pub payload_blake3: [u8; 32],
}

/// Logical TOC fields for a sealed or linked file.
#[derive(Debug, Clone)]
pub struct TocSpec {
    /// Generation counter (1 for genesis).
    pub generation: u64,
    /// Parent generation (`0` if genesis).
    pub parent_generation: u64,
    /// Echo of header UUID.
    pub file_uuid: [u8; 16],
    /// Default storage mode for regions.
    pub default_storage: StorageMode,
    /// Region rows.
    pub regions: Vec<TocRegionSpec>,
}

/// Encode `spec` as a FlatBuffers root `Toc` buffer.
pub fn build_toc(spec: &TocSpec) -> Result<Vec<u8>> {
    let mut fbb = FlatBufferBuilder::new();
    let mut region_offs = Vec::with_capacity(spec.regions.len());
    for r in &spec.regions {
        let name = r.name.as_ref().map(|s| fbb.create_string(s.as_str()));
        let content_type = r
            .content_type
            .as_ref()
            .map(|s| fbb.create_string(s.as_str()));
        let payload_blake3 = fbb.create_vector(&r.payload_blake3);

        let chunks = if r.chunks.is_empty() {
            None
        } else {
            let mut offs = Vec::with_capacity(r.chunks.len());
            for c in &r.chunks {
                let blake3 = fbb.create_vector(&c.blake3);
                offs.push(ChunkRef::create(
                    &mut fbb,
                    &ChunkRefArgs {
                        blake3: Some(blake3),
                        size: c.size,
                    },
                ));
            }
            Some(fbb.create_vector(&offs))
        };

        let encryption = r.encryption.as_ref().map(|e| {
            let age_header = if e.age_header.is_empty() {
                None
            } else {
                Some(fbb.create_vector(e.age_header.as_slice()))
            };
            let plaintext_blake3 = fbb.create_vector(&e.plaintext_blake3);
            EncryptionInfo::create(
                &mut fbb,
                &EncryptionInfoArgs {
                    age_header,
                    plaintext_blake3: Some(plaintext_blake3),
                    identity_kind: e.identity_kind,
                    is_public: e.is_public,
                },
            )
        });

        let off = RegionEntry::create(
            &mut fbb,
            &RegionEntryArgs {
                type_: r.type_,
                offset: r.offset,
                length: r.length,
                name,
                ordinal: r.ordinal,
                compression: r.compression,
                storage: r.storage,
                chunks,
                encryption,
                content_type,
                payload_blake3: Some(payload_blake3),
            },
        );
        region_offs.push(off);
    }
    let regions = fbb.create_vector(&region_offs);
    let file_uuid = fbb.create_vector(&spec.file_uuid);
    let toc = Toc::create(
        &mut fbb,
        &TocArgs {
            generation: spec.generation,
            parent_generation: spec.parent_generation,
            file_uuid: Some(file_uuid),
            default_storage: spec.default_storage,
            regions: Some(regions),
            zstd_dict_blake3: None,
        },
    );
    finish_toc_buffer(&mut fbb, toc);
    Ok(fbb.finished_data().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toc::root_as_toc;

    #[test]
    fn toc_build_parses() {
        let blob = build_toc(&TocSpec {
            generation: 1,
            parent_generation: 0,
            file_uuid: [1u8; 16],
            default_storage: StorageMode::Inline,
            regions: vec![TocRegionSpec {
                type_: RegionType::CleanText,
                offset: 4096,
                length: 10,
                name: Some("body.txt".into()),
                ordinal: 0,
                compression: CompressionCodec::Zstd,
                storage: StorageMode::Inline,
                chunks: vec![],
                encryption: None,
                content_type: Some("text/plain".into()),
                payload_blake3: [2u8; 32],
            }],
        })
        .unwrap();
        let toc = root_as_toc(&blob).unwrap();
        assert_eq!(toc.generation(), 1);
        let regions = toc.regions().unwrap();
        assert_eq!(regions.len(), 1);
        let r = regions.get(0);
        assert_eq!(r.type_(), RegionType::CleanText);
        assert_eq!(r.offset(), 4096);
        assert_eq!(r.compression(), CompressionCodec::Zstd);
        assert_eq!(r.name().unwrap(), "body.txt");
    }

    #[test]
    fn toc_with_encryption_and_chunks() {
        let blob = build_toc(&TocSpec {
            generation: 2,
            parent_generation: 1,
            file_uuid: [9u8; 16],
            default_storage: StorageMode::Linked,
            regions: vec![TocRegionSpec {
                type_: RegionType::Raw,
                offset: 4096,
                length: 0,
                name: None,
                ordinal: 0,
                compression: CompressionCodec::None,
                storage: StorageMode::Linked,
                chunks: vec![TocChunkSpec {
                    blake3: [3u8; 32],
                    size: 100,
                }],
                encryption: Some(RegionEncryptionSpec {
                    age_header: vec![0x61, 0x67, 0x65],
                    plaintext_blake3: [4u8; 32],
                    identity_kind: 0,
                    is_public: false,
                }),
                content_type: None,
                payload_blake3: [5u8; 32],
            }],
        })
        .unwrap();
        let toc = root_as_toc(&blob).unwrap();
        let r = toc.regions().unwrap().get(0);
        assert_eq!(r.storage(), StorageMode::Linked);
        assert_eq!(r.chunks().unwrap().len(), 1);
        let enc = r.encryption().unwrap();
        assert!(!enc.is_public());
        assert_eq!(enc.identity_kind(), 0);
        assert_eq!(enc.plaintext_blake3().unwrap().bytes(), &[4u8; 32]);
    }
}
