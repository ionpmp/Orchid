//! File operations (copy, move, delete) with progress and cancellation.

pub mod copy;
pub mod delete;
#[path = "move_.rs"]
pub mod move_;
pub mod progress;

pub use copy::{copy, CopyOptions};
pub use delete::{delete, DeleteOptions};
pub use move_::move_;
pub use progress::{OperationProgress, ProgressSink};
