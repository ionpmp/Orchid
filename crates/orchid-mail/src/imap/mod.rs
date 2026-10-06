//! IMAP session helpers.

use std::collections::HashSet;
use std::time::Duration;

use async_imap::extensions::idle::IdleResponse;
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

/// Authenticated IMAP session over TLS.
pub type ImapSession = Session<TlsStream<TcpStream>>;

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
///
/// Implicit TLS uses the configured port as-is. STARTTLS connects in the
/// clear, reads the greeting, upgrades, and only then sends the password.
/// Cleartext login is refused.
pub async fn connect(account: &MailAccount, secrets: &AccountSecrets) -> Result<ImapSession> {
    match account.imap.tls {
        TlsMode::Implicit => {
            let tls = tls::connect_tls(&account.imap.host, account.imap.port).await?;
            let client = Client::new(tls);
            login(client, account, secrets).await
        }
        TlsMode::StartTls => {
            let tcp = tls::connect_tcp(&account.imap.host, account.imap.port).await?;
            let tcp = begin_starttls(tcp).await?;
            let tls = tls::upgrade(&account.imap.host, tcp).await?;
            let client = Client::new(tls);
            login(client, account, secrets).await
        }
        TlsMode::None => Err(MailError::Imap("cleartext IMAP login is not used".into())),
    }
}

/// Read the greeting and complete the STARTTLS command. The returned stream
/// is still cleartext and must be wrapped in TLS before LOGIN.
pub async fn begin_starttls(stream: TcpStream) -> Result<TcpStream> {
    let mut client = Client::new(stream);
    client
        .read_response()
        .await?
        .ok_or_else(|| MailError::Imap("missing IMAP greeting".into()))?;
    client
        .run_command_and_check_ok("STARTTLS", None)
        .await
        .map_err(MailError::from)?;
    Ok(client.into_inner())
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

    Ok(headers_from_fetches(fetches, account_id, folder))
}

fn headers_from_fetches(fetches: Vec<Fetch>, account_id: Uuid, folder: &str) -> Vec<MessageHeader> {
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
    headers
}

/// Search with IMAP `TEXT` and return at most `limit` matching headers.
///
/// The query is one quoted string. Control characters are rejected so the
/// text cannot add another search key.
pub async fn search_headers(
    session: &mut ImapSession,
    account_id: Uuid,
    folder: &str,
    query: &str,
    limit: u32,
) -> Result<Vec<MessageHeader>> {
    let quoted = crate::quote_imap(query)
        .ok_or_else(|| MailError::Imap("search text is empty or has a control character".into()))?;
    session.select(folder).await?;
    let found = session
        .uid_search(format!("TEXT {quoted}"))
        .await
        .map_err(MailError::from)?;
    if found.is_empty() {
        return Ok(Vec::new());
    }
    let mut uids: Vec<_> = found.into_iter().collect();
    uids.sort_unstable();
    uids.reverse();
    uids.truncate(limit as usize);
    let set = uids
        .iter()
        .map(|uid| uid.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let fetches = session
        .uid_fetch(set, "(UID FLAGS BODY.PEEK[HEADER])")
        .await?;
    let fetches: Vec<Fetch> = fetches.try_collect().await.map_err(MailError::from)?;
    Ok(headers_from_fetches(fetches, account_id, folder))
}

/// SELECT `folder` and IDLE until `limit` or the first mailbox change.
///
/// `Ok(false)` means IDLE is unavailable or nothing changed. The password is
/// never sent on a connection that failed to upgrade.
pub async fn idle_for(session: ImapSession, folder: &str, limit: Duration) -> Result<bool> {
    idle_session(session, folder, limit).await
}

async fn idle_session<T>(mut session: Session<T>, folder: &str, limit: Duration) -> Result<bool>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    session.select(folder).await?;
    let mut idle = session.idle();
    if idle.init().await.is_err() {
        return Ok(false);
    }
    let (wait, _stop) = idle.wait_with_timeout(limit);
    let response = match wait.await {
        Ok(response) => response,
        Err(_) => return Ok(false),
    };
    if let Ok(mut session) = idle.done().await {
        let _ = session.logout().await;
    }
    Ok(matches!(response, IdleResponse::NewData(_)))
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
    store_flag_set(session, folder, &uid.to_string(), flag, add).await
}

/// Add or remove one flag on several UIDs in a single command.
pub async fn set_flag_uids(
    session: &mut ImapSession,
    folder: &str,
    uids: &[u32],
    flag: &str,
    add: bool,
) -> Result<()> {
    if uids.is_empty() {
        return Ok(());
    }
    store_flag_set(session, folder, &uid_set(uids), flag, add).await
}

async fn store_flag_set(
    session: &mut ImapSession,
    folder: &str,
    set: &str,
    flag: &str,
    add: bool,
) -> Result<()> {
    session.select(folder).await?;
    let query = if add {
        format!("+FLAGS.SILENT ({flag})")
    } else {
        format!("-FLAGS.SILENT ({flag})")
    };
    let stream = session.uid_store(set, &query).await?;
    let _: Vec<_> = stream.try_collect().await.map_err(MailError::from)?;
    Ok(())
}

/// Comma-separated IMAP UID set.
pub fn uid_set(uids: &[u32]) -> String {
    uids.iter()
        .map(|uid| uid.to_string())
        .collect::<Vec<_>>()
        .join(",")
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn uid_set_joins_the_listed_ids() {
        assert_eq!(uid_set(&[3, 8, 21]), "3,8,21");
        assert_eq!(uid_set(&[]), "");
    }
    use tokio::net::TcpListener;

    async fn read_line(sock: &mut TcpStream) -> String {
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let n = sock.read(&mut byte).await.unwrap();
            if n == 0 {
                break;
            }
            buf.push(byte[0]);
            if buf.ends_with(b"\r\n") {
                break;
            }
        }
        String::from_utf8_lossy(&buf).trim().to_string()
    }

    fn tag_of(line: &str) -> &str {
        line.split_whitespace().next().unwrap_or("A")
    }

    #[tokio::test]
    async fn starttls_reads_the_greeting_before_the_command() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            sock.write_all(b"* OK ready\r\n").await.unwrap();
            let line = read_line(&mut sock).await;
            assert!(line.to_ascii_uppercase().contains("STARTTLS"), "{line}");
            let reply = format!("{} OK begin TLS\r\n", tag_of(&line));
            sock.write_all(reply.as_bytes()).await.unwrap();
            let _ = read_line(&mut sock).await;
        });
        let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        begin_starttls(tcp).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn cleartext_login_is_refused() {
        let mut account = MailAccount::new("ada@example.com", "Ada");
        account.imap.tls = TlsMode::None;
        account.imap.host = "127.0.0.1".into();
        account.imap.port = 1;
        let err = connect(&account, &AccountSecrets::default())
            .await
            .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("cleartext"), "{text}");
    }

    #[tokio::test]
    async fn idle_reports_a_new_message() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            sock.write_all(b"* OK ready\r\n").await.unwrap();
            loop {
                let line = read_line(&mut sock).await;
                if line.is_empty() {
                    break;
                }
                let tag = tag_of(&line).to_string();
                let upper = line.to_ascii_uppercase();
                if upper.contains(" SELECT ") {
                    let reply = format!("* 1 EXISTS\r\n{tag} OK [READ-WRITE] selected\r\n");
                    sock.write_all(reply.as_bytes()).await.unwrap();
                } else if upper.contains(" IDLE") {
                    sock.write_all(b"+ idling\r\n").await.unwrap();
                    sock.write_all(b"* 2 EXISTS\r\n").await.unwrap();
                    let done = read_line(&mut sock).await;
                    assert!(done.to_ascii_uppercase().contains("DONE"), "{done}");
                    let reply = format!("{tag} OK idle done\r\n");
                    sock.write_all(reply.as_bytes()).await.unwrap();
                } else {
                    let reply = format!("{tag} OK\r\n");
                    sock.write_all(reply.as_bytes()).await.unwrap();
                }
            }
        });
        let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let mut account = MailAccount::new("ada@example.com", "Ada");
        account.imap.username = "ada".into();
        let secrets = secret_password("secret");
        let session = login(Client::new(tcp), &account, &secrets).await.unwrap();
        let changed = idle_session(session, "INBOX", Duration::from_secs(5))
            .await
            .unwrap();
        assert!(changed);
        server.await.unwrap();
    }
}
