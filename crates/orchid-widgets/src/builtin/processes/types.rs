//! Internal snapshot types for the processes provider.

#![allow(missing_docs)]

use chrono::{DateTime, Utc};

use crate::widget::payloads::ProcessGroup;

/// Raw process sample before filtering / formatting.
#[derive(Debug, Clone)]
pub struct ProcessSample {
    pub pid: u32,
    pub name: String,
    pub status: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
    pub io_read_bps: u64,
    pub io_write_bps: u64,
    pub user: String,
    pub path: String,
    pub parent_pid: Option<u32>,
    pub session_id: Option<u32>,
    pub group: ProcessGroup,
}

/// Provider snapshot of all process samples.
#[derive(Debug, Clone)]
pub struct ProcessesSnapshot {
    pub processes: Vec<ProcessSample>,
    pub captured_at: DateTime<Utc>,
}
