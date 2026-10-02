//! Managed folders — automatic deduplication of tracked files.
//!
//! Each tracked file is still recorded in the content-addressed
//! [`orchid_crypto::ChunkStore`]. When the volume can share extents, the
//! chunk file is a block clone of the source range (Windows) or a
//! `copy_file_range` (Linux) instead of a userspace copy. Otherwise the
//! bytes are written as before.
//!
//! Two whole files in the same folder that still hash the same become one
//! hard link. An in-place edit changes every name. A program that saves by
//! writing a new file and renaming it over the path breaks the link, and
//! the next ingest stores that file on its own. The chunk store keeps the
//! content either way, so deleting a name does not drop the only copy.

mod clone;
pub mod config;
pub mod engine;
pub(crate) mod index;
mod link;
pub mod policy;

pub use clone::try_clone_range;
pub use config::{ManagedFolderConfig, ManagedFolderStats};
pub use engine::{
    ManagedFileIngestFailedEvent, ManagedFileIngestStartedEvent, ManagedFileIngestedEvent,
    ManagedFolderEngine,
};
pub use link::paths_share_data;
pub use policy::ManagedFolderPolicy;
