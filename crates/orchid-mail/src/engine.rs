//! Shared mail engine used by every Mail widget instance.

use std::path::Path;
use std::sync::Arc;

use parking_lot::RwLock;
use secrecy::SecretString;
use uuid::Uuid;

use crate::account::{
    AuthKind, ComposeMessage, MailAccount, MailFolder, MessageBody, MessageHeader, ServerSuggestion,
};
use crate::autodiscover;
use crate::cache::MailCache;
use crate::error::{MailError, Result};
use crate::imap;
use crate::mime_util::{html_to_text, sanitize_html_for_webview};
use crate::oauth;
use crate::secrets::{AccountSecrets, MailSecretsStore};
use crate::smtp;
use crate::store::{ensure_mail_dir, AccountStore};

/// Process-wide mail service.
#[derive(Clone)]
pub struct MailEngine {
    accounts: Arc<AccountStore>,
    secrets: Arc<MailSecretsStore>,
    cache: Arc<MailCache>,
    http: reqwest::Client,
    /// Allow remote images in HTML reading pane.
    allow_remote_images: Arc<RwLock<bool>>,
}

impl MailEngine {
    /// Open the engine under `data_dir/mail`.
    pub fn open(data_dir: impl AsRef<Path>, http: reqwest::Client) -> Result<Arc<Self>> {
        let mail_dir = data_dir.as_ref().join("mail");
        ensure_mail_dir(&mail_dir)?;
        Ok(Arc::new(Self {
            accounts: AccountStore::open(&mail_dir)?,
            secrets: MailSecretsStore::open(&mail_dir)?,
            cache: MailCache::open(&mail_dir)?,
            http,
            allow_remote_images: Arc::new(RwLock::new(false)),
        }))
    }

    /// List configured accounts.
    pub fn accounts(&self) -> Vec<MailAccount> {
        self.accounts.list()
    }

    /// Account by id.
    pub fn account(&self, id: Uuid) -> Option<MailAccount> {
        self.accounts.get(id)
    }

    /// Whether HTML may load remote images.
    pub fn allow_remote_images(&self) -> bool {
        *self.allow_remote_images.read()
    }

    /// Toggle remote images for HTML bodies.
    pub fn set_allow_remote_images(&self, allow: bool) {
        *self.allow_remote_images.write() = allow;
    }

    /// Autodiscover servers for an email address.
    pub async fn discover(&self, email: &str) -> Result<ServerSuggestion> {
        autodiscover::discover(email, &self.http).await
    }

    /// Add a password-authenticated account after probing IMAP (and SMTP when possible).
    pub async fn add_password_account(
        &self,
        mut account: MailAccount,
        password: SecretString,
        skip_smtp_probe: bool,
    ) -> Result<MailAccount> {
        account.auth = AuthKind::Password;
        let secrets = AccountSecrets {
            password: Some({
                use secrecy::ExposeSecret;
                password.expose_secret().to_string()
            }),
            ..AccountSecrets::default()
        };
        imap::probe_login(&account, &secrets).await?;
        if !skip_smtp_probe {
            if let Err(e) = smtp::probe_smtp(&account, &secrets).await {
                tracing::warn!(error = %e, "smtp probe failed; account still saved");
            }
        }
        self.secrets.set_password(account.id, password)?;
        self.accounts.upsert(account.clone())?;
        Ok(account)
    }

    /// Start an OAuth browser flow; returns auth URL and a future for the code.
    pub async fn begin_oauth(
        &self,
        provider: &str,
    ) -> Result<(String, u16, oauth::AuthCodeReceiver)> {
        oauth::begin_auth_code_flow(provider).await
    }

    /// Finish OAuth and save the account.
    pub async fn finish_oauth_account(
        &self,
        mut account: MailAccount,
        provider: &str,
        code: &str,
        redirect_port: u16,
    ) -> Result<MailAccount> {
        account.auth = AuthKind::Oauth2;
        account.oauth_provider = Some(provider.to_string());
        let secrets = oauth::exchange_code(provider, code, redirect_port, &self.http).await?;
        let secrets =
            oauth::ensure_access_token(&account, &secrets, Some(&self.secrets), &self.http).await?;
        imap::probe_login(&account, &secrets).await?;
        self.secrets.set_oauth(
            account.id,
            SecretString::from(secrets.refresh_token.clone().unwrap_or_default()),
            secrets.access_token.clone().map(SecretString::from),
            secrets.access_expires_at,
        )?;
        self.accounts.upsert(account.clone())?;
        Ok(account)
    }

    /// Remove an account and its cache / secrets.
    pub fn remove_account(&self, id: Uuid) -> Result<()> {
        self.accounts.remove(id)?;
        self.secrets.remove(id)?;
        self.cache.purge_account(id)?;
        Ok(())
    }

    /// Sync folders + recent headers for one account/folder.
    pub async fn sync_account(&self, account_id: Uuid, folder: Option<&str>) -> Result<()> {
        let account = self
            .accounts
            .get(account_id)
            .ok_or_else(|| MailError::NotFound(account_id.to_string()))?;
        let mut secrets = self.secrets.get(account_id);
        if account.auth == AuthKind::Oauth2 {
            secrets =
                oauth::ensure_access_token(&account, &secrets, Some(&self.secrets), &self.http)
                    .await?;
        }
        let mut session = imap::connect(&account, &secrets).await?;
        let folders = imap::list_folders(&mut session, account_id).await?;
        self.cache.replace_folders(account_id, &folders)?;
        let folder = folder
            .map(str::to_string)
            .or_else(|| {
                folders
                    .iter()
                    .find(|f| f.role.as_deref() == Some("inbox"))
                    .map(|f| f.path.clone())
            })
            .unwrap_or_else(|| "INBOX".into());
        let headers = imap::fetch_headers(&mut session, account_id, &folder, 100).await?;
        self.cache.upsert_headers(&headers)?;
        let mut account = account;
        account.last_sync_at = Some(chrono::Utc::now());
        self.accounts.upsert(account)?;
        let _ = session.logout().await;
        Ok(())
    }

    /// Cached folders.
    pub fn folders(&self, account_id: Uuid) -> Result<Vec<MailFolder>> {
        self.cache.folders(account_id)
    }

    /// Cached headers.
    pub fn headers(
        &self,
        account_id: Uuid,
        folder: &str,
        limit: u32,
    ) -> Result<Vec<MessageHeader>> {
        self.cache.headers(account_id, folder, limit)
    }

    /// Load a body from cache or IMAP.
    pub async fn message_body(
        &self,
        account_id: Uuid,
        folder: &str,
        uid: u32,
    ) -> Result<MessageBody> {
        if let Some(body) = self.cache.body(account_id, folder, uid)? {
            return Ok(body);
        }
        let account = self
            .accounts
            .get(account_id)
            .ok_or_else(|| MailError::NotFound(account_id.to_string()))?;
        let mut secrets = self.secrets.get(account_id);
        if account.auth == AuthKind::Oauth2 {
            secrets =
                oauth::ensure_access_token(&account, &secrets, Some(&self.secrets), &self.http)
                    .await?;
        }
        let mut session = imap::connect(&account, &secrets).await?;
        let body = imap::fetch_body(&mut session, account_id, folder, uid).await?;
        self.cache.put_body(&body)?;
        let _ = imap::set_flag(&mut session, folder, uid, "\\Seen", true).await;
        let _ = self
            .cache
            .set_flags(account_id, folder, uid, Some(true), None);
        let _ = session.logout().await;
        Ok(body)
    }

    /// Plain text suitable for the Slint reading pane.
    pub fn reading_text(&self, body: &MessageBody) -> String {
        if !body.text.trim().is_empty() {
            body.text.clone()
        } else if !body.html.trim().is_empty() {
            html_to_text(&body.html)
        } else {
            String::new()
        }
    }

    /// HTML document for WebView2.
    pub fn reading_html_document(&self, body: &MessageBody) -> Option<String> {
        if body.html.trim().is_empty() {
            return None;
        }
        Some(sanitize_html_for_webview(
            &body.html,
            self.allow_remote_images(),
        ))
    }

    /// Toggle seen flag.
    pub async fn set_seen(
        &self,
        account_id: Uuid,
        folder: &str,
        uid: u32,
        seen: bool,
    ) -> Result<()> {
        let account = self
            .accounts
            .get(account_id)
            .ok_or_else(|| MailError::NotFound(account_id.to_string()))?;
        let mut secrets = self.secrets.get(account_id);
        if account.auth == AuthKind::Oauth2 {
            secrets =
                oauth::ensure_access_token(&account, &secrets, Some(&self.secrets), &self.http)
                    .await?;
        }
        let mut session = imap::connect(&account, &secrets).await?;
        let result = imap::set_flag(&mut session, folder, uid, "\\Seen", seen).await;
        let _ = session.logout().await;
        result?;
        self.cache
            .set_flags(account_id, folder, uid, Some(seen), None)?;
        Ok(())
    }

    /// Toggle flagged.
    pub async fn set_flagged(
        &self,
        account_id: Uuid,
        folder: &str,
        uid: u32,
        flagged: bool,
    ) -> Result<()> {
        let account = self
            .accounts
            .get(account_id)
            .ok_or_else(|| MailError::NotFound(account_id.to_string()))?;
        let mut secrets = self.secrets.get(account_id);
        if account.auth == AuthKind::Oauth2 {
            secrets =
                oauth::ensure_access_token(&account, &secrets, Some(&self.secrets), &self.http)
                    .await?;
        }
        let mut session = imap::connect(&account, &secrets).await?;
        let result = imap::set_flag(&mut session, folder, uid, "\\Flagged", flagged).await;
        let _ = session.logout().await;
        result?;
        self.cache
            .set_flags(account_id, folder, uid, None, Some(flagged))?;
        Ok(())
    }

    /// Move to trash / destination.
    pub async fn move_message(
        &self,
        account_id: Uuid,
        folder: &str,
        uid: u32,
        dest: &str,
    ) -> Result<()> {
        let account = self
            .accounts
            .get(account_id)
            .ok_or_else(|| MailError::NotFound(account_id.to_string()))?;
        let mut secrets = self.secrets.get(account_id);
        if account.auth == AuthKind::Oauth2 {
            secrets =
                oauth::ensure_access_token(&account, &secrets, Some(&self.secrets), &self.http)
                    .await?;
        }
        let mut session = imap::connect(&account, &secrets).await?;
        let result = imap::move_message(&mut session, folder, uid, dest).await;
        let _ = session.logout().await;
        result?;
        self.cache.delete_message(account_id, folder, uid)?;
        Ok(())
    }

    /// Send a message via SMTP.
    pub async fn send(&self, compose: &ComposeMessage) -> Result<()> {
        let account_id = compose
            .account_id
            .ok_or_else(|| MailError::Invalid("compose needs an account".into()))?;
        let account = self
            .accounts
            .get(account_id)
            .ok_or_else(|| MailError::NotFound(account_id.to_string()))?;
        let secrets = self.secrets.get(account_id);
        smtp::send(&account, &secrets, compose, Some(&self.secrets), &self.http).await
    }

    /// Save a draft via IMAP APPEND.
    pub async fn save_draft(&self, compose: &ComposeMessage) -> Result<()> {
        let account_id = compose
            .account_id
            .ok_or_else(|| MailError::Invalid("draft needs an account".into()))?;
        let account = self
            .accounts
            .get(account_id)
            .ok_or_else(|| MailError::NotFound(account_id.to_string()))?;
        let mut secrets = self.secrets.get(account_id);
        if account.auth == AuthKind::Oauth2 {
            secrets =
                oauth::ensure_access_token(&account, &secrets, Some(&self.secrets), &self.http)
                    .await?;
        }
        let from_name = if account.identity_name.is_empty() {
            account.display_name.as_str()
        } else {
            account.identity_name.as_str()
        };
        let raw = crate::mime_util::build_rfc822(from_name, &account.email, compose)?;
        let drafts = self
            .cache
            .folders(account_id)?
            .into_iter()
            .find(|f| f.role.as_deref() == Some("drafts"))
            .map(|f| f.path)
            .unwrap_or_else(|| "Drafts".into());
        let mut session = imap::connect(&account, &secrets).await?;
        imap::append_message(&mut session, &drafts, &raw, true).await?;
        let _ = session.logout().await;
        Ok(())
    }
}
