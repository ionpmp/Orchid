//! Async byte streaming between the PTY and user code.

use std::io::{Read, Write};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;

use bytes::{BufMut, Bytes, BytesMut};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::error::{Result, TerminalError};
use crate::pty::PtyHandle;

/// Byte-streaming I/O loops around a [`PtyHandle`].
pub struct PtyIo {
    /// Background reader task.
    pub reader_handle: JoinHandle<()>,
    /// Background writer task (one long-lived blocking thread).
    pub writer_handle: JoinHandle<()>,
    /// Send queue used by user code to push keystrokes to the PTY.
    pub writer_tx: std_mpsc::Sender<Bytes>,
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
    // Sync channel so one blocking writer thread can `recv` without
    // `spawn_blocking` per keystroke.
    let (writer_tx, writer_rx) = std_mpsc::channel::<Bytes>();

    let reader_handle = tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        // Growable buffer: reserve → read into spare capacity → freeze the
        // written prefix. Remaining capacity is reused on the next iteration
        // instead of allocating a fresh `Bytes` via `copy_from_slice`.
        let mut buf = BytesMut::with_capacity(8 * 1024);
        loop {
            buf.reserve(8 * 1024);
            let n = {
                let spare = buf.spare_capacity_mut();
                // SAFETY: `Read::read` initialises the first `n` bytes it
                // reports; we only `advance_mut` by that count.
                let read_buf = unsafe {
                    std::slice::from_raw_parts_mut(spare.as_mut_ptr().cast::<u8>(), spare.len())
                };
                match reader.read(read_buf) {
                    Ok(0) => {
                        debug!("pty reader hit EOF");
                        return;
                    }
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        warn!(error = %e, "pty reader error");
                        return;
                    }
                }
            };
            // SAFETY: `n` bytes were written by `read` above.
            unsafe {
                buf.advance_mut(n);
            }
            let chunk = buf.split_to(n).freeze();
            if bytes_tx.send(chunk).is_err() {
                debug!("pty reader: receiver dropped");
                break;
            }
        }
    });

    let writer_handle = tokio::task::spawn_blocking(move || {
        let mut writer = writer;
        while let Ok(chunk) = writer_rx.recv() {
            if let Err(e) = writer.write_all(&chunk).and_then(|()| writer.flush()) {
                warn!(error = %e, "pty write failed");
                break;
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
