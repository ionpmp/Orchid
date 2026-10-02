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
        assert_eq!(locks.count(), 2);
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
