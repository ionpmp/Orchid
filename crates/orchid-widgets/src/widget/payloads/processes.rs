//! Payload for the processes (Task Manager) widget.

#![allow(missing_docs)]

/// Active tab in the processes widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ProcessesTab {
    /// Running processes list.
    #[default]
    Processes = 0,
    /// Windows services.
    Services = 1,
    /// Startup entries.
    Startup = 2,
    /// User sessions.
    Users = 3,
}

impl ProcessesTab {
    /// Parse from UI tab index.
    #[must_use]
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Self::Services,
            2 => Self::Startup,
            3 => Self::Users,
            _ => Self::Processes,
        }
    }

    /// Tab index for Slint.
    #[must_use]
    pub fn as_index(self) -> i32 {
        self as i32
    }
}

/// Process grouping bucket (Task Manager style).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProcessGroup {
    /// Apps with a visible window / interactive session.
    Apps = 0,
    /// Background processes.
    Background = 1,
    /// Windows / system processes.
    Windows = 2,
}

/// Sort column for the processes table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ProcessSortColumn {
    /// Process name.
    #[default]
    Name = 0,
    /// PID.
    Pid = 1,
    /// CPU percent.
    Cpu = 2,
    /// Working set memory.
    Memory = 3,
    /// Disk / I/O rate (combined).
    Io = 4,
    /// User name.
    User = 5,
    /// Executable path.
    Path = 6,
    /// Status text.
    Status = 7,
}

impl ProcessSortColumn {
    /// Parse from UI column index.
    #[must_use]
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Self::Pid,
            2 => Self::Cpu,
            3 => Self::Memory,
            4 => Self::Io,
            5 => Self::User,
            6 => Self::Path,
            7 => Self::Status,
            _ => Self::Name,
        }
    }

    /// Column index for Slint.
    #[must_use]
    pub fn as_index(self) -> i32 {
        self as i32
    }
}

/// Render-ready processes widget payload.
#[derive(Debug, Clone)]
pub struct ProcessesPayload {
    /// Active tab.
    pub tab: ProcessesTab,
    /// Current search filter.
    pub search_query: String,
    /// Sort column.
    pub sort_column: ProcessSortColumn,
    /// Sort descending when `true`.
    pub sort_descending: bool,
    /// Selected process PID (0 = none).
    pub selected_pid: u32,
    /// Selected service name (empty = none).
    pub selected_service: String,
    /// Selected startup id (empty = none).
    pub selected_startup: String,
    /// Selected session id (`u32::MAX` = none).
    pub selected_session: u32,
    /// Filtered / sorted process rows for the Processes tab.
    pub processes: Vec<ProcessRowView>,
    /// Service rows.
    pub services: Vec<ServiceRowView>,
    /// Startup rows.
    pub startups: Vec<StartupRowView>,
    /// User session rows.
    pub users: Vec<UserRowView>,
    /// `true` until the first process sample is available.
    pub is_loading: bool,
    /// Optional status / error message key or text for the UI toast line.
    pub status_message: String,
    /// Whether process grouping headers should be shown.
    pub show_grouping: bool,
}

/// One process row.
#[derive(Debug, Clone)]
pub struct ProcessRowView {
    pub pid: u32,
    pub name: String,
    pub status: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub memory_text: String,
    pub io_read_bps: u64,
    pub io_write_bps: u64,
    pub io_text: String,
    pub user: String,
    pub path: String,
    pub group: ProcessGroup,
    pub parent_pid: Option<u32>,
    pub session_id: Option<u32>,
    /// `true` when this row is a group header, not a process.
    pub is_group_header: bool,
    /// Header label when [`Self::is_group_header`].
    pub group_label: String,
}

/// One Windows service row.
#[derive(Debug, Clone)]
pub struct ServiceRowView {
    pub name: String,
    pub display_name: String,
    pub status: String,
    pub status_code: u32,
    pub start_type: String,
    pub pid: u32,
    pub can_start: bool,
    pub can_stop: bool,
}

/// One startup entry row.
#[derive(Debug, Clone)]
pub struct StartupRowView {
    /// Stable id (`registry:hkcu:Run:Name` or `folder:path`).
    pub id: String,
    pub name: String,
    pub command: String,
    pub location: String,
    pub enabled: bool,
    pub can_toggle: bool,
}

/// One user session row.
#[derive(Debug, Clone)]
pub struct UserRowView {
    pub session_id: u32,
    pub user_name: String,
    pub state: String,
    pub process_count: u32,
    pub memory_bytes: u64,
    pub memory_text: String,
}
