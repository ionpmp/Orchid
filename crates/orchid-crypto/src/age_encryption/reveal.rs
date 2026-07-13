//! Time-bounded plaintext reveal sessions.
//!
//! A reveal session decrypts an age-encrypted file or directory to a
//! temporary, per-session directory and schedules automatic wipe + removal
//! after the configured duration. Callers can also close sessions manually.
//!
//! # Threat model
//!
//! Reveal sessions narrow the window during which the plaintext is present
//! on disk, but they do not protect against an attacker with simultaneous
//! read access to the user's profile (malware running as the same user can
//! read the revealed file during the reveal window). They are a usability
//! feature on top of the baseline encryption, not a replacement for
//! sensitive access policies.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::age_encryption::decryptor::Decryptor;
use crate::age_encryption::metadata::EncryptedFileMeta;
use crate::content::store::{Clock, SystemClock};
use crate::error::{CryptoError, Result};

/// How long a reveal session stays open.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize,
)]
pub enum RevealDuration {
    /// Expire after 5 minutes.
    FiveMinutes,
    /// Expire after 30 minutes.
    ThirtyMinutes,
    /// Expire after 1 hour.
    OneHour,
    /// Never auto-expire; must be closed explicitly.
    UntilClosed,
}

impl RevealDuration {
    /// Map to a `Duration`, or `None` for [`RevealDuration::UntilClosed`].
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_crypto::RevealDuration;
    /// assert!(RevealDuration::FiveMinutes.to_duration().is_some());
    /// assert!(RevealDuration::UntilClosed.to_duration().is_none());
    /// ```
    #[must_use]
    pub fn to_duration(self) -> Option<Duration> {
        match self {
            Self::FiveMinutes => Some(Duration::from_secs(5 * 60)),
            Self::ThirtyMinutes => Some(Duration::from_secs(30 * 60)),
            Self::OneHour => Some(Duration::from_secs(60 * 60)),
            Self::UntilClosed => None,
        }
    }
}

/// State of one reveal session.
#[derive(Debug, Clone)]
pub struct RevealSession {
    /// Unique id.
    pub id: Uuid,
    /// Original encrypted payload (file or directory).
    pub encrypted_path: PathBuf,
    /// Revealed plaintext location (file or directory).
    pub revealed_path: PathBuf,
    /// Metadata recovered during decryption.
    pub meta: EncryptedFileMeta,
    /// Configured lifetime.
    pub duration: RevealDuration,
    /// When the session expires, or `None` for [`RevealDuration::UntilClosed`].
    pub expires_at: Option<DateTime<Utc>>,
    /// When the reveal happened.
    pub revealed_at: DateTime<Utc>,
    /// Whether the session applies to a directory.
    pub is_directory: bool,
    /// Session-specific temp root (contains `revealed_path`).
    pub session_dir: PathBuf,
}

// --- Events --------------------------------------------------------------

/// Emitted when a reveal session is established.
#[derive(Debug, Clone)]
pub struct RevealStarted {
    /// Session id.
    pub session_id: Uuid,
}
impl orchid_core::Event for RevealStarted {
    fn event_type() -> &'static str {
        "crypto.reveal_started"
    }
}

/// Emitted when a reveal session's timer runs out.
#[derive(Debug, Clone)]
pub struct RevealExpired {
    /// Session id.
    pub session_id: Uuid,
}
impl orchid_core::Event for RevealExpired {
    fn event_type() -> &'static str {
        "crypto.reveal_expired"
    }
}

/// Emitted when a reveal session is closed (explicitly or on expiry).
#[derive(Debug, Clone)]
pub struct RevealClosed {
    /// Session id.
    pub session_id: Uuid,
}
impl orchid_core::Event for RevealClosed {
    fn event_type() -> &'static str {
        "crypto.reveal_closed"
    }
}

// --- Manager -------------------------------------------------------------

struct RevealManagerInner {
    temp_root: PathBuf,
    bus: Arc<orchid_core::EventBus>,
    clock: Arc<dyn Clock>,
    sessions: Mutex<HashMap<Uuid, RevealSession>>,
    sweeper: Mutex<Option<JoinHandle<()>>>,
    shutdown: AtomicBool,
}

/// Central coordinator for reveal sessions.
#[derive(Clone)]
pub struct RevealManager {
    inner: Arc<RevealManagerInner>,
}

impl std::fmt::Debug for RevealManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RevealManager")
            .field("temp_root", &self.inner.temp_root)
            .field("active_sessions", &self.inner.sessions.lock().len())
            .finish()
    }
}

impl RevealManager {
    /// Build a manager rooted under `temp_root`. Decrypted files will be
    /// placed inside per-session subdirectories of this directory.
    #[must_use]
    pub fn new(temp_root: PathBuf, bus: Arc<orchid_core::EventBus>) -> Self {
        Self::with_clock(temp_root, bus, Arc::new(SystemClock))
    }

    /// Builder variant that lets tests inject a clock.
    #[must_use]
    pub fn with_clock(
        temp_root: PathBuf,
        bus: Arc<orchid_core::EventBus>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            inner: Arc::new(RevealManagerInner {
                temp_root,
                bus,
                clock,
                sessions: Mutex::new(HashMap::new()),
                sweeper: Mutex::new(None),
                shutdown: AtomicBool::new(false),
            }),
        }
    }

    /// Reveal an encrypted file.
    ///
    /// # Errors
    ///
    /// Propagates decryption and I/O errors.
    pub async fn reveal(
        &self,
        decryptor: &Decryptor,
        encrypted_path: &Path,
        duration: RevealDuration,
    ) -> Result<RevealSession> {
        let session_id = crate::random::random_uuid();
        let session_dir = self.inner.temp_root.join(session_id.to_string());
        tokio::fs::create_dir_all(&session_dir).await?;

        let meta = decryptor.read_metadata(encrypted_path).await.or_else(|_| {
            // Metadata sidecar may be missing for ad-hoc reveals; fall
            // back to a stub so the session is still usable.
            Ok::<_, CryptoError>(EncryptedFileMeta {
                version: crate::age_encryption::metadata::METADATA_VERSION,
                id: crate::random::random_uuid(),
                original_name: encrypted_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("revealed")
                    .to_string(),
                original_size: 0,
                encrypted_at: self.inner.clock.now(),
                identity_kind: crate::age_encryption::metadata::IdentityKind::Passphrase,
                blake3_of_plaintext: [0u8; 32],
                content_type_hint: None,
            })
        })?;

        let revealed_path = session_dir.join(&meta.original_name);
        decryptor.decrypt_file(encrypted_path, &revealed_path).await?;

        let now = self.inner.clock.now();
        let expires_at = duration
            .to_duration()
            .map(|d| now + chrono::Duration::from_std(d).unwrap_or(chrono::Duration::zero()));
        let session = RevealSession {
            id: session_id,
            encrypted_path: encrypted_path.to_path_buf(),
            revealed_path,
            meta,
            duration,
            expires_at,
            revealed_at: now,
            is_directory: false,
            session_dir,
        };
        self.register(session.clone());
        Ok(session)
    }

    /// Reveal an encrypted directory.
    ///
    /// # Errors
    ///
    /// Propagates decryption and I/O errors.
    pub async fn reveal_directory(
        &self,
        decryptor: &Decryptor,
        encrypted_dir: &Path,
        duration: RevealDuration,
    ) -> Result<RevealSession> {
        let session_id = crate::random::random_uuid();
        let session_dir = self.inner.temp_root.join(session_id.to_string());
        let revealed_path = session_dir.join("content");
        tokio::fs::create_dir_all(&revealed_path).await?;

        let meta = decryptor
            .decrypt_directory(encrypted_dir, &revealed_path)
            .await?;

        let now = self.inner.clock.now();
        let expires_at = duration
            .to_duration()
            .map(|d| now + chrono::Duration::from_std(d).unwrap_or(chrono::Duration::zero()));
        let session = RevealSession {
            id: session_id,
            encrypted_path: encrypted_dir.to_path_buf(),
            revealed_path,
            meta,
            duration,
            expires_at,
            revealed_at: now,
            is_directory: true,
            session_dir,
        };
        self.register(session.clone());
        Ok(session)
    }

    fn register(&self, session: RevealSession) {
        self.inner.sessions.lock().insert(session.id, session.clone());
        self.inner
            .bus
            .publish(orchid_core::EventSource::Subsystem("crypto".into()), RevealStarted {
                session_id: session.id,
            });
    }

    /// Snapshot of every active session.
    #[must_use]
    pub fn list_active(&self) -> Vec<RevealSession> {
        self.inner.sessions.lock().values().cloned().collect()
    }

    /// Close a specific session, wiping and removing its revealed payload.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::RevealNotFound`] if the id is unknown.
    pub async fn close(&self, session_id: Uuid) -> Result<()> {
        let session = {
            let mut guard = self.inner.sessions.lock();
            guard.remove(&session_id)
        };
        let Some(session) = session else {
            return Err(CryptoError::RevealNotFound(session_id));
        };
        wipe_session(&session).await;
        self.inner
            .bus
            .publish(orchid_core::EventSource::Subsystem("crypto".into()), RevealClosed {
                session_id,
            });
        Ok(())
    }

    /// Start the background sweeper that auto-closes expired sessions.
    ///
    /// If already running, this is a no-op.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation.
    pub async fn start_sweeper(&self) -> Result<()> {
        let mut guard = self.inner.sweeper.lock();
        if guard.is_some() {
            return Ok(());
        }
        let manager = self.clone();
        let handle = tokio::spawn(async move {
            loop {
                if manager.inner.shutdown.load(Ordering::SeqCst) {
                    break;
                }
                manager.sweep_once().await;
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        *guard = Some(handle);
        Ok(())
    }

    /// Run a single sweep pass. Useful for tests that want deterministic
    /// timing without driving the real sleep loop.
    pub async fn sweep_once(&self) {
        let now = self.inner.clock.now();
        let expired: Vec<Uuid> = {
            let guard = self.inner.sessions.lock();
            guard
                .values()
                .filter(|s| s.expires_at.map(|t| t <= now).unwrap_or(false))
                .map(|s| s.id)
                .collect()
        };
        for id in expired {
            let session = self.inner.sessions.lock().remove(&id);
            let Some(session) = session else {
                continue;
            };
            wipe_session(&session).await;
            self.inner
                .bus
                .publish(orchid_core::EventSource::Subsystem("crypto".into()), RevealExpired {
                    session_id: id,
                });
            self.inner
                .bus
                .publish(orchid_core::EventSource::Subsystem("crypto".into()), RevealClosed {
                    session_id: id,
                });
        }
    }

    /// Shut down the manager: stop the sweeper and close every active session.
    ///
    /// # Errors
    ///
    /// Surfaces any error raised while closing individual sessions (the
    /// first is returned; the rest are logged as warnings).
    pub async fn shutdown(&self) -> Result<()> {
        self.inner.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.inner.sweeper.lock().take() {
            handle.abort();
        }
        let ids: Vec<Uuid> = self
            .inner
            .sessions
            .lock()
            .keys()
            .copied()
            .collect();
        let mut first_err: Option<CryptoError> = None;
        for id in ids {
            if let Err(e) = self.close(id).await {
                if first_err.is_none() {
                    first_err = Some(e);
                } else {
                    warn!("additional shutdown error: {id}");
                }
            }
        }
        first_err.map_or(Ok(()), Err)
    }
}

async fn wipe_session(session: &RevealSession) {
    if session.is_directory {
        // Walk the directory and wipe each file before removing.
        if let Ok(mut rd) = tokio::fs::read_dir(&session.revealed_path).await {
            while let Ok(Some(entry)) = rd.next_entry().await {
                if entry.file_type().await.map(|t| t.is_file()).unwrap_or(false) {
                    wipe_file(entry.path().as_path()).await;
                }
            }
        }
        let _ = tokio::fs::remove_dir_all(&session.session_dir).await;
    } else {
        wipe_file(&session.revealed_path).await;
        let _ = tokio::fs::remove_dir_all(&session.session_dir).await;
    }
    debug!(session_id = %session.id, "reveal session wiped");
}

async fn wipe_file(path: &Path) {
    // Best-effort logical overwrite. Not a physical secure erase — see
    // docs/SECURITY.md ("Disk wipe after encryption / reveal").
    const BLOCK: usize = 64 * 1024;
    let Ok(meta) = tokio::fs::metadata(path).await else {
        return;
    };
    if !meta.is_file() {
        return;
    }
    let len = meta.len();
    if len == 0 {
        let _ = tokio::fs::remove_file(path).await;
        return;
    }
    if let Ok(mut f) = tokio::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .await
    {
        let zeros = vec![0u8; BLOCK.min(len as usize).max(1)];
        let mut remaining = len;
        while remaining > 0 {
            let take = remaining.min(zeros.len() as u64) as usize;
            if f.write_all(&zeros[..take]).await.is_err() {
                break;
            }
            remaining -= take as u64;
        }
        let _ = f.flush().await;
        let _ = f.sync_all().await;
    }
    let _ = tokio::fs::remove_file(path).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_duration_maps_to_expected_durations() {
        assert_eq!(
            RevealDuration::FiveMinutes.to_duration(),
            Some(Duration::from_secs(300))
        );
        assert_eq!(
            RevealDuration::ThirtyMinutes.to_duration(),
            Some(Duration::from_secs(1800))
        );
        assert_eq!(
            RevealDuration::OneHour.to_duration(),
            Some(Duration::from_secs(3600))
        );
        assert!(RevealDuration::UntilClosed.to_duration().is_none());
    }
}
