//! Storage layer for Orchid: state persistence (`redb`) and configuration (TOML).
//!
//! The crate is split into two largely independent subsystems:
//!
//! * [`state`] — typed wrapper around an embedded [`redb`] database
//!   that holds user settings, action history, widget instances, workspaces,
//!   file tags, session state, and caches.
//! * [`config`] — TOML-backed, hot-reloadable configuration used for all
//!   human-editable settings.
//!
//! Filesystem locations for both are resolved through [`OrchidPaths`].
//!
//! # Quick start
//!
//! ```no_run
//! use orchid_storage::{ConfigLoader, OrchidPaths, StateStore};
//!
//! let paths = OrchidPaths::resolve()?;
//! let config = ConfigLoader::load_or_create(&paths.config_file)?;
//! let store = StateStore::open(&paths.state_db_path, env!("CARGO_PKG_VERSION"))?;
//! # let _ = (config, store);
//! # Ok::<(), orchid_storage::StorageError>(())
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
// `StorageError` aggregates several sizeable upstream error types (`toml::de::Error`,
// `redb::*Error`, ...). Boxing them all just to satisfy `clippy::result_large_err`
// would add allocation on every error path without a commensurate benefit — the
// error path is the cold path.
#![allow(clippy::result_large_err)]

pub mod config;
pub mod error;
pub mod paths;
pub mod state;

pub use config::{
    AppearanceConfig, Config, ConfigLoader, ConfigWatcher, Density, FileManagerSectionConfig,
    GeneralConfig, Hand, InputConfig, LocaleConfig, NetworkMountConfig, OnboardingConfig,
    OrchidConfig, PenDoubleTapAction, PrivacyConfig, SearchConfig, ShortcutsConfig,
    DEFAULT_CONFIG_TOML,
};
pub use error::{Result, StorageError};
pub use paths::OrchidPaths;
pub use state::{
    bincode_decode, bincode_encode, CacheEntry, CacheKind, ColorLabel, FileManagerTab, FileTag,
    GridPosition, HistoryEntry, LifecycleState, Migration, NotificationCenterItem,
    NotificationCenterState, ReadTransaction, SchemaMeta, SessionState, StateStore,
    TerminalBackend, TerminalSession, Value, ViewMode, WidgetInstance, WidgetSize, Workspace,
    WriteTransaction, CURRENT_SCHEMA_VERSION, NOTIFICATION_CENTER_CACHE_KEY,
};

/// Returns the version of this crate.
///
/// # Examples
///
/// ```
/// assert!(!orchid_storage::version().is_empty());
/// ```
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }
}
