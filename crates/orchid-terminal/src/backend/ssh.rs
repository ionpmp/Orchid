//! SSH backend target and URI helpers.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Result, TerminalError};

/// Everything needed to spawn an `ssh` subprocess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshTarget {
    /// Hostname or IP.
    pub host: String,
    /// Optional username.
    pub user: Option<String>,
    /// Optional non-standard port.
    pub port: Option<u16>,
    /// Jump hosts (`-J a,b,c`).
    pub jump_hosts: Vec<String>,
    /// Identity file path (`-i`).
    pub identity_file: Option<PathBuf>,
    /// Extra raw arguments appended verbatim.
    pub extra_args: Vec<String>,
}

impl SshTarget {
    /// Parse an `ssh://[user@]host[:port]` URI.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::BackendUnavailable`] for malformed input.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_terminal::SshTarget;
    /// let t = SshTarget::from_uri("ssh://alice@host.example:2222").unwrap();
    /// assert_eq!(t.user.as_deref(), Some("alice"));
    /// assert_eq!(t.host, "host.example");
    /// assert_eq!(t.port, Some(2222));
    /// ```
    pub fn from_uri(uri: &str) -> Result<Self> {
        let rest = uri.strip_prefix("ssh://").ok_or_else(|| {
            TerminalError::BackendUnavailable(format!("missing ssh:// scheme in {uri}"))
        })?;
        if rest.is_empty() {
            return Err(TerminalError::BackendUnavailable("empty ssh URI".into()));
        }
        let (user, host_port) = match rest.split_once('@') {
            Some((u, rest)) => (Some(u.to_string()), rest),
            None => (None, rest),
        };
        let (host, port) = match host_port.rsplit_once(':') {
            Some((h, p)) => {
                let port: u16 = p.parse().map_err(|e: std::num::ParseIntError| {
                    TerminalError::BackendUnavailable(format!("invalid port in {uri}: {e}"))
                })?;
                (h.to_string(), Some(port))
            }
            None => (host_port.to_string(), None),
        };
        if host.is_empty() {
            return Err(TerminalError::BackendUnavailable(format!(
                "empty host in {uri}"
            )));
        }
        Ok(Self {
            host,
            user,
            port,
            jump_hosts: Vec::new(),
            identity_file: None,
            extra_args: Vec::new(),
        })
    }

    /// Render this target back into an `ssh://` URI.
    #[must_use]
    pub fn to_uri(&self) -> String {
        let mut s = String::from("ssh://");
        if let Some(u) = &self.user {
            s.push_str(u);
            s.push('@');
        }
        s.push_str(&self.host);
        if let Some(p) = self.port {
            s.push(':');
            s.push_str(&p.to_string());
        }
        s
    }

    /// Compose an argv for the system `ssh` binary.
    #[must_use]
    pub fn to_args(&self) -> Vec<String> {
        let mut args: Vec<String> = Vec::new();
        if let Some(p) = self.port {
            args.push("-p".into());
            args.push(p.to_string());
        }
        if let Some(id) = &self.identity_file {
            args.push("-i".into());
            args.push(id.display().to_string());
        }
        if !self.jump_hosts.is_empty() {
            args.push("-J".into());
            args.push(self.jump_hosts.join(","));
        }
        let host_with_user = match &self.user {
            Some(u) => format!("{u}@{}", self.host),
            None => self.host.clone(),
        };
        args.push(host_with_user);
        args.extend(self.extra_args.iter().cloned());
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_uri_basic() {
        let t = SshTarget::from_uri("ssh://alice@host:2222").unwrap();
        assert_eq!(t.host, "host");
        assert_eq!(t.user.as_deref(), Some("alice"));
        assert_eq!(t.port, Some(2222));
    }

    #[test]
    fn from_uri_host_only() {
        let t = SshTarget::from_uri("ssh://host").unwrap();
        assert!(t.user.is_none());
        assert!(t.port.is_none());
    }

    #[test]
    fn to_uri_round_trip() {
        let t = SshTarget {
            host: "host".into(),
            user: Some("bob".into()),
            port: Some(22),
            jump_hosts: vec![],
            identity_file: None,
            extra_args: vec![],
        };
        let back = SshTarget::from_uri(&t.to_uri()).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn to_args_with_jumps_and_identity() {
        let t = SshTarget {
            host: "target".into(),
            user: Some("alice".into()),
            port: Some(2200),
            jump_hosts: vec!["bastion1".into(), "bastion2".into()],
            identity_file: Some(PathBuf::from("C:/keys/id_ed25519")),
            extra_args: vec!["-v".into()],
        };
        let args = t.to_args();
        assert!(args.contains(&"-p".into()));
        assert!(args.contains(&"2200".into()));
        assert!(args.contains(&"-i".into()));
        assert!(args.contains(&"-J".into()));
        assert!(args.contains(&"bastion1,bastion2".into()));
        assert!(args.contains(&"alice@target".into()));
        assert!(args.contains(&"-v".into()));
    }

    #[test]
    fn from_uri_rejects_garbage() {
        assert!(SshTarget::from_uri("not-a-uri").is_err());
        assert!(SshTarget::from_uri("ssh://").is_err());
        assert!(SshTarget::from_uri("ssh://host:abc").is_err());
    }
}
