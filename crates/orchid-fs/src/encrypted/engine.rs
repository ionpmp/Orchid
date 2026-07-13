//! Encrypted-folder engine: marks paths as encrypted, performs in-place
//! encryption, and drives reveal sessions through [`orchid_crypto`].

use std::sync::Arc;

use orchid_crypto::{
    Decryptor, Encryptor, Identity, IdentityKind, RevealDuration, RevealManager, RevealSession,
};
use redb::ReadableTable;
use tracing::warn;

use crate::encrypted::index::{EncryptedFolderRecord, ENCRYPTED_PATHS};
use crate::encrypted::marker::{looks_encrypted, looks_encrypted_directory, AGE_EXT};
use crate::error::{FsError, Result};
use crate::path::FsPath;
use crate::provider::FsProviderRegistry;
use crate::watcher::FileWatcher;

/// Public-facing encrypted-folder declaration.
#[derive(Debug, Clone)]
pub struct EncryptedFolderConfig {
    /// Encrypted path (file or folder).
    pub path: FsPath,
    /// Identity to use when opening this path. Never persisted.
    pub identity: Identity,
    /// Reveal-window policy.
    pub reveal_duration: RevealDuration,
    /// Whether the declaration is currently active.
    pub enabled: bool,
}

/// Maximum size for best-effort overwrite-then-delete of plaintext originals.
pub const WIPE_MAX_BYTES_DEFAULT: u64 = 256 * 1024 * 1024;

/// Encrypted-folder engine.
pub struct EncryptedFolderEngine {
    storage: Arc<orchid_storage::StateStore>,
    #[allow(dead_code)]
    registry: Arc<FsProviderRegistry>,
    reveal_manager: Arc<RevealManager>,
    bus: Arc<orchid_core::EventBus>,
    #[allow(dead_code)]
    watcher: Arc<FileWatcher>,
    wipe_max_bytes: u64,
}

impl std::fmt::Debug for EncryptedFolderEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedFolderEngine").finish_non_exhaustive()
    }
}

impl EncryptedFolderEngine {
    /// Construct an engine.
    #[must_use]
    pub fn new(
        storage: Arc<orchid_storage::StateStore>,
        registry: Arc<FsProviderRegistry>,
        reveal_manager: Arc<RevealManager>,
        bus: Arc<orchid_core::EventBus>,
        watcher: Arc<FileWatcher>,
    ) -> Self {
        Self {
            storage,
            registry,
            reveal_manager,
            bus,
            watcher,
            wipe_max_bytes: WIPE_MAX_BYTES_DEFAULT,
        }
    }

    /// Register a path as encrypted. The identity is NOT persisted.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub async fn mark_encrypted(&self, cfg: EncryptedFolderConfig) -> Result<()> {
        let record = EncryptedFolderRecord {
            path: cfg.path.clone(),
            identity_kind: cfg.identity.kind(),
            reveal_duration: cfg.reveal_duration.into(),
            enabled: cfg.enabled,
        };
        let db = self.storage.raw_database();
        let txn = db
            .begin_write()
            .map_err(|e| FsError::Storage(e.into()))?;
        {
            let mut table = txn
                .open_table(ENCRYPTED_PATHS)
                .map_err(|e| FsError::Storage(e.into()))?;
            table
                .insert(cfg.path.as_str(), &record)
                .map_err(|e| FsError::Storage(e.into()))?;
        }
        txn.commit().map_err(|e| FsError::Storage(e.into()))?;

        self.bus.publish(
            orchid_core::EventSource::Subsystem("fs.encrypted".into()),
            EncryptedPathRegistered {
                path: cfg.path,
                identity_kind: cfg.identity.kind(),
            },
        );
        Ok(())
    }

    /// Remove an encrypted-path record.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::NotEncryptedPath`] if the path is unknown.
    pub async fn unmark(&self, path: &FsPath) -> Result<()> {
        let db = self.storage.raw_database();
        let txn = db
            .begin_write()
            .map_err(|e| FsError::Storage(e.into()))?;
        let existed = {
            let mut table = txn
                .open_table(ENCRYPTED_PATHS)
                .map_err(|e| FsError::Storage(e.into()))?;
            let removed = table
                .remove(path.as_str())
                .map_err(|e| FsError::Storage(e.into()))?;
            removed.is_some()
        };
        txn.commit().map_err(|e| FsError::Storage(e.into()))?;
        if !existed {
            return Err(FsError::NotEncryptedPath(path.to_string()));
        }
        Ok(())
    }

    /// Encrypt a file in place: replaces `path` with `path.age` plus the
    /// `.age.meta` sidecar, then wipes / removes the plaintext.
    ///
    /// # Errors
    ///
    /// Propagates crypto and I/O errors.
    pub async fn encrypt_in_place(
        &self,
        path: &FsPath,
        identity: Identity,
    ) -> Result<()> {
        let src = path.to_local()?;
        let target_str = format!("{}.{AGE_EXT}", path.as_str());
        let target = FsPath::new(target_str)?;
        let encrypted_os = target.to_local()?;

        // Size check for wipe policy.
        let size = tokio::fs::metadata(&src).await?.len();

        let encryptor = Encryptor::new(identity.clone());
        encryptor.encrypt_file(&src, &encrypted_os).await?;

        if size <= self.wipe_max_bytes {
            if let Err(e) = overwrite_then_delete(&src).await {
                warn!(error = %e, "overwrite-before-delete failed; falling back to plain remove");
                let _ = tokio::fs::remove_file(&src).await;
            }
        } else {
            let _ = tokio::fs::remove_file(&src).await;
        }

        // Register the new `.age` path as encrypted so consumers can find it.
        self.mark_encrypted(EncryptedFolderConfig {
            path: target.clone(),
            identity,
            reveal_duration: RevealDuration::FiveMinutes,
            enabled: true,
        })
        .await?;
        Ok(())
    }

    /// Encrypt a directory tree in place: tar+age the contents into marker
    /// files inside the folder, then remove the plaintext tree.
    ///
    /// # Errors
    ///
    /// Propagates crypto and I/O errors.
    pub async fn encrypt_directory_in_place(
        &self,
        path: &FsPath,
        identity: Identity,
    ) -> Result<()> {
        let src = path.to_local()?;
        let meta = tokio::fs::metadata(&src).await?;
        if !meta.is_dir() {
            return Err(FsError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "encrypt_directory_in_place requires a directory",
            )));
        }

        let parent = src
            .parent()
            .ok_or_else(|| FsError::Io(std::io::Error::other("directory has no parent")))?;
        let base = src
            .file_name()
            .ok_or_else(|| FsError::Io(std::io::Error::other("directory has no name")))?;
        let temp = parent.join(format!("{}.orchid-encrypting", base.to_string_lossy()));

        if temp.exists() {
            if temp.is_dir() {
                tokio::fs::remove_dir_all(&temp).await?;
            } else {
                tokio::fs::remove_file(&temp).await?;
            }
        }

        let encryptor = Encryptor::new(identity.clone());
        encryptor.encrypt_directory(&src, &temp).await.map_err(|e| FsError::EncryptedOp(e.to_string()))?;

        tokio::fs::remove_dir_all(&src).await?;
        tokio::fs::rename(&temp, &src).await?;

        self.mark_encrypted(EncryptedFolderConfig {
            path: path.clone(),
            identity,
            reveal_duration: RevealDuration::FiveMinutes,
            enabled: true,
        })
        .await?;
        Ok(())
    }

    /// Open `path` in a reveal session.
    ///
    /// # Errors
    ///
    /// Propagates crypto and I/O errors.
    pub async fn reveal(
        &self,
        path: &FsPath,
        identity: Identity,
    ) -> Result<RevealSession> {
        let record = self.lookup(path).await?;
        let duration: RevealDuration = record.reveal_duration.into();
        if record.identity_kind != identity.kind() {
            return Err(FsError::EncryptedOp(format!(
                "identity kind mismatch: record={:?}, supplied={:?}",
                record.identity_kind,
                identity.kind()
            )));
        }
        let decryptor = Decryptor::new(identity);
        let encrypted_os = path.to_local()?;
        let session = if looks_encrypted_directory(path) {
            self.reveal_manager
                .reveal_directory(&decryptor, &encrypted_os, duration)
                .await
                .map_err(|e| FsError::EncryptedOp(e.to_string()))?
        } else {
            self.reveal_manager
                .reveal(&decryptor, &encrypted_os, duration)
                .await
                .map_err(|e| FsError::EncryptedOp(e.to_string()))?
        };
        Ok(session)
    }

    /// Decrypt a file in place: writes plaintext to the original path, removes
    /// the `.age` payload and sidecar, and unmarks the encrypted record.
    ///
    /// # Errors
    ///
    /// Propagates crypto, I/O, and storage errors.
    pub async fn decrypt_in_place(&self, path: &FsPath, identity: Identity) -> Result<()> {
        if looks_encrypted_directory(path) {
            return self.decrypt_directory_in_place(path, identity).await;
        }
        if !looks_encrypted(path) {
            return Err(FsError::NotEncryptedPath(path.to_string()));
        }
        let encrypted_os = path.to_local()?;
        let plaintext_fs = plaintext_path_for_encrypted(path)?;
        let plaintext_os = plaintext_fs.to_local()?;

        let decryptor = orchid_crypto::Decryptor::new(identity);
        decryptor
            .decrypt_file(&encrypted_os, &plaintext_os)
            .await
            .map_err(|e| FsError::EncryptedOp(e.to_string()))?;

        if let Err(e) = tokio::fs::remove_file(&encrypted_os).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(FsError::Io(e));
            }
        }
        let meta_path = encrypted_os.with_extension("age.meta");
        if let Err(e) = tokio::fs::remove_file(&meta_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(FsError::Io(e));
            }
        }

        self.unmark(path).await?;
        Ok(())
    }

    async fn decrypt_directory_in_place(&self, path: &FsPath, identity: Identity) -> Result<()> {
        let src = path.to_local()?;
        let parent = src
            .parent()
            .ok_or_else(|| FsError::Io(std::io::Error::other("directory has no parent")))?;
        let base = src
            .file_name()
            .ok_or_else(|| FsError::Io(std::io::Error::other("directory has no name")))?;
        let temp = parent.join(format!("{}.orchid-decrypting", base.to_string_lossy()));

        if temp.exists() {
            if temp.is_dir() {
                tokio::fs::remove_dir_all(&temp).await?;
            } else {
                tokio::fs::remove_file(&temp).await?;
            }
        }

        let decryptor = Decryptor::new(identity);
        decryptor
            .decrypt_directory(&src, &temp)
            .await
            .map_err(|e| FsError::EncryptedOp(e.to_string()))?;

        tokio::fs::remove_dir_all(&src).await?;
        tokio::fs::rename(&temp, &src).await?;

        self.unmark(path).await?;
        Ok(())
    }

    /// Look up whether a path is registered as encrypted.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub async fn is_encrypted(&self, path: &FsPath) -> Result<bool> {
        Ok(self.lookup(path).await.is_ok())
    }

    /// List all encrypted records (identity kind only — no secret material).
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub async fn list_encrypted(&self) -> Result<Vec<EncryptedFolderRecord>> {
        let db = self.storage.raw_database();
        let txn = db
            .begin_read()
            .map_err(|e| FsError::Storage(e.into()))?;
        let table = match txn.open_table(ENCRYPTED_PATHS) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(FsError::Storage(e.into())),
        };
        let mut out = Vec::new();
        for item in table.iter().map_err(|e| FsError::Storage(e.into()))? {
            let (_, v) = item.map_err(|e| FsError::Storage(e.into()))?;
            out.push(v.value());
        }
        Ok(out)
    }

    /// Start the engine (currently no background task; placeholder for
    /// future FS-event hooks).
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation.
    pub async fn start(&self) -> Result<()> {
        Ok(())
    }

    /// Shut the engine down.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation.
    pub async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn lookup(&self, path: &FsPath) -> Result<EncryptedFolderRecord> {
        let db = self.storage.raw_database();
        let txn = db
            .begin_read()
            .map_err(|e| FsError::Storage(e.into()))?;
        let table = match txn.open_table(ENCRYPTED_PATHS) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => {
                return Err(FsError::NotEncryptedPath(path.to_string()));
            }
            Err(e) => return Err(FsError::Storage(e.into())),
        };
        let got = table
            .get(path.as_str())
            .map_err(|e| FsError::Storage(e.into()))?;
        let value = got.map(|g| g.value());
        value.ok_or_else(|| FsError::NotEncryptedPath(path.to_string()))
    }
}

/// Emitted when a path is marked encrypted.
#[derive(Debug, Clone)]
pub struct EncryptedPathRegistered {
    /// Path that was marked.
    pub path: FsPath,
    /// Identity kind stored in the record.
    pub identity_kind: IdentityKind,
}
impl orchid_core::Event for EncryptedPathRegistered {
    fn event_type() -> &'static str {
        "fs.encrypted_registered"
    }
}

fn plaintext_path_for_encrypted(path: &FsPath) -> Result<FsPath> {
    let s = path.as_str();
    let plain = s
        .strip_suffix(".age")
        .or_else(|| {
            if s.len() >= 4 && s[s.len() - 4..].eq_ignore_ascii_case(".age") {
                Some(&s[..s.len() - 4])
            } else {
                None
            }
        })
        .ok_or_else(|| FsError::NotEncryptedPath(path.to_string()))?;
    FsPath::new(plain)
}

async fn overwrite_then_delete(path: &std::path::Path) -> Result<()> {
    // Best-effort logical overwrite. Not a physical secure erase — see
    // docs/SECURITY.md ("Disk wipe after encryption / reveal").
    use tokio::io::AsyncWriteExt;
    const BLOCK: usize = 64 * 1024;
    let meta = tokio::fs::metadata(path).await?;
    let len = meta.len();
    {
        let mut f = tokio::fs::OpenOptions::new().write(true).open(path).await?;
        let zeros = vec![0u8; BLOCK.min(len as usize).max(1)];
        let mut remaining = len;
        while remaining > 0 {
            let take = remaining.min(zeros.len() as u64) as usize;
            f.write_all(&zeros[..take]).await?;
            remaining -= take as u64;
        }
        f.flush().await?;
        f.sync_all().await?;
    }
    tokio::fs::remove_file(path).await?;
    Ok(())
}
