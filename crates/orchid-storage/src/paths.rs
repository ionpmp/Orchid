//! OS-aware filesystem paths used by Orchid.
//!
//! [`OrchidPaths`] is the single source of truth for where configuration,
//! data, caches, and logs live on disk. All other subsystems should derive
//! their paths from this struct rather than calling into the [`directories`]
//! crate themselves.

use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::error::{Result, StorageError};

/// Collection of filesystem locations Orchid writes to.
///
/// On Windows the defaults resolve roughly to:
///
/// | field             | example location                        |
/// |-------------------|-----------------------------------------|
/// | `config_dir`      | `%AppData%\Orchid\Orchid\config`        |
/// | `data_dir`        | `%AppData%\Orchid\Orchid\data`          |
/// | `cache_dir`       | `%LocalAppData%\Orchid\Orchid\cache`    |
/// | `logs_dir`        | `%LocalAppData%\Orchid\Orchid\data\logs`|
///
/// (The exact values come from the [`directories`] crate and may vary by
/// OS version.) In tests use [`OrchidPaths::for_testing`] to pin all paths
/// under a temporary directory.
#[derive(Debug, Clone)]
pub struct OrchidPaths {
    /// Root directory for user-editable configuration.
    pub config_dir: PathBuf,
    /// Root directory for persistent application data (the redb database,
    /// chunk store, password vault).
    pub data_dir: PathBuf,
    /// Root directory for disposable caches.
    pub cache_dir: PathBuf,
    /// Directory for rotated log files.
    pub logs_dir: PathBuf,
    /// Directory containing user-installed theme files.
    pub themes_dir: PathBuf,
    /// Directory containing saved workspace definitions.
    pub workspaces_dir: PathBuf,
    /// Directory containing user-installed widget bundles.
    pub widgets_dir: PathBuf,
    /// Directory containing translation catalogues.
    pub locales_dir: PathBuf,
    /// Directory containing the content-addressed chunk store.
    pub chunks_dir: PathBuf,
    /// File path of the main redb state database.
    pub state_db_path: PathBuf,
    /// File path of the KDBX4 password database.
    pub passwords_db_path: PathBuf,
    /// File path of the main TOML configuration file.
    pub config_file: PathBuf,
}

impl OrchidPaths {
    /// Resolve the canonical Orchid paths for the current user and create
    /// every directory that does not yet exist.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::PathResolution`] if the OS cannot provide a
    /// project directory (e.g. in a sandbox with no home directory), and
    /// [`StorageError::Io`] if directory creation fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orchid_storage::OrchidPaths;
    ///
    /// let paths = OrchidPaths::resolve()?;
    /// assert!(paths.config_dir.exists());
    /// # Ok::<(), orchid_storage::StorageError>(())
    /// ```
    pub fn resolve() -> Result<Self> {
        let project = ProjectDirs::from("com", "Orchid", "Orchid").ok_or_else(|| {
            StorageError::PathResolution(
                "could not determine project directories (no home directory?)".into(),
            )
        })?;

        let config_dir = project.config_dir().to_path_buf();
        let data_dir = project.data_dir().to_path_buf();
        let cache_dir = project.cache_dir().to_path_buf();
        let logs_dir = data_dir.join("logs");

        let paths = Self::compose(config_dir, data_dir, cache_dir, logs_dir);
        paths.ensure_directories()?;
        Ok(paths)
    }

    /// Build a set of paths rooted under `root`, suitable for tests.
    ///
    /// No directories are created; call [`OrchidPaths::ensure_directories`]
    /// explicitly if needed.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_storage::OrchidPaths;
    /// use std::path::Path;
    ///
    /// let paths = OrchidPaths::for_testing(Path::new("/tmp/orchid-test"));
    /// assert!(paths.config_dir.starts_with("/tmp/orchid-test"));
    /// ```
    #[must_use]
    pub fn for_testing(root: &Path) -> Self {
        let config_dir = root.join("config");
        let data_dir = root.join("data");
        let cache_dir = root.join("cache");
        let logs_dir = root.join("logs");
        Self::compose(config_dir, data_dir, cache_dir, logs_dir)
    }

    fn compose(
        config_dir: PathBuf,
        data_dir: PathBuf,
        cache_dir: PathBuf,
        logs_dir: PathBuf,
    ) -> Self {
        let themes_dir = config_dir.join("themes");
        let workspaces_dir = config_dir.join("workspaces");
        let widgets_dir = config_dir.join("widgets");
        let locales_dir = config_dir.join("locales");
        let chunks_dir = data_dir.join("chunks");
        let state_db_path = data_dir.join("state.redb");
        let passwords_db_path = data_dir.join("passwords.kdbx");
        let config_file = config_dir.join("config.toml");

        Self {
            config_dir,
            data_dir,
            cache_dir,
            logs_dir,
            themes_dir,
            workspaces_dir,
            widgets_dir,
            locales_dir,
            chunks_dir,
            state_db_path,
            passwords_db_path,
            config_file,
        }
    }

    /// Create every directory this struct refers to if it does not already
    /// exist. Idempotent.
    ///
    /// # Errors
    ///
    /// Propagates [`StorageError::Io`] if any directory cannot be created.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_storage::OrchidPaths;
    /// let tmp = tempfile::tempdir().unwrap();
    /// let paths = OrchidPaths::for_testing(tmp.path());
    /// paths.ensure_directories().unwrap();
    /// assert!(paths.themes_dir.exists());
    /// ```
    pub fn ensure_directories(&self) -> Result<()> {
        for dir in [
            &self.config_dir,
            &self.data_dir,
            &self.cache_dir,
            &self.logs_dir,
            &self.themes_dir,
            &self.workspaces_dir,
            &self.widgets_dir,
            &self.locales_dir,
            &self.chunks_dir,
        ] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_testing_places_subdirs_under_root() {
        let root = Path::new("/tmp/orchid-test-root");
        let paths = OrchidPaths::for_testing(root);

        assert!(paths.config_dir.starts_with(root));
        assert!(paths.data_dir.starts_with(root));
        assert!(paths.cache_dir.starts_with(root));
        assert!(paths.logs_dir.starts_with(root));
        assert!(paths.themes_dir.starts_with(&paths.config_dir));
        assert!(paths.chunks_dir.starts_with(&paths.data_dir));
        assert_eq!(paths.state_db_path.file_name().unwrap(), "state.redb");
        assert_eq!(paths.passwords_db_path.file_name().unwrap(), "passwords.kdbx");
        assert_eq!(paths.config_file.file_name().unwrap(), "config.toml");
    }

    #[test]
    fn ensure_directories_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = OrchidPaths::for_testing(tmp.path());
        paths.ensure_directories().unwrap();
        paths.ensure_directories().unwrap();
        assert!(paths.themes_dir.exists());
        assert!(paths.chunks_dir.exists());
    }
}
