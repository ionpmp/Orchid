//! IMAP session helpers.

use std::collections::HashSet;

use async_imap::types::{Fetch, NameAttribute};
use async_imap::{Authenticator, Client, Session};
use futures::TryStreamExt;
use secrecy::{ExposeSecret, SecretString};
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use uuid::Uuid;

use crate::account::{AuthKind, MailAccount, MailFolder, MessageHeader, TlsMode};
use crate::error::{MailError, Result};
use crate::mime_util::{header_fields, parse_rfc822};
use crate::secrets::AccountSecrets;
use crate::tls;

type ImapSession = Session<TlsStream<TcpStream>>;

struct Xoauth2 {
    user: String,
    token: String,
}

impl Authenticator for Xoauth2 {
    type Response = Vec<u8>;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        format!("user={}\x01auth=Bearer {}\x01\x01", self.user, self.token).into_bytes()
    }
}

/// Open an authenticated IMAP session for `account`.
pub async fn connect(account: &MailAccount, secrets: &AccountSecrets) -> Result<ImapSession> {
    if account.imap.tls != TlsMode::Implicit {
        return Err(MailError::Imap(
            "only implicit TLS IMAP (port 993) is supported in this build".into(),
        ));
    }
    let tls = tls::connect_tls(&account.imap.host, account.imap.port).await?;
    let client = Client::new(tls);
    login(client, account, secrets).await
}

async fn login<T>(
    client: Client<T>,
    account: &MailAccount,
    secrets: &AccountSecrets,
) -> Result<Session<T>>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    match account.auth {
        AuthKind::Password => {
            let password = secrets
                .password
                .as_ref()
                .ok_or_else(|| MailError::Invalid("missing mail password".into()))?;
            client
                .login(&account.imap.username, password)
                .await
                .map_err(|(e, _)| MailError::Imap(e.to_string()))
        }
        AuthKind::Oauth2 => {
            let token = secrets
                .access_token
                .as_ref()
                .ok_or_else(|| MailError::Oauth("missing access token".into()))?;
            let auth = Xoauth2 {
                user: account.imap.username.clone(),
                token: token.clone(),
            };
            client
                .authenticate("XOAUTH2", auth)
                .await
                .map_err(|(e, _)| MailError::Imap(e.to_string()))
        }
    }
}

/// Verify credentials by connecting and listing INBOX.
pub async fn probe_login(account: &MailAccount, secrets: &AccountSecrets) -> Result<()> {
    let mut session = connect(account, secrets).await?;
    let _ = session.select("INBOX").await?;
    let _ = session.logout().await;
    Ok(())
}

/// List mailboxes and map SPECIAL-USE roles.
pub async fn list_folders(session: &mut ImapSession, account_id: Uuid) -> Result<Vec<MailFolder>> {
    let list = session.list(Some(""), Some("*")).await?;
    let names: Vec<_> = list.try_collect().await.map_err(MailError::from)?;
    let mut folders = Vec::new();
    for name in names {
        let path = name.name().to_string();
        if path.is_empty() {
            continue;
        }
        let depth = path.matches(name.delimiter().unwrap_or("/")).count() as u32;
        let name_label = path
            .rsplit(name.delimiter().unwrap_or("/"))
            .next()
            .unwrap_or(path.as_str())
            .to_string();
        let role = special_use_role(name.attributes());
        let (unread, total) = status_counts(session, &path).await.unwrap_or((0, 0));
        folders.push(MailFolder {
            account_id,
            path,
            name: name_label,
            depth,
            role,
            unread,
            total,
        });
    }
    if folders.is_empty() {
        folders.push(MailFolder {
            account_id,
            path: "INBOX".into(),
            name: "INBOX".into(),
            depth: 0,
            role: Some("inbox".into()),
            unread: 0,
            total: 0,
        });
    }
    Ok(folders)
}

fn special_use_role(attrs: &[NameAttribute<'_>]) -> Option<String> {
    for attr in attrs {
        let role = match attr {
            NameAttribute::All | NameAttribute::Archive => Some("archive"),
            NameAttribute::Drafts => Some("drafts"),
            NameAttribute::Flagged => Some("flagged"),
            NameAttribute::Junk => Some("junk"),
            NameAttribute::Sent => Some("sent"),
            NameAttribute::Trash => Some("trash"),
            NameAttribute::Extension(ext) if ext.eq_ignore_ascii_case("\\Inbox") => Some("inbox"),
            _ => None,
        };
        if role.is_some() {
            return role.map(str::to_string);
        }
    }
    None
}

async fn status_counts(session: &mut ImapSession, path: &str) -> Result<(u32, u32)> {
    // SELECT is heavier but widely supported; STATUS may lack UNSEEN on some servers.
    let mailbox = session.select(path).await?;
    let total = mailbox.exists;
    // Approximate unread via SEARCH UNSEEN (best effort).
    let unread = match session.search("UNSEEN").await {
        Ok(uids) => uids.len() as u32,
        Err(_) => 0,
    };
    Ok((unread, total))
}

/// Fetch recent message headers (UID-based) into the cache shape.
pub async fn fetch_headers(
    session: &mut ImapSession,
    account_id: Uuid,
    folder: &str,
    limit: u32,
) -> Result<Vec<MessageHeader>> {
    let mailbox = session.select(folder).await?;
    if mailbox.exists == 0 {
        return Ok(Vec::new());
    }
    let start = mailbox.exists.saturating_sub(limit).saturating_add(1);
    let sequence = format!("{start}:*");
    let fetches = session
        .fetch(&sequence, "(UID FLAGS BODY.PEEK[HEADER] BODYSTRUCTURE)")
        .await?;
    let fetches: Vec<Fetch> = fetches.try_collect().await.map_err(MailError::from)?;

    let mut headers = Vec::new();
    for fetch in fetches {
        let uid = match fetch.uid {
            Some(u) => u,
            None => continue,
        };
        let flags: HashSet<String> = fetch.flags().map(|f| format!("{f:?}")).collect();
        let seen = flags.iter().any(|f| f.contains("Seen"));
        let flagged = flags.iter().any(|f| f.contains("Flagged"));
        let raw = fetch.header().unwrap_or_default();
        let (from, to, subject, date, date_unix, message_id, has_attachment) = header_fields(raw);
        headers.push(MessageHeader {
            account_id,
            folder: folder.to_string(),
            uid,
            message_id,
            from,
            to,
            subject,
            date,
            date_unix,
            seen,
            flagged,
            has_attachment,
            snippet: String::new(),
        });
    }
    headers.sort_by(|a, b| b.date_unix.cmp(&a.date_unix).then(b.uid.cmp(&a.uid)));
    Ok(headers)
}

/// Fetch one full RFC822 body.
pub async fn fetch_body(
    session: &mut ImapSession,
    account_id: Uuid,
    folder: &str,
    uid: u32,
) -> Result<crate::account::MessageBody> {
    session.select(folder).await?;
    let fetches = session.uid_fetch(uid.to_string(), "BODY.PEEK[]").await?;
    let fetches: Vec<Fetch> = fetches.try_collect().await.map_err(MailError::from)?;
    let fetch = fetches
        .into_iter()
        .next()
        .ok_or_else(|| MailError::NotFound(format!("uid {uid}")))?;
    let raw = fetch
        .body()
        .ok_or_else(|| MailError::Imap("empty body".into()))?;
    Ok(parse_rfc822(account_id, folder, uid, raw))
}

/// Store / clear flags.
pub async fn set_flag(
    session: &mut ImapSession,
    folder: &str,
    uid: u32,
    flag: &str,
    add: bool,
) -> Result<()> {
    session.select(folder).await?;
    let query = if add {
        format!("+FLAGS ({flag})")
    } else {
        format!("-FLAGS ({flag})")
    };
    let stream = session.uid_store(uid.to_string(), &query).await?;
    let _: Vec<_> = stream.try_collect().await.map_err(MailError::from)?;
    Ok(())
}

/// Move (COPY + delete) a message.
pub async fn move_message(
    session: &mut ImapSession,
    folder: &str,
    uid: u32,
    dest: &str,
) -> Result<()> {
    session.select(folder).await?;
    session.uid_copy(uid.to_string(), dest).await?;
    let stream = session
        .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
        .await?;
    let _: Vec<_> = stream.try_collect().await.map_err(MailError::from)?;
    let _ = session.expunge().await;
    Ok(())
}

/// Append a message (e.g. draft) to a mailbox.
pub async fn append_message(
    session: &mut ImapSession,
    folder: &str,
    rfc822: &[u8],
    draft: bool,
) -> Result<()> {
    let flags = if draft { Some("(\\Draft)") } else { None };
    session
        .append(folder, flags, None, rfc822)
        .await
        .map_err(MailError::from)?;
    Ok(())
}

/// Password helper for tests / probe.
#[must_use]
pub fn secret_password(password: impl Into<String>) -> AccountSecrets {
    AccountSecrets {
        password: Some(password.into()),
        ..AccountSecrets::default()
    }
}

/// Expose password from SecretString.
#[must_use]
pub fn password_secret(password: &SecretString) -> AccountSecrets {
    AccountSecrets {
        password: Some(password.expose_secret().to_string()),
        ..AccountSecrets::default()
    }
}
