//! Snapshot payload for the protection widget.

#![allow(missing_docs)]

/// One cleaner or one program row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectRow {
    pub id: String,
    /// Fluent key. Empty when [`Self::title`] is already display text.
    pub title_key: String,
    pub title: String,
    /// Fluent key. Empty when [`Self::detail`] is already display text.
    pub detail_key: String,
    pub detail: String,
    /// Fluent key for the size column.
    ///
    /// Empty means format [`Self::bytes`]. `protect-files` uses [`Self::files`].
    pub size_key: String,
    pub files: u64,
    pub bytes: u64,
    pub checked: bool,
}

/// A volume offered for a free-space wipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectDrive {
    pub label: String,
    pub selected: bool,
}

/// Render-ready protection state. Strings that depend on the locale are keys;
/// the UI model translates them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectPayload {
    /// 0 traces, 1 histories, 2 free space, 3 network.
    pub tab: i32,
    pub rows: Vec<ProtectRow>,
    pub drives: Vec<ProtectDrive>,
    /// 1 or 3 overwrite passes.
    pub passes: i32,
    /// 0..=100 while a wipe runs, or -1 when idle.
    pub wipe_percent: i32,
    pub status_key: String,
    pub status_files: u64,
    pub status_bytes: u64,
    pub status_detail: String,
    pub busy: bool,
    /// A scan has finished, so Clean may run.
    pub scanned: bool,
    /// Bytes the checked rows on this tab reported. Zero hides the size on Clean.
    pub clean_bytes: u64,
    pub free_bytes: u64,
}
