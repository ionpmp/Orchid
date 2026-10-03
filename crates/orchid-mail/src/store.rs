//! Account list persisted as JSON under `data/mail/accounts.json`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use uuid::Uuid;

use crate::account::MailAccount;
use crate::error::{MailError, Result};

const ACCOUNTS_FILE: &str = "accounts.json";

/// Non-secret account registry.
#[derive(Debug)]
pub struct AccountStore {
    path: PathBuf,
    accounts: RwLock<Vec<MailAccount>>,
}

impl AccountStore {
    /// Open or create the account list under `mail_dir`.
    pub fn open(mail_dir: impl AsRef<Path>) -> Result<Arc<Self>> {
        let path = mail_dir.as_ref().join(ACCOUNTS_FILE);
        let accounts = if path.is_file() {
            let bytes = std::fs::read(&path)?;
            serde_json::from_slice(&bytes).unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(Arc::new(Self {
            path,
            accounts: RwLock::new(accounts),
        }))
    }

    /// Snapshot of all accounts.
    pub fn list(&self) -> Vec<MailAccount> {
        self.accounts.read().clone()
    }

    /// Find by id.
    pub fn get(&self, id: Uuid) -> Option<MailAccount> {
        self.accounts.read().iter().find(|a| a.id == id).cloned()
    }

    /// Insert or replace an account and flush.
    pub fn upsert(&self, account: MailAccount) -> Result<()> {
        let mut guard = self.accounts.write();
        if let Some(slot) = guard.iter_mut().find(|a| a.id == account.id) {
            *slot = account;
        } else {
            guard.push(account);
        }
        drop(guard);
        self.flush()
    }

    /// Remove an account by id.
    pub fn remove(&self, id: Uuid) -> Result<()> {
        self.accounts.write().retain(|a| a.id != id);
        self.flush()
    }

    fn flush(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(&*self.accounts.read())?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }
}

/// Ensure `mail_dir` exists.
pub fn ensure_mail_dir(mail_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(mail_dir).map_err(MailError::from)
}
