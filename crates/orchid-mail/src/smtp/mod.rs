//! SMTP send via lettre.

use lettre::address::Envelope;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use secrecy::{ExposeSecret, SecretString};

use crate::account::{AuthKind, ComposeMessage, MailAccount, TlsMode};
use crate::error::{MailError, Result};
use crate::mime_util::build_rfc822;
use crate::oauth;
use crate::secrets::{AccountSecrets, MailSecretsStore};

/// Send `compose` through the account's SMTP server.
pub async fn send(
    account: &MailAccount,
    secrets: &AccountSecrets,
    compose: &ComposeMessage,
    secrets_store: Option<&MailSecretsStore>,
    http: &reqwest::Client,
) -> Result<()> {
    let mut secrets = secrets.clone();
    if account.auth == AuthKind::Oauth2 {
        secrets = oauth::ensure_access_token(account, &secrets, secrets_store, http).await?;
    }

    let from_name = if account.identity_name.is_empty() {
        account.display_name.as_str()
    } else {
        account.identity_name.as_str()
    };
    let raw = build_rfc822(from_name, &account.email, compose)?;
    let from: lettre::Address = account
        .email
        .parse()
        .map_err(|e: lettre::address::AddressError| MailError::Invalid(e.to_string()))?;
    let mut to = Vec::new();
    for addr in split_addrs(&compose.to)
        .into_iter()
        .chain(split_addrs(&compose.cc))
        .chain(split_addrs(&compose.bcc))
    {
        to.push(
            addr.parse()
                .map_err(|e: lettre::address::AddressError| MailError::Invalid(e.to_string()))?,
        );
    }
    if to.is_empty() {
        return Err(MailError::Invalid("at least one recipient required".into()));
    }
    let envelope = Envelope::new(Some(from), to).map_err(|e| MailError::Smtp(e.to_string()))?;

    let transport = build_transport(account, &secrets)?;
    transport
        .send_raw(&envelope, &raw)
        .await
        .map_err(|e| MailError::Smtp(e.to_string()))?;
    Ok(())
}

/// Verify SMTP credentials with a NOOP-style connect.
pub async fn probe_smtp(account: &MailAccount, secrets: &AccountSecrets) -> Result<()> {
    let transport = build_transport(account, secrets)?;
    transport
        .test_connection()
        .await
        .map_err(|e| MailError::Smtp(e.to_string()))?;
    Ok(())
}

fn build_transport(
    account: &MailAccount,
    secrets: &AccountSecrets,
) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    let builder = match account.smtp.tls {
        TlsMode::Implicit => AsyncSmtpTransport::<Tokio1Executor>::relay(&account.smtp.host)
            .map_err(|e| MailError::Smtp(e.to_string()))?
            .port(account.smtp.port),
        TlsMode::StartTls => {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&account.smtp.host)
                .map_err(|e| MailError::Smtp(e.to_string()))?
                .port(account.smtp.port)
        }
        TlsMode::None => {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&account.smtp.host)
                .port(account.smtp.port)
        }
    };

    let builder = match account.auth {
        AuthKind::Password => {
            let password = secrets
                .password
                .as_ref()
                .ok_or_else(|| MailError::Invalid("missing mail password".into()))?;
            builder.credentials(Credentials::new(
                account.smtp.username.clone(),
                password.clone(),
            ))
        }
        AuthKind::Oauth2 => {
            let token = secrets
                .access_token
                .as_ref()
                .ok_or_else(|| MailError::Oauth("missing access token".into()))?;
            builder
                .authentication(vec![Mechanism::Xoauth2])
                .credentials(Credentials::new(
                    account.smtp.username.clone(),
                    token.clone(),
                ))
        }
    };

    Ok(builder.build())
}

fn split_addrs(raw: &str) -> Vec<String> {
    raw.split([',', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Parse a mailbox address for validation helpers.
pub fn parse_mailbox(addr: &str) -> Result<Mailbox> {
    addr.parse()
        .map_err(|e: lettre::address::AddressError| MailError::Invalid(e.to_string()))
}

/// Wrap a clear password.
#[must_use]
pub fn password_from(secret: &SecretString) -> String {
    secret.expose_secret().to_string()
}
