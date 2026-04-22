//! Spawn a child process inside a freshly-created PTY.

use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use portable_pty::native_pty_system;

use crate::backend::BackendSpec;
use crate::error::{Result, TerminalError};
use crate::pty::{PtyHandle, PtySize};

/// Spawn a child inside a PTY sized to `size`.
///
/// Returns an `Arc<PtyHandle>` that owns both master and child. The slave
/// end is dropped inside so EOF propagates cleanly when the child exits.
///
/// # Errors
///
/// * [`TerminalError::Pty`] if the OS cannot create a PTY.
/// * [`TerminalError::SpawnFailed`] if the child can't be spawned.
pub fn spawn(spec: &BackendSpec, size: PtySize) -> Result<Arc<PtyHandle>> {
    let system = native_pty_system();
    let pair = system
        .openpty(size.into())
        .map_err(|e| TerminalError::Pty(e.to_string()))?;
    let builder = spec.to_command()?;

    let child = pair
        .slave
        .spawn_command(builder)
        .map_err(|e| TerminalError::SpawnFailed(e.to_string()))?;

    // Drop the slave side so reads on the master observe EOF when the child
    // closes its stdio.
    drop(pair.slave);

    // NOTE: a Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) would
    // guarantee that every descendant dies when Orchid exits. We deferred
    // that plumbing — the `windows` crate's Job-Object surface changes often
    // between versions, and `portable-pty`'s own `child.kill()` plus our
    // explicit teardown covers the common case. See TODOs in the crate
    // README.

    Ok(Arc::new(PtyHandle {
        child: Mutex::new(child),
        master: Mutex::new(pair.master),
        size: RwLock::new(size),
        started_at: chrono::Utc::now(),
    }))
}
