//! Terminal subsystem for Orchid: PTY backends, VT emulation, sessions, and
//! layouts.
//!
//! # Architecture
//!
//! * [`backend`] — shell / WSL / SSH launch specs.
//! * [`pty`] — thin, async-friendly wrapper around `portable-pty`.
//! * [`emulator`] — VT / ANSI state machine (vte-based custom implementation).
//! * [`input`] — key + paste + mouse → PTY byte encoder.
//! * [`session`] — end-to-end lifecycle: spawn + emulator + input pump.
//! * [`layout`] — tab / split tree data model (UI-agnostic).

#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

pub mod backend;
pub mod emulator;
pub mod error;
pub mod events;
pub mod input;
pub mod layout;
pub mod metrics;
pub mod pty;
pub mod search;
pub mod session;

pub use backend::{BackendKind, BackendSpec, SshTarget};
pub use emulator::{
    resolve_color, xterm_256_color, Cell, CellColor, CellFlags, ColorRole, CursorState,
    CursorStyle, GridLine, GridPoint, GridSnapshot, Rgba, ScrollPosition, Selection,
    TerminalEmulator, TerminalPalette,
};
pub use error::{Result, TerminalError};
pub use events::{
    TerminalBell, TerminalClosed, TerminalCrashed, TerminalCwdChanged, TerminalExited,
    TerminalOpened, TerminalOutput, TerminalTitleChanged,
};
pub use input::{InputEncoder, MouseAction, MouseButtonReport, MouseMode};
pub use layout::{
    focus_next_in_tab, focus_previous_in_tab, focused_session, LayoutRoot, LayoutSnapshot,
    PaneBounds, PaneSnapshot, SplitDirection, SplitNode, Tab, TabSet, TabSnapshot,
};
pub use metrics::FontMetrics;
pub use pty::{spawn, PtyHandle, PtyIo, PtySize};
pub use search::SearchMatch;
pub use session::{SessionManager, SessionState, TerminalSession};

/// Crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
