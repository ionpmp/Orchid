//! Resize coordination for an already-spawned PTY.

use tracing::debug;

use crate::error::{Result, TerminalError};
use crate::pty::{PtyHandle, PtySize};

/// Resize `handle` to `new_size`, updating both the kernel PTY and the
/// cached `PtySize`.
///
/// # Errors
///
/// * [`TerminalError::InvalidResize`] when either dimension is zero.
/// * [`TerminalError::Pty`] when the underlying resize syscall fails.
pub fn resize(handle: &PtyHandle, new_size: PtySize) -> Result<()> {
    if new_size.cols == 0 || new_size.rows == 0 {
        return Err(TerminalError::InvalidResize {
            cols: new_size.cols,
            rows: new_size.rows,
        });
    }
    {
        let master = handle.master.lock();
        master
            .resize(new_size.into())
            .map_err(|e| TerminalError::Pty(e.to_string()))?;
    }
    *handle.size.write() = new_size;
    debug!(cols = new_size.cols, rows = new_size.rows, "pty resized");
    Ok(())
}
