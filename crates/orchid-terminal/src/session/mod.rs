//! Terminal sessions: backend + PTY + emulator + lifecycle.

pub mod lifecycle;
pub mod manager;
pub mod persistence;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use std::sync::mpsc as std_mpsc;

use bytes::Bytes;
use parking_lot::{Mutex, RwLock};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::backend::BackendSpec;
use crate::emulator::TerminalEmulator;
use crate::input::InputEncoder;
use crate::pty::PtyHandle;

pub use manager::SessionManager;

/// Slimmed-down PTY I/O container used inside [`TerminalSession`].
///
/// We keep only the writer sender and the task join handles here; the
/// byte-receiver is moved into the reader pump at session construction.
pub(crate) struct PtyIoShell {
    pub(crate) reader_handle: Mutex<Option<JoinHandle<()>>>,
    pub(crate) writer_handle: Mutex<Option<JoinHandle<()>>>,
    pub(crate) writer_tx: std_mpsc::Sender<Bytes>,
}

/// Full runtime state of a live terminal session.
pub struct TerminalSession {
    /// Stable session id.
    pub id: Uuid,
    /// Original backend spec used to spawn this session.
    pub spec: BackendSpec,
    /// Shared PTY handle.
    pub pty: Arc<PtyHandle>,
    /// Reader / writer halves. `None` once the session is closing.
    pub(crate) io: Mutex<Option<PtyIoShell>>,
    /// Emulator state.
    pub emulator: Arc<TerminalEmulator>,
    /// Input-mode flags (DECCKM, bracketed paste, mouse).
    pub encoder: Arc<RwLock<InputEncoder>>,
    /// When the session was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Current lifecycle state.
    pub state: RwLock<SessionState>,
    /// Background task that pumps PTY bytes into the emulator.
    pub(crate) reader_task: Mutex<Option<JoinHandle<()>>>,
    /// Set once a close has started, to stop the reader from racing.
    pub(crate) shutting_down: AtomicBool,
}

impl std::fmt::Debug for TerminalSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalSession")
            .field("id", &self.id)
            .field("backend", &self.spec.display_name())
            .field("state", &*self.state.read())
            .finish_non_exhaustive()
    }
}

/// Lifecycle state for a terminal session.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Starting,
    Running,
    /// Child exited normally with this code.
    Exited(i32),
    /// Dropped because of an unrecoverable error; reason tracked via events.
    Crashed,
    Closed,
}
