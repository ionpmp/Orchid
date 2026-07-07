//! DPAPI-protected passphrase for encrypted-folder operations (Windows Hello gate).

use std::path::PathBuf;
use std::sync::Arc;

use secrecy::{ExposeSecret, SecretString};

use crate::biometric::{self, BiometricAvailability};
use crate::error::{CryptoError, Result};

use super::{load_dpapi_blob, save_dpapi_blob, verify_biometric};

const FM_PASSPHRASE_FILE: &str = "fm.passphrase.dpapi";
const DPAPI_DESC: &str = "Orchid encrypted-folder passphrase";

/// Remembers the last successful FM encryption passphrase via DPAPI.
#[derive(Debug)]
pub struct FmPassphraseVault {
    key_path: PathBuf,
}

impl FmPassphraseVault {
    /// Store file lives under `data_dir`.
    #[must_use]
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            key_path: data_dir.join(FM_PASSPHRASE_FILE),
        })
    }

    /// Whether a DPAPI-wrapped passphrase is stored.
    #[must_use]
    pub fn has_stored_passphrase(&self) -> bool {
        self.key_path.is_file()
    }

    /// Whether Windows Hello can fill the passphrase from the stored blob.
    #[must_use]
    pub fn biometric_unlock_available(&self) -> bool {
        self.has_stored_passphrase()
            && biometric::check_availability() == BiometricAvailability::Available
    }

    /// Persist the passphrase for later Hello unlock (best-effort on non-Windows).
    pub fn save_passphrase(&self, passphrase: SecretString) -> Result<()> {
        save_dpapi_blob(
            &self.key_path,
            passphrase.expose_secret().as_bytes(),
            DPAPI_DESC,
        )
    }

    /// Verify the user via Hello and return the stored passphrase.
    pub fn load_passphrase_after_biometric(&self, prompt: &str) -> Result<SecretString> {
        if !self.has_stored_passphrase() {
            return Err(CryptoError::MasterKeyNotStored);
        }
        verify_biometric(prompt)?;
        load_dpapi_blob(&self.key_path)
    }
}
