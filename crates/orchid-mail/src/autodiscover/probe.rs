//! Guess common hostnames when ISPDB / SRV fail.

use crate::account::{AuthKind, ServerEndpoint, ServerSuggestion, TlsMode};

/// Build a best-effort suggestion from conventional hostnames.
#[must_use]
pub fn guess_hosts(domain: &str) -> Option<ServerSuggestion> {
    if domain.is_empty() {
        return None;
    }
    Some(ServerSuggestion {
        imap: ServerEndpoint {
            host: format!("imap.{domain}"),
            port: 993,
            tls: TlsMode::Implicit,
            username: String::new(),
        },
        smtp: ServerEndpoint {
            host: format!("smtp.{domain}"),
            port: 587,
            tls: TlsMode::StartTls,
            username: String::new(),
        },
        auth: AuthKind::Password,
        oauth_provider: None,
        source: String::new(),
    })
}
