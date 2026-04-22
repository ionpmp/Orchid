//! Session registry + persistence helpers.

use std::sync::Arc;

use dashmap::DashMap;
use uuid::Uuid;

use crate::backend::BackendSpec;
use crate::error::{Result, TerminalError};
use crate::pty::PtySize;
use crate::session::persistence::{backend_kind_from_storage, backend_kind_to_storage};
use crate::session::TerminalSession;

/// Registry of every live terminal session.
pub struct SessionManager {
    sessions: DashMap<Uuid, Arc<TerminalSession>>,
    bus: Arc<orchid_core::EventBus>,
    storage: Arc<orchid_storage::StateStore>,
}

impl std::fmt::Debug for SessionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionManager")
            .field("session_count", &self.sessions.len())
            .finish_non_exhaustive()
    }
}

impl SessionManager {
    /// Construct a manager bound to a bus and a state store.
    #[must_use]
    pub fn new(
        bus: Arc<orchid_core::EventBus>,
        storage: Arc<orchid_storage::StateStore>,
    ) -> Self {
        Self {
            sessions: DashMap::new(),
            bus,
            storage,
        }
    }

    /// Open a new session.
    ///
    /// # Errors
    ///
    /// Propagates [`TerminalError::SpawnFailed`] and related errors.
    pub async fn open(&self, spec: BackendSpec, size: PtySize) -> Result<Uuid> {
        let session = TerminalSession::open(spec, size, Arc::clone(&self.bus)).await?;
        let id = session.id;
        self.sessions.insert(id, session);
        Ok(id)
    }

    /// Look up a session by id.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::SessionNotFound`] on miss.
    pub fn get(&self, id: Uuid) -> Result<Arc<TerminalSession>> {
        self.sessions
            .get(&id)
            .map(|e| Arc::clone(e.value()))
            .ok_or(TerminalError::SessionNotFound(id))
    }

    /// Every session id currently registered.
    #[must_use]
    pub fn list(&self) -> Vec<Uuid> {
        self.sessions.iter().map(|e| *e.key()).collect()
    }

    /// Close a specific session.
    ///
    /// # Errors
    ///
    /// Propagates session-close errors.
    pub async fn close(&self, id: Uuid) -> Result<()> {
        let Some((_, session)) = self.sessions.remove(&id) else {
            return Err(TerminalError::SessionNotFound(id));
        };
        session.close(Arc::clone(&self.bus)).await?;
        Ok(())
    }

    /// Close every open session (best-effort).
    ///
    /// # Errors
    ///
    /// Returns the first close error encountered; the rest are logged.
    pub async fn close_all(&self) -> Result<()> {
        let ids: Vec<Uuid> = self.sessions.iter().map(|e| *e.key()).collect();
        let mut first_err: Option<TerminalError> = None;
        for id in ids {
            if let Err(e) = self.close(id).await {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
        first_err.map_or(Ok(()), Err)
    }

    /// Persist a snapshot of active sessions into storage. The payload stored
    /// is minimal — backend kind + window title + (optional) OSC-7 cwd — so
    /// a reopen starts a fresh shell of the same kind.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub fn snapshot_to_storage(&self) -> Result<()> {
        let mut tabs: Vec<orchid_storage::TerminalSession> = Vec::new();
        for entry in self.sessions.iter() {
            let session = entry.value();
            let backend = backend_kind_to_storage(&session.spec.kind);
            let cwd = session
                .emulator
                .working_directory()
                .and_then(|p| p.to_str().map(str::to_owned));
            let title = session.emulator.title();
            tabs.push(orchid_storage::TerminalSession {
                id: session.id,
                backend,
                working_directory: cwd,
                title,
            });
        }

        let mut w = self.storage.write()?;
        w.set_session_state(&orchid_storage::SessionState {
            active_workspace_id: None,
            open_file_manager_tabs: Vec::new(),
            open_terminal_sessions: tabs,
            last_saved_at: chrono::Utc::now(),
        })?;
        w.commit()?;
        Ok(())
    }

    /// Read the persisted session list from storage. Does not reopen them —
    /// the caller picks which to revive.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub fn load_previous(&self) -> Result<Vec<orchid_storage::TerminalSession>> {
        let r = self.storage.read()?;
        let Some(state) = r.get_session_state()? else {
            return Ok(Vec::new());
        };
        Ok(state.open_terminal_sessions)
    }

    /// Convert a persisted terminal-session record back into a [`BackendSpec`].
    ///
    /// # Errors
    ///
    /// Propagates [`TerminalError::BackendUnavailable`] for malformed custom
    /// entries.
    pub fn spec_from_persisted(
        record: &orchid_storage::TerminalSession,
    ) -> Result<BackendSpec> {
        let kind = backend_kind_from_storage(&record.backend)?;
        // The persisted `title` field is advisory only; it isn't used to
        // reconstruct the spec (re-spawned shells produce their own title).
        let _ = &record.title;
        Ok(BackendSpec {
            kind,
            working_directory: record
                .working_directory
                .as_ref()
                .map(std::path::PathBuf::from),
            env: std::collections::BTreeMap::new(),
            initial_command: None,
        })
    }
}
