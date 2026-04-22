//! PTY wrapper: spawn + async I/O + resize.

pub mod io;
pub mod resize;
pub mod spawn;

use std::sync::Arc;

use parking_lot::{Mutex, RwLock};

pub use io::{start_io, PtyIo};
pub use resize::resize;
pub use spawn::spawn;

/// Size of a PTY measured in both cells and pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    /// Number of columns.
    pub cols: u16,
    /// Number of rows.
    pub rows: u16,
    /// Pixel width (optional; some programs use it for images).
    pub pixel_width: u16,
    /// Pixel height (optional).
    pub pixel_height: u16,
}

impl PtySize {
    /// Default 80 × 24 with zero pixel dimensions.
    #[must_use]
    pub const fn default_80x24() -> Self {
        Self {
            cols: 80,
            rows: 24,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

impl From<PtySize> for portable_pty::PtySize {
    fn from(s: PtySize) -> Self {
        portable_pty::PtySize {
            rows: s.rows,
            cols: s.cols,
            pixel_width: s.pixel_width,
            pixel_height: s.pixel_height,
        }
    }
}

/// Owning wrapper around a portable-pty child + master, plus our cached size.
pub struct PtyHandle {
    /// Child process handle. Locked to coordinate `try_wait` / `kill`.
    pub child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
    /// Master side of the PTY.
    pub master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    /// Cached size (kept in sync with the kernel side).
    pub size: RwLock<PtySize>,
    /// When the child was started, for diagnostics.
    pub started_at: chrono::DateTime<chrono::Utc>,
}

impl std::fmt::Debug for PtyHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyHandle")
            .field("size", &*self.size.read())
            .field("started_at", &self.started_at)
            .finish_non_exhaustive()
    }
}

/// Shared alias — PTY handles are always held behind `Arc`.
pub type SharedPty = Arc<PtyHandle>;
