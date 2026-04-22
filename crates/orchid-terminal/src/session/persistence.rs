//! Conversions between our [`BackendKind`] and the storage-side enum.

use crate::backend::{BackendKind, SshTarget};
use crate::error::Result;

/// Project a [`BackendKind`] down to what the storage layer can persist.
///
/// The storage shape is a simpler enum that does not carry jump hosts / key
/// paths. Those are reconstructed from the user's config on restore.
#[must_use]
pub fn backend_kind_to_storage(k: &BackendKind) -> orchid_storage::TerminalBackend {
    match k {
        BackendKind::PowerShell => orchid_storage::TerminalBackend::PowerShell,
        BackendKind::Cmd => orchid_storage::TerminalBackend::Cmd,
        BackendKind::Wsl(d) => orchid_storage::TerminalBackend::Wsl(d.clone()),
        BackendKind::Ssh(target) => orchid_storage::TerminalBackend::Ssh(target.to_uri()),
        BackendKind::Custom { command, .. } => {
            // Map Custom to an SSH-shaped string with a fake scheme so it's
            // distinguishable on inspection; the real restore path would use
            // a Custom variant in storage which we'll add in v1.x.
            orchid_storage::TerminalBackend::Ssh(format!("custom://{command}"))
        }
    }
}

/// Inverse of [`backend_kind_to_storage`].
///
/// # Errors
///
/// Returns [`TerminalError::BackendUnavailable`] when a persisted Custom
/// entry cannot be parsed.
pub fn backend_kind_from_storage(
    k: &orchid_storage::TerminalBackend,
) -> Result<BackendKind> {
    Ok(match k {
        orchid_storage::TerminalBackend::PowerShell => BackendKind::PowerShell,
        orchid_storage::TerminalBackend::Cmd => BackendKind::Cmd,
        orchid_storage::TerminalBackend::Wsl(d) => BackendKind::Wsl(d.clone()),
        orchid_storage::TerminalBackend::Ssh(uri) => {
            if let Some(rest) = uri.strip_prefix("custom://") {
                BackendKind::Custom {
                    command: rest.to_string(),
                    args: Vec::new(),
                }
            } else {
                BackendKind::Ssh(SshTarget::from_uri(uri)?)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powershell_round_trip() {
        let original = BackendKind::PowerShell;
        let stored = backend_kind_to_storage(&original);
        let back = backend_kind_from_storage(&stored).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn wsl_round_trip() {
        let original = BackendKind::Wsl("Ubuntu-22.04".into());
        let stored = backend_kind_to_storage(&original);
        let back = backend_kind_from_storage(&stored).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn ssh_round_trip() {
        let target = SshTarget {
            host: "host".into(),
            user: Some("alice".into()),
            port: Some(22),
            jump_hosts: vec![],
            identity_file: None,
            extra_args: vec![],
        };
        let original = BackendKind::Ssh(target);
        let stored = backend_kind_to_storage(&original);
        let back = backend_kind_from_storage(&stored).unwrap();
        assert_eq!(back, original);
    }
}
