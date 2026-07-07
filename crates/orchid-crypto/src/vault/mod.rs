//! Password vault lifecycle: unlock, lock, and DPAPI-protected master key storage.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use secrecy::{ExposeSecret, SecretString};

use crate::biometric::{self, BiometricAvailability, BiometricVerification};
use crate::error::{CryptoError, Result};
use crate::kdbx::PasswordDatabase;
use crate::secret::dpapi;

const MASTER_KEY_FILE: &str = "passwords.master.dpapi";
const KDBX_FILE: &str = "passwords.kdbx";

/// Shared handle to the optional unlocked KDBX database.
#[derive(Debug)]
pub struct PasswordVault {
    db_path: PathBuf,
    key_path: PathBuf,
    database: RwLock<Option<Arc<PasswordDatabase>>>,
}

impl PasswordVault {
    /// Vault files live under `data_dir`.
    #[must_use]
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            db_path: data_dir.join(KDBX_FILE),
            key_path: data_dir.join(MASTER_KEY_FILE),
            database: RwLock::new(None),
        })
    }

    /// Path to the KDBX file.
    #[must_use]
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Whether the KDBX file exists on disk.
    #[must_use]
    pub fn db_exists(&self) -> bool {
        self.db_path.exists()
    }

    /// Whether a DPAPI-wrapped master key is stored for biometric unlock.
    #[must_use]
    pub fn has_stored_master(&self) -> bool {
        self.key_path.is_file()
    }

    /// True when the vault database is loaded in memory.
    #[must_use]
    pub fn is_unlocked(&self) -> bool {
        self.database.read().is_some()
    }

    /// Borrow the unlocked database, if any.
    #[must_use]
    pub fn database(&self) -> Option<Arc<PasswordDatabase>> {
        self.database.read().clone()
    }

    /// Whether Windows Hello can unlock using the stored master key.
    #[must_use]
    pub fn biometric_unlock_available(&self) -> bool {
        self.has_stored_master()
            && biometric::check_availability() == BiometricAvailability::Available
    }

    /// Unlock (or create) the vault with a master passphrase.
    ///
    /// On success the master is persisted via DPAPI for later Hello unlock.
    pub fn unlock_with_passphrase(&self, master: SecretString) -> Result<()> {
        let db = if self.db_path.exists() {
            PasswordDatabase::open(&self.db_path, master.clone())?
        } else {
            PasswordDatabase::create(&self.db_path, master.clone())?
        };
        let _ = save_master_key(&self.key_path, master.expose_secret().as_bytes());
        *self.database.write() = Some(Arc::new(db));
        Ok(())
    }

    /// Unlock using Windows Hello plus the stored DPAPI master key.
    pub fn unlock_with_biometric(&self, prompt: &str) -> Result<()> {
        if !self.has_stored_master() {
            return Err(CryptoError::MasterKeyNotStored);
        }
        match biometric::verify_user(prompt)? {
            BiometricVerification::Verified => {}
            BiometricVerification::Cancelled => return Err(CryptoError::BiometricCancelled),
            BiometricVerification::DeviceBusy => {
                return Err(CryptoError::BiometricFailed("device busy".into()));
            }
            BiometricVerification::Failed => {
                return Err(CryptoError::BiometricFailed("verification failed".into()));
            }
        }
        let master = load_master_key(&self.key_path)?;
        let db = PasswordDatabase::open(&self.db_path, master)?;
        *self.database.write() = Some(Arc::new(db));
        Ok(())
    }

    /// Drop the in-memory database handle.
    pub fn lock(&self) {
        *self.database.write() = None;
    }
}

fn save_master_key(path: &Path, master_utf8: &[u8]) -> Result<()> {
    match dpapi::protect(master_utf8, Some("Orchid password vault")) {
        Ok(protected) => std::fs::write(path, protected).map_err(Into::into),
        Err(CryptoError::DpapiUnavailable) => Ok(()),
        Err(e) => Err(e),
    }
}

fn load_master_key(path: &Path) -> Result<SecretString> {
    let blob = std::fs::read(path)?;
    let plain = dpapi::unprotect(&blob)?;
    let s = String::from_utf8(plain.into_inner()).map_err(|e| CryptoError::Dpapi(e.to_string()))?;
    Ok(SecretString::new(s))
}
