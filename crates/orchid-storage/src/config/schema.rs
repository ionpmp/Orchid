//! Typed representation of `config.toml`.
//!
//! The top-level type is [`OrchidConfig`]. Every field has a [`Default`]
//! implementation and uses `#[serde(default)]` so that partial TOML files
//! round-trip cleanly and new keys can be added without breaking older
//! configs.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{Result, StorageError};

/// Root of the TOML configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
pub struct OrchidConfig {
    /// Miscellaneous, application-wide settings.
    pub general: GeneralConfig,
    /// Theme, density, and typography settings.
    pub appearance: AppearanceConfig,
    /// Touch, pen, mouse, and keyboard preferences.
    pub input: InputConfig,
    /// User-defined keyboard shortcut overrides.
    pub shortcuts: ShortcutsConfig,
    /// Locale, date / time formatting, calendar preferences.
    pub locale: LocaleConfig,
    /// History retention and clipboard privacy controls.
    pub privacy: PrivacyConfig,
    /// File-manager global settings (network mounts, etc.).
    pub file_manager: FileManagerSectionConfig,
}

/// Application-wide toggles that don't fit any other section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct GeneralConfig {
    /// Whether Orchid automatically checks for and downloads updates.
    pub auto_update: bool,
    /// Whether anonymous telemetry is enabled. Off by default (opt-in only).
    pub telemetry: bool,
    /// Whether Orchid should start automatically on user login.
    pub open_on_startup: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            auto_update: true,
            telemetry: false,
            open_on_startup: false,
        }
    }
}

/// Theme, density, and typography settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct AppearanceConfig {
    /// Identifier of the active theme (filename stem in `themes/`).
    pub theme: String,
    /// Layout density.
    pub density: Density,
    /// Preferred UI font family. `None` means use the system default.
    pub font_family: Option<String>,
    /// UI font scale factor. Valid range is `0.75..=2.0`.
    pub font_scale: f32,
    /// If `true`, disables large motion animations.
    pub reduce_motion: bool,
    /// If `true`, the `dark_theme` / `light_theme` pair follows the OS setting.
    pub follow_system_theme: bool,
    /// Theme to use when the system reports a dark appearance.
    pub dark_theme: String,
    /// Theme to use when the system reports a light appearance.
    pub light_theme: String,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: "orchid-dark".to_string(),
            density: Density::Hybrid,
            font_family: None,
            font_scale: 1.0,
            reduce_motion: false,
            follow_system_theme: true,
            dark_theme: "orchid-dark".to_string(),
            light_theme: "orchid-light".to_string(),
        }
    }
}

/// Layout density.
#[allow(missing_docs)] // variants are self-describing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Density {
    Touch,
    Mouse,
    Hybrid,
}

/// Input device preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct InputConfig {
    /// Which hand the user primarily uses. Affects drawer placement and
    /// gesture mirroring.
    pub primary_hand: Hand,
    /// If `true`, edge swipes originate from the opposite edge based on
    /// `primary_hand`.
    pub mirror_edge_swipes: bool,
    /// If `true`, the touch subsystem provides haptic feedback where
    /// supported.
    pub haptic_feedback: bool,
    /// If `true`, palm touches are rejected while a pen is in proximity.
    pub palm_rejection: bool,
    /// Action triggered by a pen double-tap.
    pub pen_double_tap_action: PenDoubleTapAction,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            primary_hand: Hand::Right,
            mirror_edge_swipes: false,
            haptic_feedback: true,
            palm_rejection: true,
            pen_double_tap_action: PenDoubleTapAction::SwitchTool,
        }
    }
}

/// Primary hand for input mirroring.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Hand {
    Left,
    Right,
}

/// Action bound to a pen double-tap.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PenDoubleTapAction {
    None,
    SwitchTool,
    Erase,
}

/// User-configured keyboard shortcut overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ShortcutsConfig {
    /// Map of command identifier (e.g. `"command-palette"`) to a shortcut
    /// string like `"Ctrl+Shift+P"`.
    pub overrides: HashMap<String, String>,
}

/// Locale, formatting, and calendar preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LocaleConfig {
    /// BCP 47 language tag (e.g. `"en-US"`, `"de-DE"`, `"zh-Hans"`).
    pub language: String,
    /// Custom date format string; `None` derives it from `language`.
    pub date_format: Option<String>,
    /// Custom time format string; `None` derives it from `language`.
    pub time_format: Option<String>,
    /// First day of the week: `0` = Sunday, `1` = Monday.
    pub first_day_of_week: u8,
}

impl Default for LocaleConfig {
    fn default() -> Self {
        Self {
            language: "en-US".to_string(),
            date_format: None,
            time_format: None,
            first_day_of_week: 1,
        }
    }
}

/// Retention and clipboard privacy controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PrivacyConfig {
    /// If `true`, user actions are appended to the action history.
    pub record_action_history: bool,
    /// Number of days to retain history entries before automatic pruning.
    pub history_retention_days: u32,
    /// Automatically clear the clipboard this many seconds after a sensitive
    /// value (password, token) is copied. `0` disables the feature.
    pub clear_clipboard_seconds: u32,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            record_action_history: true,
            history_retention_days: 90,
            clear_clipboard_seconds: 30,
        }
    }
}

/// Global file-manager settings stored in `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
pub struct FileManagerSectionConfig {
    /// Remote folder mounts listed under the Network sidebar (rclone pending).
    pub network_mounts: Vec<NetworkMountConfig>,
}

/// One configured remote mount (SFTP, SMB, WebDAV, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct NetworkMountConfig {
    /// Display name in the sidebar and virtual network folder.
    pub name: String,
    /// Mount URI (`sftp:host/path` or `sftp://host/path`).
    pub uri: String,
    /// Optional username for on-the-fly rclone connection strings.
    pub user: Option<String>,
    /// Optional password for on-the-fly rclone connection strings.
    pub password: Option<String>,
    /// When set, use this rclone.conf remote name instead of parsing `uri`.
    pub rclone_remote: Option<String>,
    /// When false, the mount is hidden from the UI.
    pub enabled: bool,
}

impl Default for NetworkMountConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            uri: String::new(),
            user: None,
            password: None,
            rclone_remote: None,
            enabled: true,
        }
    }
}

impl OrchidConfig {
    /// Validate semantic invariants that cannot be enforced purely by types.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::ConfigValidation`] with a human-readable
    /// message if any rule is violated:
    ///
    /// * `appearance.font_scale` must fall within `[0.75, 2.0]`.
    /// * `locale.first_day_of_week` must be `0` (Sunday) or `1` (Monday).
    /// * `locale.language` must look like a BCP 47 tag.
    /// * `privacy.history_retention_days` must be `<= 3650` (10 years).
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_storage::OrchidConfig;
    ///
    /// let mut cfg = OrchidConfig::default();
    /// assert!(cfg.validate().is_ok());
    ///
    /// cfg.appearance.font_scale = 5.0;
    /// assert!(cfg.validate().is_err());
    /// ```
    pub fn validate(&self) -> Result<()> {
        if !(0.75..=2.0).contains(&self.appearance.font_scale) {
            return Err(StorageError::ConfigValidation(format!(
                "appearance.font-scale must be within [0.75, 2.0], got {}",
                self.appearance.font_scale
            )));
        }

        if self.locale.first_day_of_week > 1 {
            return Err(StorageError::ConfigValidation(format!(
                "locale.first-day-of-week must be 0 (Sunday) or 1 (Monday), got {}",
                self.locale.first_day_of_week
            )));
        }

        if !is_valid_bcp47(&self.locale.language) {
            return Err(StorageError::ConfigValidation(format!(
                "locale.language `{}` does not look like a valid BCP 47 tag",
                self.locale.language
            )));
        }

        if self.privacy.history_retention_days > 3650 {
            return Err(StorageError::ConfigValidation(format!(
                "privacy.history-retention-days must be <= 3650 (10 years), got {}",
                self.privacy.history_retention_days
            )));
        }

        Ok(())
    }
}

/// Minimal BCP 47 sanity check.
///
/// The full grammar is intentionally not reimplemented — we just make sure the
/// tag is non-empty, consists of `-`-separated subtags made of ASCII letters
/// and digits, and starts with a letter subtag of 2-3 characters (the primary
/// language).
fn is_valid_bcp47(tag: &str) -> bool {
    if tag.is_empty() || tag.len() > 35 {
        return false;
    }
    let mut subtags = tag.split('-');
    let Some(primary) = subtags.next() else {
        return false;
    };
    if !(2..=3).contains(&primary.len()) || !primary.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    subtags.all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_roundtrips_through_toml() {
        let cfg = OrchidConfig::default();
        let s = toml::to_string_pretty(&cfg).unwrap();
        let decoded: OrchidConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg.appearance.theme, decoded.appearance.theme);
        assert_eq!(cfg.locale.language, decoded.locale.language);
        assert_eq!(cfg.input.primary_hand, decoded.input.primary_hand);
    }

    #[test]
    fn missing_sections_fall_back_to_defaults() {
        let cfg: OrchidConfig = toml::from_str("").unwrap();
        assert!(cfg.general.auto_update);
        assert!((cfg.appearance.font_scale - 1.0).abs() < f32::EPSILON);
        assert_eq!(cfg.locale.language, "en-US");
    }

    #[test]
    fn validate_rejects_out_of_range_font_scale() {
        let mut cfg = OrchidConfig::default();
        cfg.appearance.font_scale = 0.5;
        assert!(cfg.validate().is_err());
        cfg.appearance.font_scale = 2.5;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_bad_first_day_of_week() {
        let mut cfg = OrchidConfig::default();
        cfg.locale.first_day_of_week = 7;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_malformed_language_tag() {
        let mut cfg = OrchidConfig::default();
        cfg.locale.language = "!!".to_string();
        assert!(cfg.validate().is_err());
        cfg.locale.language = String::new();
        assert!(cfg.validate().is_err());
        cfg.locale.language = "x".to_string();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_absurd_retention() {
        let mut cfg = OrchidConfig::default();
        cfg.privacy.history_retention_days = 100_000;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn valid_bcp47_accepts_common_tags() {
        assert!(is_valid_bcp47("en"));
        assert!(is_valid_bcp47("en-US"));
        assert!(is_valid_bcp47("zh-Hans"));
        assert!(is_valid_bcp47("de-DE"));
        assert!(!is_valid_bcp47(""));
        assert!(!is_valid_bcp47("english"));
        assert!(!is_valid_bcp47("en_US"));
    }
}
