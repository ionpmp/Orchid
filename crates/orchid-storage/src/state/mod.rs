//! Persistent state store built on top of [`redb`].
//!
//! Everything user-visible is re-exported through [`crate`] — this module
//! is primarily an organisational boundary for the implementation.

pub mod codec;
pub mod database;
pub mod migrations;
pub mod tables;
pub mod types;

pub use codec::Value;
pub use database::{ReadTransaction, StateStore, WriteTransaction};
pub use migrations::{
    available_migrations, Migration, CURRENT_SCHEMA_VERSION,
};
pub use types::{
    CacheEntry, CacheKind, ColorLabel, FileManagerTab, FileTag, GridPosition, HistoryEntry,
    LifecycleState, SchemaMeta, SessionState, TerminalBackend, TerminalSession, ViewMode,
    WidgetInstance, WidgetSize, Workspace,
};
