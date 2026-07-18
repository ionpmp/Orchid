//! Persistent config for the processes widget.

#![allow(missing_docs)]

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::widget::payloads::{ProcessSortColumn, ProcessesTab};

/// Persistent processes-widget config.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ProcessesConfig {
    /// Refresh interval in seconds (minimum 1).
    pub refresh_interval_seconds: u32,
    /// Default tab on open (`0` Processes … `3` Users).
    pub default_tab: u8,
    /// Show Apps / Background / Windows group headers.
    pub show_grouping: bool,
    /// Last search query.
    pub search_query: String,
    /// Sort column index.
    pub sort_column: u8,
    /// Sort descending.
    pub sort_descending: bool,
}

impl Default for ProcessesConfig {
    fn default() -> Self {
        Self {
            // Keep UI cost down: each refresh updates a large Slint row list.
            refresh_interval_seconds: 5,
            default_tab: ProcessesTab::Processes as u8,
            show_grouping: true,
            search_query: String::new(),
            sort_column: ProcessSortColumn::Cpu as u8,
            sort_descending: true,
        }
    }
}

impl ProcessesConfig {
    /// Active tab from config.
    #[must_use]
    pub fn tab(&self) -> ProcessesTab {
        ProcessesTab::from_index(i32::from(self.default_tab))
    }

    /// Sort column from config.
    #[must_use]
    pub fn sort(&self) -> ProcessSortColumn {
        ProcessSortColumn::from_index(i32::from(self.sort_column))
    }
}
