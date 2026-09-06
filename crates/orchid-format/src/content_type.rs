//! Content-type strings for Structured (and related) regions.

/// Phase 1 immutable snapshot blob.
pub const STRUCTURED_SNAPSHOT_V1: &str = "orchid.structured.snapshot.v1";

/// Phase 4 CRDT document (compaction snapshot + append-only op log).
pub const STRUCTURED_CRDT_V1: &str = "orchid.structured.crdt.v1";
