//! Live indexer: scheduler, FS-event subscriber, content extractor dispatch.

pub mod extract;
pub mod scheduler;
pub mod watcher;

pub use scheduler::{IndexScheduler, IndexTask};
pub use watcher::{IndexFsSubscriber, IndexScope};
