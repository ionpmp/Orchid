//! Native `.orchid` container format (`application/vnd.orchid`).
//!
//! Phase 1 sealed framing: [`write_sealed_file`] / [`SealedFile`]. Spec:
//! [`docs/ORCHID_FORMAT.md`](../../../docs/ORCHID_FORMAT.md).

#![warn(missing_docs)]
#![warn(clippy::all)]
#![deny(unsafe_code)]
// Aggregate errors include orchid_crypto::CryptoError; boxing on every path
// would allocate for no benefit (same rationale as orchid-crypto).
#![allow(clippy::result_large_err)]

mod compress;
mod content_type;
mod crdt;
mod crypto_region;
mod embedding;
mod error;
mod framing;
mod linked;
mod provenance;
mod reader;
mod toc_build;
mod writer;

#[allow(dead_code)]
#[allow(missing_docs)]
#[allow(unsafe_code)]
#[allow(clippy::all)]
#[allow(clippy::pedantic)]
#[allow(unused_imports)]
#[allow(clippy::needless_lifetimes)]
#[allow(elided_lifetimes_in_paths)]
#[allow(mismatched_lifetime_syntaxes)]
#[rustfmt::skip]
#[path = "toc_generated.rs"]
mod toc_generated;

pub use compress::{compress_zstd, decompress_zstd};
pub use content_type::{STRUCTURED_CRDT_V1, STRUCTURED_SNAPSHOT_V1};
pub use crdt::{
    decode_crdt_payload, encode_crdt_payload, ActorId, CrdtDocument, Op, OpId, CRDT_PAYLOAD_MAGIC,
};
pub use embedding::{
    document_embedding, EmbeddingLevel, EmbeddingPayload, EmbeddingRecord, EMBEDDING_HIER_F32_V1,
    EMBEDDING_MAGIC, EMBEDDING_WIRE_VERSION,
};
pub use crypto_region::{
    decode_region_body, prepare_region_body, PreparedRegionBody, RegionEncryptionSpec,
};
pub use error::{FormatError, Result};
pub use framing::{
    align_up, pad_len, pad_to_alignment, Footer, Header, RegionHeader,
};
pub use linked::{
    build_linked_bytes, linked_region_plaintext, linked_to_sealed, sealed_to_linked,
    write_linked_file, LinkedCreateRequest,
};
pub use provenance::{
    is_c2pa_accepted, sign_clean_text_provenance, verify_provenance_carrier, SignedProvenance,
    CLEAN_TEXT_ASSERTION, PROVENANCE_CONTENT_TYPE,
};
pub use reader::SealedFile;
pub use toc_build::{build_toc, TocChunkSpec, TocRegionSpec, TocSpec};
pub use writer::{build_sealed_bytes, empty_raw, write_sealed_file, SealedCreateRequest};

/// File extension including the leading dot.
pub const EXTENSION: &str = ".orchid";

/// IANA-style vendor MIME type.
pub const MIME_TYPE: &str = "application/vnd.orchid";

/// Header magic and footer terminator (`ORCD`).
pub const FILE_MAGIC: &[u8; 4] = b"ORCD";

/// Region mini-header magic (`ORCR`).
pub const REGION_MAGIC: &[u8; 4] = b"ORCR";

/// Alignment for header, regions, and TOC (4 KiB).
pub const ALIGNMENT: u64 = 4096;

/// Fixed header size after zero-pad (one aligned block).
pub const HEADER_SIZE: u64 = ALIGNMENT;

/// Region mini-header size: magic(4) + type_id(2) + length(8).
pub const REGION_MINI_HEADER_SIZE: usize = 14;

/// Fixed footer size at EOF.
pub const FOOTER_SIZE: usize = 44;

/// Initial format major version carried in the header.
pub const FORMAT_VERSION_MAJOR: u16 = 1;
/// Initial format minor version carried in the header.
pub const FORMAT_VERSION_MINOR: u16 = 0;

/// Region taxonomy ids ([`docs/ORCHID_FORMAT.md`](../../../docs/ORCHID_FORMAT.md) §4).
pub mod region_type {
    /// Opaque media / original bytes.
    pub const RAW: u16 = 0x0001;
    /// UTF-8 plain text (search / TTS / agents).
    pub const CLEAN_TEXT: u16 = 0x0002;
    /// Editor document model snapshot.
    pub const STRUCTURED: u16 = 0x0003;
    /// Hierarchical vectors + token counts.
    pub const EMBEDDING: u16 = 0x0004;
    /// C2PA / attestation stubs.
    pub const PROVENANCE: u16 = 0x0005;
    /// Generation graph for linked mode.
    pub const VERSION_HISTORY: u16 = 0x0006;
}

/// Capability flag bits in the header `capability_flags` field.
pub mod capability {
    /// At least one linked region.
    pub const LINKED: u64 = 1 << 0;
    /// At least one private encrypted region.
    pub const ENCRYPTED: u64 = 1 << 1;
    /// Structured uses a CRDT op log.
    pub const CRDT: u64 = 1 << 2;
    /// Embedding region present.
    pub const EMBEDDINGS: u64 = 1 << 3;
    /// Provenance carries C2PA.
    pub const C2PA: u64 = 1 << 4;
    /// Text regions need a library zstd dictionary.
    pub const ZSTD_DICT: u64 = 1 << 5;
}

/// Generated FlatBuffers accessors for [`schema/orchid_toc.fbs`].
pub mod toc {
    pub use crate::toc_generated::orchid::format::*;
}

/// Returns the crate version string.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_spec() {
        assert_eq!(FILE_MAGIC, b"ORCD");
        assert_eq!(REGION_MAGIC, b"ORCR");
        assert_eq!(ALIGNMENT, 4096);
        assert_eq!(FOOTER_SIZE, 44);
        assert_eq!(REGION_MINI_HEADER_SIZE, 14);
        assert_eq!(MIME_TYPE, "application/vnd.orchid");
        assert_eq!(EXTENSION, ".orchid");
    }
}
