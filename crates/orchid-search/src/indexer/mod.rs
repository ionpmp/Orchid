//! Live indexer: scheduler, FS-event subscriber, bootstrap crawl, extractors.

pub mod crawl;
pub mod extract;
pub mod scheduler;
pub mod watcher;

pub use crawl::crawl_roots;
pub use scheduler::{IndexScheduler, IndexTask};
pub use watcher::{IndexFsSubscriber, IndexScope};
