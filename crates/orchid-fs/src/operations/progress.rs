//! Progress reporting channel used by file operations.

use tokio::sync::mpsc;

use crate::path::FsPath;

/// Snapshot of a long-running file operation.
#[derive(Debug, Clone)]
pub struct OperationProgress {
    /// Total bytes to process (`0` if unknown).
    pub total_bytes: u64,
    /// Bytes processed so far.
    pub processed_bytes: u64,
    /// Path currently being worked on.
    pub current_path: FsPath,
    /// Files processed so far.
    pub items_processed: u64,
    /// Total files in the operation (`0` if unknown).
    pub items_total: u64,
}

/// Send side of the progress channel.
#[derive(Debug, Clone)]
pub struct ProgressSink {
    tx: mpsc::UnboundedSender<OperationProgress>,
}

impl ProgressSink {
    /// Build a channel pair.
    #[must_use]
    pub fn channel() -> (Self, mpsc::UnboundedReceiver<OperationProgress>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    /// Publish a progress snapshot. Drops silently if the receiver is gone.
    pub fn send(&self, progress: OperationProgress) {
        let _ = self.tx.send(progress);
    }
}
