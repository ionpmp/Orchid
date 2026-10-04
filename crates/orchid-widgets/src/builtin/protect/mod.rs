//! Privacy protection: trace and history cleanup, free-space overwrite,
//! and per-program outbound firewall blocks.
//!
//! The cleaners follow the split used by BleachBit, CCleaner, and PrivaZer
//! (traces vs. histories vs. cookies). Free-space overwrite follows Eraser /
//! CCleaner Drive Wiper (zeros, a reserved margin, a small pass count).
//! Outbound blocks follow simplewall: one Windows Firewall rule per program,
//! created and removed only under the `Orchid Protect` name prefix.

mod catalog;
mod config;
mod firefox;
mod firewall;
mod fs;
mod platform;
mod resolve;
mod widget;
mod wipe;

pub use catalog::{CleanerId, ProtectTab};
pub use firewall::{
    block_refusal, merge_app_rows, netsh_add_args, netsh_delete_args, parse_firewall_json,
    rule_name, AppRow, BlockRefusal, FirewallRule, RunningApp, ORCHID_RULE_PREFIX,
};
pub use fs::CleanStats;
pub use platform::{firewall_rules_json, run_netsh, PlatformError};
pub use resolve::{clean_cleaner, scan_cleaner, CleanReport, HostLayout};
pub use widget::{
    cancel_wipe, clean, descriptor, refresh_network, scan, select_drive, set_passes, set_tab,
    start_wipe, toggle_row, TYPE_ID,
};
pub use wipe::{
    clamp_passes, filler_directory, volume_key, wipe_budget, wipe_free_space, WipeReport,
    RESERVE_BYTES,
};
