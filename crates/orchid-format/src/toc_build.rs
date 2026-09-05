//! Build FlatBuffers TOC blobs for sealed writers.

use flatbuffers::FlatBufferBuilder;

use crate::toc::{
    finish_toc_buffer, CompressionCodec, RegionEntry, RegionEntryArgs, RegionType, StorageMode,
    Toc, TocArgs,
};
use crate::Result;

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
    /// Compression applied to the inline payload.
    pub compression: CompressionCodec,
    /// MIME / schema hint.
    pub content_type: Option<String>,
    /// BLAKE3 of the stored payload bytes (after compression, before encryption).
    pub payload_blake3: [u8; 32],
}

/// Logical TOC fields for a sealed Phase 1 file.
#[derive(Debug, Clone)]
pub struct TocSpec {
    /// Generation counter (1 for genesis sealed create).
    pub generation: u64,
    /// Parent generation (`0` if genesis).
    pub parent_generation: u64,
    /// Echo of header UUID.
    pub file_uuid: [u8; 16],
    /// Region rows.
    pub regions: Vec<TocRegionSpec>,
}

/// Encode `spec` as a FlatBuffers root `Toc` buffer.
pub fn build_toc(spec: &TocSpec) -> Result<Vec<u8>> {
    let mut fbb = FlatBufferBuilder::new();
    let mut region_offs = Vec::with_capacity(spec.regions.len());
    for r in &spec.regions {
        let name = r
            .name
            .as_ref()
            .map(|s| fbb.create_string(s.as_str()));
        let content_type = r
            .content_type
            .as_ref()
            .map(|s| fbb.create_string(s.as_str()));
        let payload_blake3 = fbb.create_vector(&r.payload_blake3);
        let off = RegionEntry::create(
            &mut fbb,
            &RegionEntryArgs {
                type_: r.type_,
                offset: r.offset,
                length: r.length,
                name,
                ordinal: r.ordinal,
                compression: r.compression,
                storage: StorageMode::Inline,
                chunks: None,
                encryption: None,
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
            default_storage: StorageMode::Inline,
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
            regions: vec![TocRegionSpec {
                type_: RegionType::CleanText,
                offset: 4096,
                length: 10,
                name: Some("body.txt".into()),
                ordinal: 0,
                compression: CompressionCodec::Zstd,
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
}
