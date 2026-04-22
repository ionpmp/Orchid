//! Bus events emitted by the terminal subsystem.

use std::path::PathBuf;

use orchid_core::Event;
use uuid::Uuid;

/// A new terminal session was started.
#[derive(Debug, Clone)]
pub struct TerminalOpened {
    /// Session id.
    pub session_id: Uuid,
    /// Human-readable backend label.
    pub backend: String,
}
impl Event for TerminalOpened {
    fn event_type() -> &'static str {
        "terminal.opened"
    }
}

/// The PTY produced output; the UI should re-snapshot.
#[derive(Debug, Clone)]
pub struct TerminalOutput {
    /// Session id.
    pub session_id: Uuid,
}
impl Event for TerminalOutput {
    fn event_type() -> &'static str {
        "terminal.output"
    }
}

/// Window title changed (via OSC 0 / 2).
#[derive(Debug, Clone)]
pub struct TerminalTitleChanged {
    /// Session id.
    pub session_id: Uuid,
    /// New title.
    pub title: String,
}
impl Event for TerminalTitleChanged {
    fn event_type() -> &'static str {
        "terminal.title_changed"
    }
}

/// BEL received.
#[derive(Debug, Clone)]
pub struct TerminalBell {
    /// Session id.
    pub session_id: Uuid,
}
impl Event for TerminalBell {
    fn event_type() -> &'static str {
        "terminal.bell"
    }
}

/// The child process exited normally.
#[derive(Debug, Clone)]
pub struct TerminalExited {
    /// Session id.
    pub session_id: Uuid,
    /// Process exit code.
    pub exit_code: i32,
}
impl Event for TerminalExited {
    fn event_type() -> &'static str {
        "terminal.exited"
    }
}

/// The child process crashed (signal / abnormal exit on non-POSIX platforms).
#[derive(Debug, Clone)]
pub struct TerminalCrashed {
    /// Session id.
    pub session_id: Uuid,
    /// Human-readable reason.
    pub reason: String,
}
impl Event for TerminalCrashed {
    fn event_type() -> &'static str {
        "terminal.crashed"
    }
}

/// The session was closed by us.
#[derive(Debug, Clone)]
pub struct TerminalClosed {
    /// Session id.
    pub session_id: Uuid,
}
impl Event for TerminalClosed {
    fn event_type() -> &'static str {
        "terminal.closed"
    }
}

/// OSC 7 — working directory change.
#[derive(Debug, Clone)]
pub struct TerminalCwdChanged {
    /// Session id.
    pub session_id: Uuid,
    /// New working directory.
    pub cwd: PathBuf,
}
impl Event for TerminalCwdChanged {
    fn event_type() -> &'static str {
        "terminal.cwd_changed"
    }
}
