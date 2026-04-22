//! Async byte streaming between the PTY and user code.

use std::io::{Read, Write};
use std::sync::Arc;

use parking_lot::Mutex;

use bytes::Bytes;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::error::{Result, TerminalError};
use crate::pty::PtyHandle;

/// Byte-streaming I/O loops around a [`PtyHandle`].
pub struct PtyIo {
    /// Background reader task.
    pub reader_handle: JoinHandle<()>,
    /// Background writer task.
    pub writer_handle: JoinHandle<()>,
    /// Send queue used by user code to push keystrokes to the PTY.
    pub writer_tx: mpsc::UnboundedSender<Bytes>,
    /// Byte chunks streamed from the PTY.
    pub bytes_rx: mpsc::UnboundedReceiver<Bytes>,
}

impl std::fmt::Debug for PtyIo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyIo").finish_non_exhaustive()
    }
}

impl PtyIo {
    /// Gracefully drop the write side, then await both background tasks.
    pub async fn shutdown(self) {
        // Dropping the sender triggers EOF on the writer loop.
        drop(self.writer_tx);
        let _ = self.writer_handle.await;
        // Reader usually terminates on EOF when the child exits; if it
        // hasn't, give it a beat and abort.
        let abort = self.reader_handle.abort_handle();
        tokio::select! {
            _ = &mut { self.reader_handle } => {},
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                abort.abort();
            }
        }
    }
}

/// Spin up the reader + writer tasks for a [`PtyHandle`].
///
/// # Errors
///
/// [`TerminalError::Pty`] when the underlying `portable-pty` API rejects the
/// master-clone operations used internally.
pub fn start_io(handle: Arc<PtyHandle>) -> Result<PtyIo> {
    // Clone reader and writer from the master. `portable-pty` supports
    // independent reader / writer handles; we own both from here on.
    let (reader, writer) = {
        let master = handle.master.lock();
        let reader = master
            .try_clone_reader()
            .map_err(|e| TerminalError::Pty(e.to_string()))?;
        let writer = master
            .take_writer()
            .map_err(|e| TerminalError::Pty(e.to_string()))?;
        (reader, writer)
    };
    let _ = handle; // `handle` is still used by the spawn site via Arc.

    let (bytes_tx, bytes_rx) = mpsc::unbounded_channel::<Bytes>();
    let (writer_tx, mut writer_rx) = mpsc::unbounded_channel::<Bytes>();

    let reader_handle = tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let mut buf = vec![0u8; 8 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    debug!("pty reader hit EOF");
                    break;
                }
                Ok(n) => {
                    let chunk = Bytes::copy_from_slice(&buf[..n]);
                    if bytes_tx.send(chunk).is_err() {
                        debug!("pty reader: receiver dropped");
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    warn!(error = %e, "pty reader error");
                    break;
                }
            }
        }
    });

    // The writer lives in a parking_lot::Mutex so we can move into closures
    // run on spawn_blocking; write calls themselves are sync.
    let writer = Arc::new(Mutex::new(writer));
    let writer_handle = tokio::spawn(async move {
        while let Some(chunk) = writer_rx.recv().await {
            let writer = Arc::clone(&writer);
            let bytes = chunk;
            let result = tokio::task::spawn_blocking(move || -> Result<()> {
                let mut w = writer.lock();
                w.write_all(&bytes)
                    .map_err(|e| TerminalError::WriteFailed(e.to_string()))?;
                w.flush()
                    .map_err(|e| TerminalError::WriteFailed(e.to_string()))?;
                Ok(())
            })
            .await;
            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => warn!(error = %e, "pty write failed"),
                Err(e) => warn!(error = %e, "pty write task panicked"),
            }
        }
        debug!("pty writer shutting down");
    });

    Ok(PtyIo {
        reader_handle,
        writer_handle,
        writer_tx,
        bytes_rx,
    })
}
