//! Session lifecycle: open, run, close.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use parking_lot::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::backend::BackendSpec;
use crate::emulator::TerminalEmulator;
use crate::error::{Result, TerminalError};
use crate::events::{TerminalClosed, TerminalExited, TerminalOpened, TerminalOutput};
use crate::input::InputEncoder;
use crate::pty::{self, PtyIo, PtySize};
use crate::session::{PtyIoShell, SessionState, TerminalSession};

/// How often `TerminalOutput` events are allowed to fire per session.
const OUTPUT_EVENT_THROTTLE: Duration = Duration::from_millis(16);

impl TerminalSession {
    /// Spawn a new session with the given backend and initial size.
    ///
    /// # Errors
    ///
    /// Propagates PTY and spawn errors.
    pub async fn open(
        spec: BackendSpec,
        size: PtySize,
        bus: Arc<orchid_core::EventBus>,
    ) -> Result<Arc<Self>> {
        let id = Uuid::new_v4();
        // `portable_pty` spawn can block for a long time on some Windows setups
        // (AV hooks, first-run profile work). Run it off the async executor so
        // other tasks — including test timeouts and the snapshot pump — stay
        // schedulable.
        let spec_spawn = spec.clone();
        let pty = tokio::task::spawn_blocking(move || pty::spawn(&spec_spawn, size))
            .await
            .map_err(|e| TerminalError::SpawnFailed(format!("pty spawn join: {e}")))?;
        let pty = pty?;
        let io = pty::start_io(Arc::clone(&pty))?;
        let emulator = Arc::new(TerminalEmulator::new(
            size.cols,
            size.rows,
            crate::emulator::DEFAULT_SCROLLBACK,
            Arc::clone(&bus),
            id,
        ));
        let encoder = Arc::new(RwLock::new(InputEncoder::new()));
        let initial_command = spec.initial_command.clone();
        let created_at = chrono::Utc::now();
        let state = RwLock::new(SessionState::Starting);

        let PtyIo {
            reader_handle,
            writer_handle,
            writer_tx,
            bytes_rx,
        } = io;
        let io_shell = PtyIoShell {
            reader_handle: Mutex::new(Some(reader_handle)),
            writer_handle: Mutex::new(Some(writer_handle)),
            writer_tx,
        };
        let session = Arc::new(Self {
            id,
            spec: spec.clone(),
            pty: Arc::clone(&pty),
            io: Mutex::new(Some(io_shell)),
            emulator: Arc::clone(&emulator),
            encoder,
            created_at,
            state,
            reader_task: Mutex::new(None),
            shutting_down: AtomicBool::new(false),
        });

        // Start the reader → emulator pump. The pump owns `bytes_rx`
        // directly so it can block on recv without fighting the close path
        // for the `io` mutex.
        let reader_task =
            spawn_reader_task(Arc::clone(&session), Arc::clone(&bus), bytes_rx);
        *session.reader_task.lock() = Some(reader_task);

        *session.state.write() = SessionState::Running;
        bus.publish(
            orchid_core::EventSource::Subsystem("terminal".into()),
            TerminalOpened {
                session_id: id,
                backend: spec.display_name(),
            },
        );

        if let Some(cmd) = initial_command {
            // Feed the startup command as if the user typed it.
            let mut bytes = cmd.into_bytes();
            bytes.push(b'\r');
            session.send_input(&bytes)?;
        }

        Ok(session)
    }

    /// Forward raw bytes to the PTY input.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::SessionClosed`] when the writer has been
    /// torn down; [`TerminalError::WriteFailed`] on send failure.
    pub fn send_input(&self, bytes: &[u8]) -> Result<()> {
        let io = self.io.lock();
        let Some(shell) = io.as_ref() else {
            return Err(TerminalError::SessionClosed);
        };
        shell
            .writer_tx
            .send(Bytes::copy_from_slice(bytes))
            .map_err(|_| TerminalError::WriteFailed("writer channel closed".into()))
    }

    /// Resize both the PTY and the emulator.
    ///
    /// # Errors
    ///
    /// Propagates PTY / emulator errors.
    pub fn resize(&self, size: PtySize) -> Result<()> {
        pty::resize(&self.pty, size)?;
        self.emulator.resize(size.cols, size.rows)?;
        Ok(())
    }

    /// Close the session: drop the writer, terminate the child, drain I/O,
    /// transition to `Closed`, and emit [`TerminalClosed`] on the bus.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation — failures are logged.
    pub async fn close(&self, bus: Arc<orchid_core::EventBus>) -> Result<()> {
        if self.shutting_down.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        debug!(session_id = %self.id, "closing session");

        // Drop writer → child stdin EOF. Joining the reader / writer tasks
        // happens after the child exits.
        let shell_owned = { self.io.lock().take() };
        if let Some(shell) = shell_owned {
            drop(shell.writer_tx);
            let writer = { shell.writer_handle.lock().take() };
            if let Some(h) = writer {
                let _ = h.await;
            }
            let reader = { shell.reader_handle.lock().take() };
            if let Some(h) = reader {
                // Best-effort: if the reader task is still blocked on read,
                // wait up to 200 ms and then abort it.
                let abort = h.abort_handle();
                tokio::select! {
                    _ = h => {}
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {
                        abort.abort();
                    }
                }
            }
        }

        // Wait briefly for the child to exit.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let status = {
                let mut child = self.pty.child.lock();
                child.try_wait().ok().flatten()
            };
            if status.is_some() {
                break;
            }
            if Instant::now() >= deadline {
                warn!(session_id = %self.id, "child did not exit in 2s; killing");
                let mut child = self.pty.child.lock();
                let _ = child.kill();
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let task = { self.reader_task.lock().take() };
        if let Some(task) = task {
            let abort = task.abort_handle();
            tokio::select! {
                _ = task => {}
                _ = tokio::time::sleep(Duration::from_millis(300)) => {
                    abort.abort();
                }
            }
        }

        *self.state.write() = SessionState::Closed;

        // Emit `TerminalClosed` idempotently — the reader task may already
        // have published one on child exit; a second publish is harmless
        // since subscribers key off the session id.
        bus.publish(
            orchid_core::EventSource::Subsystem("terminal".into()),
            TerminalClosed {
                session_id: self.id,
            },
        );
        Ok(())
    }
}

fn spawn_reader_task(
    session: Arc<TerminalSession>,
    bus: Arc<orchid_core::EventBus>,
    mut bytes_rx: tokio::sync::mpsc::UnboundedReceiver<Bytes>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_event = Instant::now() - OUTPUT_EVENT_THROTTLE;
        loop {
            if session.shutting_down.load(Ordering::SeqCst) {
                break;
            }
            tokio::select! {
                chunk = bytes_rx.recv() => {
                    match chunk {
                        Some(chunk) => {
                            let response = session.emulator.feed(&chunk);
                            if !response.is_empty() {
                                let _ = session.send_input(&response);
                            }
                            if last_event.elapsed() >= OUTPUT_EVENT_THROTTLE {
                                bus.publish(
                                    orchid_core::EventSource::Subsystem("terminal".into()),
                                    TerminalOutput { session_id: session.id },
                                );
                                last_event = Instant::now();
                            }
                        }
                        None => break,
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    // Periodic tick to check whether the child has exited.
                    let exited = {
                        let mut child = session.pty.child.lock();
                        child.try_wait().ok().flatten()
                    };
                    if let Some(status) = exited {
                        let code = status.exit_code() as i32;
                        *session.state.write() = SessionState::Exited(code);
                        bus.publish(
                            orchid_core::EventSource::Subsystem("terminal".into()),
                            TerminalExited {
                                session_id: session.id,
                                exit_code: code,
                            },
                        );
                        // Drain any remaining buffered bytes before we emit
                        // the close event.
                        while let Ok(chunk) = bytes_rx.try_recv() {
                            let _ = session.emulator.feed(&chunk);
                        }
                        bus.publish(
                            orchid_core::EventSource::Subsystem("terminal".into()),
                            TerminalClosed { session_id: session.id },
                        );
                        break;
                    }
                }
            }
        }
    })
}
