//! DPAPI-backed storage for mail passwords and OAuth tokens.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{MailError, Result};

const SECRETS_FILE: &str = "secrets.dpapi";
const DPAPI_DESC: &str = "Orchid mail account secrets";

/// One account's secret material.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountSecrets {
    /// Password or app password when using password auth.
    #[serde(default)]
    pub password: Option<String>,
    /// OAuth refresh token.
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Cached OAuth access token.
    #[serde(default)]
    pub access_token: Option<String>,
    /// Access token expiry as unix seconds.
    #[serde(default)]
    pub access_expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SecretsFile {
    accounts: HashMap<Uuid, AccountSecrets>,
}

/// Process-wide store for mail credentials under `data/mail/`.
#[derive(Debug)]
pub struct MailSecretsStore {
    path: PathBuf,
    inner: RwLock<SecretsFile>,
}

impl MailSecretsStore {
    /// Load (or create empty) secrets from `mail_dir`.
    pub fn open(mail_dir: impl AsRef<Path>) -> Result<Arc<Self>> {
        let path = mail_dir.as_ref().join(SECRETS_FILE);
        let inner = if path.is_file() {
            match load_blob(&path) {
                Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
                Err(MailError::Crypto(_)) => SecretsFile::default(),
                Err(e) => return Err(e),
            }
        } else {
            SecretsFile::default()
        };
        Ok(Arc::new(Self {
            path,
            inner: RwLock::new(inner),
        }))
    }

    /// Persist password for `account_id`.
    pub fn set_password(&self, account_id: Uuid, password: SecretString) -> Result<()> {
        let mut guard = self.inner.write();
        let entry = guard.accounts.entry(account_id).or_default();
        entry.password = Some(password.expose_secret().to_string());
        drop(guard);
        self.flush()
    }

    /// Read password when present.
    pub fn password(&self, account_id: Uuid) -> Option<SecretString> {
        self.inner
            .read()
            .accounts
            .get(&account_id)
            .and_then(|s| s.password.as_ref())
            .map(|p| SecretString::from(p.clone()))
    }

    /// Store OAuth tokens.
    pub fn set_oauth(
        &self,
        account_id: Uuid,
        refresh: SecretString,
        access: Option<SecretString>,
        access_expires_at: Option<i64>,
    ) -> Result<()> {
        let mut guard = self.inner.write();
        let entry = guard.accounts.entry(account_id).or_default();
        entry.refresh_token = Some(refresh.expose_secret().to_string());
        entry.access_token = access.map(|t| t.expose_secret().to_string());
        entry.access_expires_at = access_expires_at;
        drop(guard);
        self.flush()
    }

    /// Clone secrets for an account.
    pub fn get(&self, account_id: Uuid) -> AccountSecrets {
        self.inner
            .read()
            .accounts
            .get(&account_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Drop secrets for a removed account.
    pub fn remove(&self, account_id: Uuid) -> Result<()> {
        self.inner.write().accounts.remove(&account_id);
        self.flush()
    }

    fn flush(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(&*self.inner.read())?;
        save_blob(&self.path, &json)
    }
}

fn save_blob(path: &Path, plaintext: &[u8]) -> Result<()> {
    match orchid_crypto::secret::dpapi::protect(plaintext, Some(DPAPI_DESC)) {
        Ok(protected) => {
            std::fs::write(path, protected)?;
            Ok(())
        }
        Err(orchid_crypto::CryptoError::DpapiUnavailable) => {
            // Non-Windows / unavailable: write opaque best-effort file for tests.
            std::fs::write(path, plaintext)?;
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

fn load_blob(path: &Path) -> Result<Vec<u8>> {
    let blob = std::fs::read(path)?;
    match orchid_crypto::secret::dpapi::unprotect(&blob) {
        Ok(plain) => Ok(plain.into_inner()),
        Err(orchid_crypto::CryptoError::DpapiUnavailable) => Ok(blob),
        Err(e) => {
            // File may be plaintext from a DPAPI-unavailable save.
            if blob.starts_with(b"{") {
                Ok(blob)
            } else {
                Err(e.into())
            }
        }
    }
}
