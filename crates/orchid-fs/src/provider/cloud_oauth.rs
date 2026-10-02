//! Browser sign-in for Google Drive, personal OneDrive, and Dropbox.
//!
//! rclone stores the OAuth token in its own config. Orchid only keeps the
//! remote name. The wizard passes `config_is_local true` so rclone opens a
//! browser even when it is spawned without a terminal.

use tokio::process::Command;

use crate::error::{FsError, Result};

/// A cloud remote the user asked to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudConnect {
    /// Sidebar label.
    pub display_name: String,
    /// rclone backend: `drive`, `onedrive`, or `dropbox`.
    pub backend: String,
    /// rclone.conf remote id.
    pub remote: String,
}

/// Parse `Name | drive|onedrive|dropbox` with an optional `| remote-id`.
///
/// `google` and `gdrive` mean Drive. OneDrive here is the personal account
/// (`drive_type personal`); business tenants stay a manual `rclone config`.
#[must_use]
pub fn parse_cloud_connect(input: &str) -> Option<CloudConnect> {
    let parts: Vec<&str> = input
        .split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let (name, backend_raw, remote) = match parts.as_slice() {
        [name, backend] => (*name, *backend, None),
        [name, backend, remote] => (*name, *backend, Some(*remote)),
        _ => return None,
    };
    let backend = match backend_raw.to_ascii_lowercase().as_str() {
        "drive" | "google" | "gdrive" => "drive",
        "onedrive" | "one-drive" => "onedrive",
        "dropbox" => "dropbox",
        _ => return None,
    };
    if name.is_empty() || name.chars().count() > 80 {
        return None;
    }
    let remote = match remote {
        Some(id) if is_remote_id(id) => id.to_string(),
        Some(_) => return None,
        None => slug_remote(name, backend)?,
    };
    Some(CloudConnect {
        display_name: name.to_string(),
        backend: backend.to_string(),
        remote,
    })
}

/// Arguments for `rclone config create`, without the binary name.
#[must_use]
pub fn cloud_config_create_args(spec: &CloudConnect) -> Vec<String> {
    let mut args = vec![
        "config".to_string(),
        "create".to_string(),
        spec.remote.clone(),
        spec.backend.clone(),
        "config_is_local".to_string(),
        "true".to_string(),
    ];
    match spec.backend.as_str() {
        "drive" => {
            args.push("scope".to_string());
            args.push("drive".to_string());
        }
        "onedrive" => {
            args.push("drive_type".to_string());
            args.push("personal".to_string());
        }
        _ => {}
    }
    args
}

/// Run rclone's browser OAuth and write the remote into rclone.conf.
///
/// # Errors
///
/// rclone is missing, or the sign-in did not finish. The error text does not
/// include the token.
pub async fn create_cloud_remote(spec: &CloudConnect) -> Result<()> {
    let bin = std::env::var("RCLONE_BIN").unwrap_or_else(|_| "rclone".to_string());
    let args = cloud_config_create_args(spec);
    let output = Command::new(&bin)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FsError::InvalidPath {
                    reason: format!(
                        "`{bin}` not found; install rclone and ensure it is on PATH (or set RCLONE_BIN)"
                    ),
                }
            } else {
                FsError::Io(e)
            }
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    tracing::warn!(
        remote = %spec.remote,
        backend = %spec.backend,
        detail = %redact_oauth_log(stderr.trim()),
        "cloud sign-in did not finish"
    );
    Err(FsError::InvalidPath {
        reason: "fm-cloud-oauth-failed".into(),
    })
}

fn is_remote_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let mut len = 1usize;
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return false;
        }
        len += 1;
        if len > 32 {
            return false;
        }
    }
    true
}

fn slug_remote(name: &str, backend: &str) -> Option<String> {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        }
    }
    if out.is_empty() || !out.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        out = format!("{backend}{out}");
    }
    if out.len() > 32 {
        out.truncate(32);
    }
    is_remote_id(&out).then_some(out)
}

fn redact_oauth_log(text: &str) -> String {
    let flat = text.replace(['\n', '\r'], " ");
    let lower = flat.to_ascii_lowercase();
    let mut s = if let Some(i) = lower.find("token") {
        let mut cut = flat[..i].to_string();
        cut.push_str("token=***");
        cut
    } else {
        flat
    };
    if s.len() > 240 {
        s.truncate(240);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_drive_and_slugs_the_name() {
        let spec = parse_cloud_connect("My Drive | gdrive").expect("parse");
        assert_eq!(spec.display_name, "My Drive");
        assert_eq!(spec.backend, "drive");
        assert_eq!(spec.remote, "MyDrive");
        let args = cloud_config_create_args(&spec);
        assert_eq!(
            args,
            vec![
                "config",
                "create",
                "MyDrive",
                "drive",
                "config_is_local",
                "true",
                "scope",
                "drive",
            ]
        );
    }

    #[test]
    fn onedrive_uses_personal_drive_and_explicit_id() {
        let spec = parse_cloud_connect("Home | one-drive | homebox").expect("parse");
        assert_eq!(spec.backend, "onedrive");
        assert_eq!(spec.remote, "homebox");
        let args = cloud_config_create_args(&spec);
        assert!(args.windows(2).any(|w| w == ["drive_type", "personal"]));
        assert!(!args.iter().any(|a| a == "scope"));
    }

    #[test]
    fn rejects_other_backends_and_bad_ids() {
        assert!(parse_cloud_connect("Host | sftp").is_none());
        assert!(parse_cloud_connect("Only a name").is_none());
        assert!(parse_cloud_connect("X | dropbox | has space").is_none());
        assert!(parse_cloud_connect("X | dropbox | -bad").is_none());
    }

    #[test]
    fn non_ascii_name_falls_back_to_the_backend() {
        let spec = parse_cloud_connect("Облако | dropbox").expect("parse");
        assert_eq!(spec.remote, "dropbox");
    }

    #[test]
    fn log_redaction_drops_token_text() {
        let s = redact_oauth_log("failed token=secret-value more");
        assert!(s.contains("token=***"));
        assert!(!s.contains("secret-value"));
    }
}
