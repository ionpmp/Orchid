//! Server discovery for IMAP / SMTP.

pub mod dns;
pub mod ispdb;
pub mod probe;
pub mod profiles;

use crate::account::{AuthKind, ServerEndpoint, ServerSuggestion, TlsMode};
use crate::error::Result;

pub use dns::lookup_srv;
pub use ispdb::fetch_ispdb;
pub use profiles::builtin_profile;

/// Run the full autodiscover cascade for `email`.
pub async fn discover(email: &str, http: &reqwest::Client) -> Result<ServerSuggestion> {
    let email = email.trim();
    let domain = domain_of(email)
        .ok_or_else(|| crate::error::MailError::Invalid("email address needs a domain".into()))?;

    if let Some(mut suggestion) = builtin_profile(email, domain) {
        suggestion.source = "profile".into();
        fill_usernames(&mut suggestion, email);
        return Ok(suggestion);
    }

    if let Ok(Some(mut suggestion)) = fetch_ispdb(domain, http).await {
        suggestion.source = "ispdb".into();
        fill_usernames(&mut suggestion, email);
        return Ok(suggestion);
    }

    if let Ok(Some(mut suggestion)) = lookup_srv(domain).await {
        suggestion.source = "srv".into();
        fill_usernames(&mut suggestion, email);
        return Ok(suggestion);
    }

    if let Some(mut suggestion) = probe::guess_hosts(domain) {
        suggestion.source = "probe".into();
        fill_usernames(&mut suggestion, email);
        return Ok(suggestion);
    }

    Ok(ServerSuggestion {
        imap: ServerEndpoint {
            host: format!("imap.{domain}"),
            port: 993,
            tls: TlsMode::Implicit,
            username: email.to_string(),
        },
        smtp: ServerEndpoint {
            host: format!("smtp.{domain}"),
            port: 587,
            tls: TlsMode::StartTls,
            username: email.to_string(),
        },
        auth: AuthKind::Password,
        oauth_provider: None,
        source: "manual".into(),
    })
}

fn domain_of(email: &str) -> Option<&str> {
    let at = email.rfind('@')?;
    let domain = email.get(at + 1..)?.trim();
    if domain.is_empty() {
        None
    } else {
        Some(domain)
    }
}

fn fill_usernames(suggestion: &mut ServerSuggestion, email: &str) {
    if suggestion.imap.username.is_empty() {
        suggestion.imap.username = email.to_string();
    }
    if suggestion.smtp.username.is_empty() {
        suggestion.smtp.username = email.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmail_profile() {
        let s = builtin_profile("user@gmail.com", "gmail.com").expect("gmail");
        assert_eq!(s.imap.host, "imap.gmail.com");
        assert_eq!(s.smtp.port, 587);
        assert_eq!(s.auth, AuthKind::Oauth2);
    }

    #[test]
    fn domain_parse() {
        assert_eq!(domain_of("a@b.co"), Some("b.co"));
        assert!(domain_of("nope").is_none());
    }
}
