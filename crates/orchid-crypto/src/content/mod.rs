//! Content-addressed storage primitives.
//!
//! * [`hash`] — BLAKE3 hashing (mmap + streaming).
//! * [`chunker`] — FastCDC content-defined chunking.
//! * [`store`] — disk + redb refcount table (chunk store).
//! * [`deduplicator`] — glues the three together into ingest / reconstruct.

pub mod chunker;
pub mod deduplicator;
pub mod hash;
pub mod store;

pub use chunker::{Chunk, Chunker, ChunkerConfig};
pub use deduplicator::{ChunkRef, DedupStats, Deduplicator, FileManifest};
pub use hash::{from_hex, hash_bytes, hash_file, hex, StreamHasher};
pub use store::{ChunkRefInfo, ChunkStore, Clock, FixedClock, GcStats, SystemClock};
