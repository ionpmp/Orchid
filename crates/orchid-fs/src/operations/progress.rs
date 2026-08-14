//! Progress reporting channel used by file operations.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::error::{FsError, Result};
use crate::path::FsPath;

/// Cancellation + pause flags shared with a running copy/move.
#[derive(Clone, Default)]
pub struct TransferControl {
    /// Cancel the operation at the next file/chunk boundary.
    pub cancel: Option<CancellationToken>,
    /// When `true`, the copy loop waits until cleared.
    pub pause: Option<Arc<AtomicBool>>,
}

impl TransferControl {
    /// Wait while paused; error if cancelled.
    pub(crate) async fn wait(&self) -> Result<()> {
        loop {
            if let Some(c) = &self.cancel {
                if c.is_cancelled() {
                    return Err(FsError::Cancelled);
                }
            }
            let paused = self
                .pause
                .as_ref()
                .is_some_and(|p| p.load(Ordering::Relaxed));
            if !paused {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

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
