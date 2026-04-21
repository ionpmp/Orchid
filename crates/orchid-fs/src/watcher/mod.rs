//! File-system watcher that turns provider change events into
//! [`orchid_core::EventBus`] messages.

pub mod events;
pub mod fs_watcher;

pub use events::{FsCreatedEvent, FsDeletedEvent, FsModifiedEvent, FsRenamedEvent};
pub use fs_watcher::{FileWatcher, WatchHandle};
