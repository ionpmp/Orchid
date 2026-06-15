//! Managed folders — automatic deduplication of tracked files.
//!
//! ## MVP trade-off
//!
//! A managed folder has every file mirrored into the content-addressed
//! [`orchid_crypto::ChunkStore`]. **Orchid leaves the original files on
//! disk** so external tools (Explorer, text editors, Git, backup software)
//! keep seeing regular files. That means on-disk savings only kick in when
//! the same content recurs across files or folders; single-copy files
//! consume storage twice (once on disk, once as chunks).
//!
//! The full reflink / NTFS-hardlink strategy that removes the redundant
//! copy is planned for v1.x and tracked in the roadmap; it requires careful
//! handling of ReFS / NTFS semantics that is out of scope for MVP.

pub mod config;
pub mod engine;
pub(crate) mod index;

pub use config::{ManagedFolderConfig, ManagedFolderStats};
pub use engine::{
    ManagedFileIngestFailedEvent, ManagedFileIngestStartedEvent, ManagedFileIngestedEvent,
    ManagedFolderEngine,
};
