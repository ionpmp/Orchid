//! OAuth2 for Gmail and Microsoft 365 (public client + localhost redirect).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use secrecy::SecretString;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use url::Url;

use crate::account::MailAccount;
use crate::error::{MailError, Result};
use crate::secrets::{AccountSecrets, MailSecretsStore};

/// Google OAuth client id (override with `ORCHID_MAIL_GOOGLE_CLIENT_ID`).
pub fn google_client_id() -> String {
    std::env::var("ORCHID_MAIL_GOOGLE_CLIENT_ID").unwrap_or_default()
}

/// Microsoft public client id (override with `ORCHID_MAIL_MS_CLIENT_ID`).
pub fn microsoft_client_id() -> String {
    std::env::var("ORCHID_MAIL_MS_CLIENT_ID")
        .unwrap_or_else(|_| "9e5f94bc-e8a4-4e73-b8be-63364c29d753".into())
}

/// Scopes for IMAP/SMTP.
#[must_use]
pub fn scopes_for(provider: &str) -> &'static [&'static str] {
    match provider {
        "google" => &["https://mail.google.com/", "openid", "email"],
        "microsoft" => &[
            "https://outlook.office.com/IMAP.AccessAsUser.All",
            "https://outlook.office.com/SMTP.Send",
            "offline_access",
            "openid",
            "email",
        ],
        _ => &[],
    }
}

fn token_url(provider: &str) -> Result<&'static str> {
    match provider {
        "google" => Ok("https://oauth2.googleapis.com/token"),
        "microsoft" => Ok("https://login.microsoftonline.com/common/oauth2/v2.0/token"),
        other => Err(MailError::Oauth(format!("unknown provider {other}"))),
    }
}

fn auth_url(provider: &str) -> Result<&'static str> {
    match provider {
        "google" => Ok("https://accounts.google.com/o/oauth2/v2/auth"),
        "microsoft" => Ok("https://login.microsoftonline.com/common/oauth2/v2.0/authorize"),
        other => Err(MailError::Oauth(format!("unknown provider {other}"))),
    }
}

fn client_id_for(provider: &str) -> Result<String> {
    let id = match provider {
        "google" => google_client_id(),
        "microsoft" => microsoft_client_id(),
        other => return Err(MailError::Oauth(format!("unknown provider {other}"))),
    };
    if id.is_empty() {
        return Err(MailError::Oauth(format!(
            "set ORCHID_MAIL_{}_CLIENT_ID for OAuth",
            provider.to_ascii_uppercase()
        )));
    }
    Ok(id)
}

/// Build the browser authorization URL and the localhost redirect port.
pub async fn begin_auth_code_flow(
    provider: &str,
) -> Result<(String, u16, oneshot::Receiver<String>)> {
    let client_id = client_id_for(provider)?;
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
    let port = listener.local_addr()?.port();
    let redirect = format!("http://127.0.0.1:{port}/oauth");
    let (tx, rx) = oneshot::channel();
    tokio::spawn(async move {
        if let Ok(code) = accept_redirect(listener).await {
            let _ = tx.send(code);
        }
    });

    let mut url = Url::parse(auth_url(provider)?).map_err(|e| MailError::Oauth(e.to_string()))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", &client_id);
        q.append_pair("redirect_uri", &redirect);
        q.append_pair("response_type", "code");
        q.append_pair("scope", &scopes_for(provider).join(" "));
        q.append_pair("access_type", "offline");
        q.append_pair("prompt", "consent");
    }
    Ok((url.to_string(), port, rx))
}

async fn accept_redirect(listener: TcpListener) -> Result<String> {
    let (mut socket, _) = listener.accept().await?;
    let mut buf = vec![0u8; 4096];
    let n = socket.read(&mut buf).await?;
    let req = String::from_utf8_lossy(&buf[..n]);
    let line = req.lines().next().unwrap_or("");
    let path = line.split_whitespace().nth(1).unwrap_or("/");
    let url = format!("http://127.0.0.1{path}");
    let parsed = Url::parse(&url).map_err(|e| MailError::Oauth(e.to_string()))?;
    let code = parsed
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(|| MailError::Oauth("authorization code missing".into()))?;
    let body = b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<html><body><h1>Orchid mail</h1><p>You can close this window.</p></body></html>";
    let _ = socket.write_all(body).await;
    Ok(code)
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// Exchange an auth code for tokens and return secrets.
pub async fn exchange_code(
    provider: &str,
    code: &str,
    redirect_port: u16,
    http: &reqwest::Client,
) -> Result<AccountSecrets> {
    let client_id = client_id_for(provider)?;
    let redirect = format!("http://127.0.0.1:{redirect_port}/oauth");
    let mut form = HashMap::new();
    form.insert("client_id", client_id);
    form.insert("code", code.to_string());
    form.insert("grant_type", "authorization_code".into());
    form.insert("redirect_uri", redirect);

    let response = http
        .post(token_url(provider)?)
        .form(&form)
        .send()
        .await?
        .error_for_status()?;
    let tokens: TokenResponse = response.json().await?;
    let expires = tokens.expires_in.map(|s| now_unix() + s);
    Ok(AccountSecrets {
        password: None,
        refresh_token: tokens.refresh_token,
        access_token: Some(tokens.access_token),
        access_expires_at: expires,
    })
}

/// Refresh the access token when expired; optionally persist.
pub async fn ensure_access_token(
    account: &MailAccount,
    secrets: &AccountSecrets,
    store: Option<&MailSecretsStore>,
    http: &reqwest::Client,
) -> Result<AccountSecrets> {
    let provider = account
        .oauth_provider
        .as_deref()
        .ok_or_else(|| MailError::Oauth("account has no oauth provider".into()))?;

    if let (Some(token), Some(exp)) = (&secrets.access_token, secrets.access_expires_at) {
        if now_unix() + 60 < exp && !token.is_empty() {
            return Ok(secrets.clone());
        }
    }

    let refresh = secrets
        .refresh_token
        .as_ref()
        .ok_or_else(|| MailError::Oauth("missing refresh token".into()))?;
    let client_id = client_id_for(provider)?;
    let mut form = HashMap::new();
    form.insert("client_id", client_id);
    form.insert("refresh_token", refresh.clone());
    form.insert("grant_type", "refresh_token".into());

    let response = http
        .post(token_url(provider)?)
        .form(&form)
        .send()
        .await?
        .error_for_status()?;
    let tokens: TokenResponse = response.json().await?;
    let expires = tokens.expires_in.map(|s| now_unix() + s);
    let mut updated = secrets.clone();
    updated.access_token = Some(tokens.access_token);
    updated.access_expires_at = expires;
    if let Some(r) = tokens.refresh_token {
        updated.refresh_token = Some(r);
    }
    if let Some(store) = store {
        if let Some(refresh) = updated.refresh_token.clone() {
            store.set_oauth(
                account.id,
                SecretString::from(refresh),
                updated.access_token.clone().map(SecretString::from),
                updated.access_expires_at,
            )?;
        }
    }
    Ok(updated)
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs() as i64
}

/// Shared handle used by the wizard while waiting for the redirect.
pub type AuthCodeReceiver = oneshot::Receiver<String>;

/// Placeholder so callers can hold the listener lifetime conceptually.
#[derive(Clone)]
pub struct OauthPending {
    /// Provider id.
    pub provider: String,
    /// Local redirect port.
    pub port: u16,
}
