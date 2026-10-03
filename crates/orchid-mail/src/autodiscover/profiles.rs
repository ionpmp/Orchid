//! Built-in ISP profiles for common consumer mail providers.

use crate::account::{AuthKind, ServerEndpoint, ServerSuggestion, TlsMode};

/// Return a known profile for `domain` (lowercase), if any.
#[must_use]
pub fn builtin_profile(email: &str, domain: &str) -> Option<ServerSuggestion> {
    let domain = domain.to_ascii_lowercase();
    let email = email.to_string();
    let (imap_host, imap_port, smtp_host, smtp_port, smtp_tls, auth, oauth) = match domain.as_str()
    {
        "gmail.com" | "googlemail.com" => (
            "imap.gmail.com",
            993,
            "smtp.gmail.com",
            587,
            TlsMode::StartTls,
            AuthKind::Oauth2,
            Some("google"),
        ),
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" | "office365.com" => (
            "outlook.office365.com",
            993,
            "smtp.office365.com",
            587,
            TlsMode::StartTls,
            AuthKind::Oauth2,
            Some("microsoft"),
        ),
        "yahoo.com" | "ymail.com" | "rocketmail.com" => (
            "imap.mail.yahoo.com",
            993,
            "smtp.mail.yahoo.com",
            465,
            TlsMode::Implicit,
            AuthKind::Password,
            None,
        ),
        "icloud.com" | "me.com" | "mac.com" => (
            "imap.mail.me.com",
            993,
            "smtp.mail.me.com",
            587,
            TlsMode::StartTls,
            AuthKind::Password,
            None,
        ),
        "yandex.ru" | "yandex.com" | "ya.ru" => (
            "imap.yandex.com",
            993,
            "smtp.yandex.com",
            465,
            TlsMode::Implicit,
            AuthKind::Password,
            None,
        ),
        "mail.ru" | "inbox.ru" | "bk.ru" | "list.ru" => (
            "imap.mail.ru",
            993,
            "smtp.mail.ru",
            465,
            TlsMode::Implicit,
            AuthKind::Password,
            None,
        ),
        "gmx.com" | "gmx.net" | "gmx.de" => (
            "imap.gmx.com",
            993,
            "mail.gmx.com",
            587,
            TlsMode::StartTls,
            AuthKind::Password,
            None,
        ),
        "fastmail.com" | "fastmail.fm" => (
            "imap.fastmail.com",
            993,
            "smtp.fastmail.com",
            465,
            TlsMode::Implicit,
            AuthKind::Password,
            None,
        ),
        "zoho.com" | "zohomail.com" => (
            "imap.zoho.com",
            993,
            "smtp.zoho.com",
            465,
            TlsMode::Implicit,
            AuthKind::Password,
            None,
        ),
        "protonmail.com" | "proton.me" | "pm.me" => (
            "127.0.0.1",
            1143,
            "127.0.0.1",
            1025,
            TlsMode::None,
            AuthKind::Password,
            None,
        ),
        _ => return None,
    };

    // Proton Bridge is local-only; still expose the profile so the wizard can
    // tell the user to start Bridge and use these ports.
    let _ = domain;

    Some(ServerSuggestion {
        imap: ServerEndpoint {
            host: imap_host.into(),
            port: imap_port,
            tls: TlsMode::Implicit,
            username: email.clone(),
        },
        smtp: ServerEndpoint {
            host: smtp_host.into(),
            port: smtp_port,
            tls: smtp_tls,
            username: email,
        },
        auth,
        oauth_provider: oauth.map(str::to_string),
        source: String::new(),
    })
}
