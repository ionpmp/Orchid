//! User-editable configuration.
//!
//! The configuration lives in a TOML file (by convention `config.toml`
//! resolved through [`crate::OrchidPaths`]) and is hot-reloadable.
//!
//! * [`OrchidConfig`] — typed representation; see [`schema`] for subsections.
//! * [`ConfigLoader`] — synchronous load / save API.
//! * [`ConfigWatcher`] — async, debounced hot-reload bridge.

mod locale_format;

pub mod loader;
pub mod schema;
pub mod watcher;

pub use loader::{ConfigLoader, DEFAULT_CONFIG_TOML};
pub use schema::{
    AppearanceConfig, Density, FileManagerSectionConfig, GeneralConfig, Hand, InputConfig,
    LocaleConfig, NetworkMountConfig, OnboardingConfig, OrchidConfig, PenDoubleTapAction,
    PrivacyConfig, SearchConfig, ShortcutsConfig,
};
pub use watcher::ConfigWatcher;

/// Alias kept for ergonomic `use orchid_storage::Config;` call sites.
pub type Config = OrchidConfig;
