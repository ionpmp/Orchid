//! Local policy file and the on-disk audit log.
//!
//! [`policy_path`] is `policy.toml` beside `config.toml`. It marks a fixed set
//! of settings read-only in the Settings panel. It does not rewrite those
//! values in `config.toml`. An optional HTTPS address in `[policy]` is fetched
//! at startup into this same file. A failed fetch leaves the file as it was.
//! [`audit_path`] is `audit.log` in that directory. Lines stay on this computer.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File name of the policy document, beside `config.toml`.
pub const POLICY_FILE_NAME: &str = "policy.toml";

/// File name of the local audit log, beside `config.toml`.
pub const AUDIT_FILE_NAME: &str = "audit.log";

/// Largest policy document that replaces the file on disk.
pub const POLICY_MAX_BYTES: usize = 64 * 1024;

/// Settings the policy file can mark read-only.
///
/// A missing key is editable. Unknown keys in the file are ignored.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default, rename_all = "kebab-case")]
pub struct PolicyLocks {
    /// Settings → General, automatic update check.
    pub auto_update: bool,
    /// Settings → General, telemetry switch.
    pub telemetry: bool,
    /// Settings → General, telemetry address.
    pub telemetry_endpoint: bool,
    /// Settings → General, open on startup.
    pub open_on_startup: bool,
    /// Settings → General, Windows notifications.
    pub os_notifications: bool,
    /// Settings → Appearance, theme.
    pub theme: bool,
    /// Settings → Locale, language.
    pub language: bool,
    /// Settings → Shell, replace Explorer at sign-in.
    pub shell_replace: bool,
    /// Settings → Policy, the HTTPS address itself.
    pub policy_url: bool,
    /// Settings → Search, index roots.
    pub search_roots: bool,
    /// Settings → Search, exclusion patterns.
    pub search_excludes: bool,
    /// Settings → Search, content size limit.
    pub search_max_mib: bool,
    /// Settings → Search, plain-text extraction.
    pub search_extract_text: bool,
    /// Settings → Search, PDF and office extraction.
    pub search_extract_pdf: bool,
    /// Settings → Search, replacement ONNX path.
    pub search_model: bool,
    /// Settings → Appearance, density.
    pub density: bool,
    /// Settings → Appearance, font family.
    pub font_family: bool,
    /// Settings → Appearance, font scale.
    pub font_scale: bool,
    /// Settings → Appearance, reduce motion.
    pub reduce_motion: bool,
    /// Settings → Appearance, follow the system theme.
    pub follow_system_theme: bool,
    /// Settings → Appearance, dark theme.
    pub dark_theme: bool,
    /// Settings → Appearance, light theme.
    pub light_theme: bool,
    /// Settings → Locale, date format.
    pub date_format: bool,
    /// Settings → Locale, time format.
    pub time_format: bool,
    /// Settings → Locale, first day of the week.
    pub first_day_of_week: bool,
    /// Settings → Privacy, record action history.
    pub record_action_history: bool,
    /// Settings → Privacy, history retention.
    pub history_retention_days: bool,
    /// Settings → Privacy, clipboard clear delay.
    pub clear_clipboard_seconds: bool,
    /// Settings → Privacy, vault auto-lock.
    pub vault_auto_lock_seconds: bool,
    /// Settings → Terminal, grid.
    pub terminal_grid: bool,
    /// Settings → Input, primary hand.
    pub primary_hand: bool,
    /// Settings → Input, mirror edge swipes.
    pub mirror_edge_swipes: bool,
    /// Settings → Input, haptic feedback.
    pub haptic_feedback: bool,
    /// Settings → Input, palm rejection.
    pub palm_rejection: bool,
    /// Settings → Input, pen double-tap.
    pub pen_double_tap: bool,
    /// Settings → Photos, auto-tag from folder names.
    pub photos_auto_tag: bool,
    /// Settings → Photos, face detection.
    pub photos_detect_faces: bool,
    /// Settings → Agent, enable the chat.
    pub agent_enabled: bool,
    /// Settings → Agent, backend.
    pub agent_backend: bool,
    /// Settings → Agent, endpoint.
    pub agent_endpoint: bool,
    /// Settings → Agent, model name.
    pub agent_model: bool,
    /// Settings → Agent, API key and the clear button.
    pub agent_key: bool,
}

impl PolicyLocks {
    /// Whether the Settings field `section` / `key` is read-only.
    #[must_use]
    pub fn is_locked(&self, section: &str, key: &str) -> bool {
        match (section, key) {
            ("general", "auto-update") => self.auto_update,
            ("general", "telemetry") => self.telemetry,
            ("general", "telemetry-endpoint") => self.telemetry_endpoint,
            ("general", "open-on-startup") => self.open_on_startup,
            ("general", "os-notifications") => self.os_notifications,
            ("appearance", "theme") => self.theme,
            ("locale", "language") => self.language,
            ("shell", "replace") => self.shell_replace,
            ("policy", "url") => self.policy_url,
            ("search", "included-roots") => self.search_roots,
            ("search", "excluded-patterns") => self.search_excludes,
            ("search", "max-file-size-mib") => self.search_max_mib,
            ("search", "extract-text") => self.search_extract_text,
            ("search", "extract-pdf") => self.search_extract_pdf,
            ("search", "sentence-model") => self.search_model,
            ("appearance", "density") => self.density,
            ("appearance", "font-family") => self.font_family,
            ("appearance", "font-scale") => self.font_scale,
            ("appearance", "reduce-motion") => self.reduce_motion,
            ("appearance", "follow-system-theme") => self.follow_system_theme,
            ("appearance", "dark-theme") => self.dark_theme,
            ("appearance", "light-theme") => self.light_theme,
            ("locale", "date-format") => self.date_format,
            ("locale", "time-format") => self.time_format,
            ("locale", "first-day-of-week") => self.first_day_of_week,
            ("privacy", "record-action-history") => self.record_action_history,
            ("privacy", "history-retention-days") => self.history_retention_days,
            ("privacy", "clear-clipboard-seconds") => self.clear_clipboard_seconds,
            ("privacy", "vault-auto-lock-seconds") => self.vault_auto_lock_seconds,
            ("terminal", "terminal-grid") => self.terminal_grid,
            ("input", "primary-hand") => self.primary_hand,
            ("input", "mirror-edge-swipes") => self.mirror_edge_swipes,
            ("input", "haptic-feedback") => self.haptic_feedback,
            ("input", "palm-rejection") => self.palm_rejection,
            ("input", "pen-double-tap-action") => self.pen_double_tap,
            ("photos", "auto-tag") => self.photos_auto_tag,
            ("photos", "detect-faces") => self.photos_detect_faces,
            ("agent", "enabled") => self.agent_enabled,
            ("agent", "backend") => self.agent_backend,
            ("agent", "endpoint") => self.agent_endpoint,
            ("agent", "model") => self.agent_model,
            ("agent", "api-key") | ("agent", "clear-key") => self.agent_key,
            _ => false,
        }
    }

    /// How many of the known settings are locked.
    #[must_use]
    pub fn count(&self) -> u32 {
        let flags = [
            self.auto_update,
            self.telemetry,
            self.telemetry_endpoint,
            self.open_on_startup,
            self.os_notifications,
            self.theme,
            self.language,
            self.shell_replace,
            self.policy_url,
            self.search_roots,
            self.search_excludes,
            self.search_max_mib,
            self.search_extract_text,
            self.search_extract_pdf,
            self.search_model,
            self.density,
            self.font_family,
            self.font_scale,
            self.reduce_motion,
            self.follow_system_theme,
            self.dark_theme,
            self.light_theme,
            self.date_format,
            self.time_format,
            self.first_day_of_week,
            self.record_action_history,
            self.history_retention_days,
            self.clear_clipboard_seconds,
            self.vault_auto_lock_seconds,
            self.terminal_grid,
            self.primary_hand,
            self.mirror_edge_swipes,
            self.haptic_feedback,
            self.palm_rejection,
            self.pen_double_tap,
            self.photos_auto_tag,
            self.photos_detect_faces,
            self.agent_enabled,
            self.agent_backend,
            self.agent_endpoint,
            self.agent_model,
            self.agent_key,
        ];
        flags.iter().filter(|flag| **flag).count() as u32
    }
}

/// Parsed `policy.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default, rename_all = "kebab-case")]
pub struct PolicyDocument {
    /// Read-only switches.
    pub lock: PolicyLocks,
}

/// What [`load_policy`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyLoad {
    /// The file is not there.
    Absent,
    /// A document parsed.
    Document(PolicyDocument),
    /// The file exists but is not a policy document.
    Invalid,
}

/// Result of trying to replace `policy.toml` with a fetched body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallPolicy {
    /// The body parsed and replaced the file.
    Saved {
        /// Locked settings in the new document.
        locks: u32,
    },
    /// The body was rejected. The previous file is unchanged.
    Rejected,
}

/// `policy.toml` next to `config_file`.
#[must_use]
pub fn policy_path(config_file: &Path) -> PathBuf {
    sibling(config_file, POLICY_FILE_NAME)
}

/// `audit.log` next to `config_file`.
#[must_use]
pub fn audit_path(config_file: &Path) -> PathBuf {
    sibling(config_file, AUDIT_FILE_NAME)
}

/// Locks from the policy file. A missing or invalid file locks nothing.
#[must_use]
pub fn locks_from_file(path: &Path) -> PolicyLocks {
    match load_policy(path) {
        PolicyLoad::Document(doc) => doc.lock,
        PolicyLoad::Absent | PolicyLoad::Invalid => PolicyLocks::default(),
    }
}

/// Read `policy.toml`.
#[must_use]
pub fn load_policy(path: &Path) -> PolicyLoad {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return PolicyLoad::Absent,
        Err(_) => return PolicyLoad::Invalid,
    };
    match toml::from_str::<PolicyDocument>(&text) {
        Ok(doc) => PolicyLoad::Document(doc),
        Err(_) => PolicyLoad::Invalid,
    }
}

/// Replace `path` when `body` is a policy document within [`POLICY_MAX_BYTES`].
///
/// # Errors
///
/// Returns an error when the directory or the file cannot be written. A body
/// that does not parse is [`InstallPolicy::Rejected`] and does not touch `path`.
pub fn install_policy_body(path: &Path, body: &str) -> io::Result<InstallPolicy> {
    if body.len() > POLICY_MAX_BYTES {
        return Ok(InstallPolicy::Rejected);
    }
    let doc: PolicyDocument = match toml::from_str(body) {
        Ok(doc) => doc,
        Err(_) => return Ok(InstallPolicy::Rejected),
    };
    atomic_write(path, body.as_bytes())?;
    Ok(InstallPolicy::Saved {
        locks: doc.lock.count(),
    })
}

/// True when `url` is empty or an `https` address without embedded credentials.
#[must_use]
pub fn policy_url_allowed(raw: &str) -> bool {
    let url = raw.trim();
    if url.is_empty() {
        return true;
    }
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    if rest.is_empty() || rest.contains(' ') || rest.contains('@') {
        return false;
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    !host.is_empty() && !host.starts_with('.') && !host.starts_with(':')
}

/// Append one audit line. Control characters in `event` and `detail` become spaces.
///
/// # Errors
///
/// Returns an error when the directory or the file cannot be written.
pub fn append_audit(path: &Path, event: &str, detail: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let stamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let line = format!("{} {} {}\n", stamp, sanitize(event), sanitize(detail));
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(line.as_bytes())?;
    Ok(())
}

fn sibling(config_file: &Path, name: &str) -> PathBuf {
    match config_file.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
        _ => PathBuf::from(name),
    }
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .take(180)
        .collect()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let tmp_path = match path.extension() {
        Some(ext) => {
            let mut os = ext.to_os_string();
            os.push(".tmp");
            path.with_extension(os)
        }
        None => path.with_extension("tmp"),
    };
    fs::write(&tmp_path, bytes)?;
    match fs::rename(&tmp_path, path) {
        Ok(()) => Ok(()),
        Err(_) => {
            let _ = fs::remove_file(path);
            fs::rename(&tmp_path, path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_follow_the_known_settings() {
        let mut locks = PolicyLocks::default();
        assert!(!locks.is_locked("general", "telemetry"));
        assert!(!locks.is_locked("general", "font-scale"));
        locks.telemetry = true;
        locks.shell_replace = true;
        assert!(locks.is_locked("general", "telemetry"));
        assert!(locks.is_locked("shell", "replace"));
        assert!(!locks.is_locked("shell", "previous"));
        assert!(!locks.is_locked("search", "included-roots"));
        locks.search_roots = true;
        assert!(locks.is_locked("search", "included-roots"));
        assert!(!locks.is_locked("appearance", "font-scale"));
        assert!(!locks.is_locked("agent", "clear-key"));
        locks.font_scale = true;
        locks.agent_key = true;
        assert!(locks.is_locked("appearance", "font-scale"));
        assert!(locks.is_locked("agent", "api-key"));
        assert!(locks.is_locked("agent", "clear-key"));
        assert_eq!(locks.count(), 5);
    }

    #[test]
    fn missing_file_locks_nothing_and_invalid_body_keeps_the_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("policy.toml");
        assert_eq!(load_policy(&path), PolicyLoad::Absent);
        assert_eq!(locks_from_file(&path).count(), 0);

        let body = "[lock]\ntelemetry = true\nshell-replace = true\n";
        assert_eq!(
            install_policy_body(&path, body).unwrap(),
            InstallPolicy::Saved { locks: 2 }
        );
        let PolicyLoad::Document(doc) = load_policy(&path) else {
            panic!("parsed");
        };
        assert!(doc.lock.telemetry);
        assert!(doc.lock.shell_replace);

        assert_eq!(
            install_policy_body(&path, "lock = [").unwrap(),
            InstallPolicy::Rejected
        );
        let kept = fs::read_to_string(&path).unwrap();
        assert!(kept.contains("telemetry = true"));
        assert_eq!(
            install_policy_body(&path, &"x".repeat(POLICY_MAX_BYTES + 1)).unwrap(),
            InstallPolicy::Rejected
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), kept);
    }

    #[test]
    fn audit_lines_stay_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.log");
        append_audit(&path, "policy-apply", "locks=2").unwrap();
        append_audit(&path, "update-check", "failed").unwrap();
        append_audit(&path, "shell-replace", "on\nhttps://secret.example").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("policy-apply locks=2"));
        assert!(lines[1].contains("update-check failed"));
        assert!(lines[2].contains("shell-replace on https://secret.example"));
    }

    #[test]
    fn policy_address_must_be_https_without_credentials() {
        assert!(policy_url_allowed(""));
        assert!(policy_url_allowed(
            "  https://example.com/orchid/policy.toml  "
        ));
        assert!(!policy_url_allowed("http://example.com/policy.toml"));
        assert!(!policy_url_allowed(
            "https://user:secret@example.com/policy.toml"
        ));
        assert!(!policy_url_allowed("not a url"));
    }
}
