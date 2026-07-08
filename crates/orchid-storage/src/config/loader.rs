//! Load, persist, and reload `config.toml`.
//!
//! All operations here are synchronous; the file-watching, hot-reload path
//! lives in [`super::watcher`].

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::schema::OrchidConfig;
use crate::error::{Result, StorageError};

/// Default TOML config template shipped when no file exists yet.
///
/// Keeping this as a string constant (rather than serialising
/// `OrchidConfig::default()`) lets us include helpful comments for the user.
pub const DEFAULT_CONFIG_TOML: &str = r#"# Orchid configuration
#
# This file is written on first launch and hot-reloaded whenever you save it.
# Unknown keys are preserved on best effort; missing keys fall back to the
# defaults shown below.

[general]
# Automatically check for and download updates.
auto-update = true
# Opt-in anonymous telemetry. Off by default.
telemetry = false
# Start Orchid on user login.
open-on-startup = false

[appearance]
theme = "orchid-dark"
# One of: "touch", "mouse", "hybrid".
density = "hybrid"
# font-family = "Inter"
font-scale = 1.0
reduce-motion = false
follow-system-theme = true
dark-theme = "orchid-dark"
light-theme = "orchid-light"

[input]
# "left" or "right".
primary-hand = "right"
mirror-edge-swipes = false
haptic-feedback = true
palm-rejection = true
# One of: "none", "switch-tool", "erase".
pen-double-tap-action = "switch-tool"

[shortcuts]
# Override built-in keyboard shortcuts by command id.
# Example:
# overrides = { command-palette = "Ctrl+Shift+P" }
overrides = {}
# Leader-key mode: press leader-key, then a letter within leader-timeout-ms.
leader-key = "Ctrl+Shift+Space"
leader-timeout-ms = 1200
# leader-key = "" disables leader mode.
# leader-bindings = { p = "command-palette", s = "settings.open" }

[locale]
# BCP 47 language tag.
language = "en-US"
# date-format = "yyyy-MM-dd"
# time-format = "HH:mm"
# 0 = Sunday, 1 = Monday.
first-day-of-week = 1

[privacy]
record-action-history = true
history-retention-days = 90
clear-clipboard-seconds = 30

[onboarding]
# Set to true after the first-run tour is completed or skipped.
completed = false
# Show subtle gesture hints on the workspace and dock (Win+? toggles at runtime).
hint-mode-enabled = false

# [file-manager]
# Remote mounts shown under the file manager Network sidebar.
# Use Orchid path syntax (`sftp:host/path`) or a URL (`sftp://host/path`).
# [[file-manager.network-mounts]]
# name = "Home SFTP"
# uri = "sftp://myserver/home/alice"
# user = "alice"
# password = "secret"
# rclone-remote = "myserver"
# enabled = true
"#;

/// Load / save / reload API for the TOML configuration.
///
/// Implemented as an empty struct with associated functions so that callers
/// don't have to thread mutable state around.
#[derive(Debug)]
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load the configuration at `path`, creating a default file first if it
    /// does not already exist.
    ///
    /// # Errors
    ///
    /// See [`ConfigLoader::load`] — a newly created file always parses, so
    /// the only extra failure mode is I/O when writing the default.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_storage::ConfigLoader;
    /// let tmp = tempfile::tempdir().unwrap();
    /// let path = tmp.path().join("config.toml");
    /// let cfg = ConfigLoader::load_or_create(&path).unwrap();
    /// assert!(path.exists());
    /// assert_eq!(cfg.locale.language, "en-US");
    /// ```
    pub fn load_or_create(path: &Path) -> Result<OrchidConfig> {
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            atomic_write(path, DEFAULT_CONFIG_TOML.as_bytes())?;
        }
        Self::load(path)
    }

    /// Read, parse, and validate the configuration at `path`.
    ///
    /// # Errors
    ///
    /// * [`StorageError::Io`] if the file can't be read.
    /// * [`StorageError::Toml`] if parsing fails.
    /// * [`StorageError::ConfigValidation`] if semantic validation fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_storage::ConfigLoader;
    /// # let tmp = tempfile::tempdir().unwrap();
    /// # let path = tmp.path().join("config.toml");
    /// # ConfigLoader::load_or_create(&path).unwrap();
    /// let cfg = ConfigLoader::load(&path).unwrap();
    /// # let _ = cfg;
    /// ```
    pub fn load(path: &Path) -> Result<OrchidConfig> {
        let text = fs::read_to_string(path)?;
        let cfg: OrchidConfig = toml::from_str(&text).map_err(|source| StorageError::Toml {
            path: path.to_path_buf(),
            source,
        })?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Atomically overwrite the file at `path` with `config`.
    ///
    /// Implementation writes to `<path>.tmp` first, then renames over the
    /// target. Readers never observe a partially-written file.
    ///
    /// # Errors
    ///
    /// * [`StorageError::TomlSerialize`] if serialisation fails.
    /// * [`StorageError::Io`] if the temp file or rename fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_storage::{ConfigLoader, OrchidConfig};
    /// let tmp = tempfile::tempdir().unwrap();
    /// let path = tmp.path().join("config.toml");
    /// ConfigLoader::save(&OrchidConfig::default(), &path).unwrap();
    /// ```
    pub fn save(config: &OrchidConfig, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(config)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_write(path, text.as_bytes())
    }

    /// Re-read the configuration at `path`. Equivalent to [`Self::load`]; the
    /// separate name documents intent at call sites that react to file
    /// changes.
    ///
    /// # Errors
    ///
    /// Same as [`Self::load`].
    pub fn reload(path: &Path) -> Result<OrchidConfig> {
        Self::load(path)
    }
}

/// Write `bytes` to `path` atomically via a same-directory temp file.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp_path: PathBuf = match path.extension() {
        Some(ext) => {
            let mut os = ext.to_os_string();
            os.push(".tmp");
            path.with_extension(os)
        }
        None => path.with_extension("tmp"),
    };
    fs::write(&tmp_path, bytes)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::OrchidConfig;

    #[test]
    fn load_or_create_writes_default_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        assert!(!path.exists());
        let cfg = ConfigLoader::load_or_create(&path).unwrap();
        assert!(path.exists());
        assert_eq!(cfg.appearance.theme, "orchid-dark");
    }

    #[test]
    fn save_then_load_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        let mut cfg = OrchidConfig::default();
        cfg.appearance.theme = "solarised".to_string();
        cfg.privacy.history_retention_days = 7;

        ConfigLoader::save(&cfg, &path).unwrap();
        let loaded = ConfigLoader::load(&path).unwrap();
        assert_eq!(loaded.appearance.theme, "solarised");
        assert_eq!(loaded.privacy.history_retention_days, 7);
    }

    #[test]
    fn atomic_save_does_not_leave_tmp_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        ConfigLoader::save(&OrchidConfig::default(), &path).unwrap();

        let entries: Vec<_> = fs::read_dir(tmp.path()).unwrap().collect();
        assert_eq!(entries.len(), 1, "only final config.toml should remain");
    }

    #[test]
    fn load_surfaces_parse_errors_with_path() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(&path, "this is not = [valid toml").unwrap();
        let err = ConfigLoader::load(&path).unwrap_err();
        match err {
            StorageError::Toml { path: p, .. } => assert_eq!(p, path),
            other => panic!("expected Toml error, got {other:?}"),
        }
    }

    #[test]
    fn load_rejects_semantically_invalid_config() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(
            &path,
            "[appearance]\nfont-scale = 10.0\n",
        )
        .unwrap();
        let err = ConfigLoader::load(&path).unwrap_err();
        assert!(matches!(err, StorageError::ConfigValidation(_)));
    }
}
