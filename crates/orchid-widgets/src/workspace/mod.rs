//! Virtual-desktop management.

pub mod operations;
pub mod persistence;

use std::sync::Arc;

use chrono::Utc;
use orchid_storage::{StateStore, Workspace};
use parking_lot::RwLock;
use tracing::debug;
use uuid::Uuid;

use crate::error::{Result, WidgetError};
use crate::events::{
    WorkspaceCreated, WorkspaceDeleted, WorkspaceRenamed, WorkspaceSwitched,
};

/// Maximum number of simultaneous workspaces.
pub const MAX_WORKSPACES: usize = 9;

struct WorkspaceManagerInner {
    bus: Arc<orchid_core::EventBus>,
    storage: Arc<StateStore>,
    workspaces: RwLock<Vec<Workspace>>,
    active_id: RwLock<Option<Uuid>>,
}

/// Owns every workspace and mediates workspace-level state changes.
pub struct WorkspaceManager {
    inner: Arc<WorkspaceManagerInner>,
}

impl std::fmt::Debug for WorkspaceManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceManager")
            .field("workspaces", &self.inner.workspaces.read().len())
            .finish_non_exhaustive()
    }
}

impl WorkspaceManager {
    /// Build a fresh manager. Call [`WorkspaceManager::restore_from_storage`]
    /// afterwards to load prior state.
    #[must_use]
    pub fn new(bus: Arc<orchid_core::EventBus>, storage: Arc<StateStore>) -> Self {
        Self {
            inner: Arc::new(WorkspaceManagerInner {
                bus,
                storage,
                workspaces: RwLock::new(Vec::new()),
                active_id: RwLock::new(None),
            }),
        }
    }

    /// Hydrate from storage. Picks the lowest-ordinal workspace as active by
    /// default.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub async fn restore_from_storage(&self) -> Result<()> {
        let mut list = persistence::load_all(&self.inner.storage)?;
        operations::redensify(&mut list);
        let active = list.first().map(|w| w.id);
        *self.inner.workspaces.write() = list;
        *self.inner.active_id.write() = active;
        Ok(())
    }

    /// Snapshot every currently-known workspace.
    #[must_use]
    pub fn list(&self) -> Vec<Workspace> {
        self.inner.workspaces.read().clone()
    }

    /// Fetch a workspace by id.
    ///
    /// # Errors
    ///
    /// [`WidgetError::WorkspaceNotFound`] when the id is unknown.
    pub fn get(&self, id: Uuid) -> Result<Workspace> {
        self.inner
            .workspaces
            .read()
            .iter()
            .find(|w| w.id == id)
            .cloned()
            .ok_or(WidgetError::WorkspaceNotFound(id))
    }

    /// Active workspace.
    ///
    /// # Errors
    ///
    /// [`WidgetError::WorkspaceNotFound`] when no workspace is active yet.
    pub fn active(&self) -> Result<Workspace> {
        let id = self
            .inner
            .active_id
            .read()
            .ok_or_else(|| WidgetError::WorkspaceNotFound(Uuid::nil()))?;
        self.get(id)
    }

    /// Create a new workspace with the given name.
    ///
    /// # Errors
    ///
    /// * [`WidgetError::WorkspaceLimitReached`] when `MAX_WORKSPACES` reached.
    pub async fn create(&self, name: String) -> Result<Uuid> {
        let mut list = self.inner.workspaces.write();
        if list.len() >= MAX_WORKSPACES {
            return Err(WidgetError::WorkspaceLimitReached {
                max: MAX_WORKSPACES,
            });
        }
        let ordinal = (list.len() as u8).saturating_add(1);
        let now = Utc::now();
        let ws = Workspace {
            id: Uuid::new_v4(),
            name: name.clone(),
            ordinal,
            wallpaper: None,
            created_at: now,
            updated_at: now,
        };
        let id = ws.id;
        list.push(ws.clone());
        drop(list);
        persistence::save(&self.inner.storage, &ws)?;

        // First workspace becomes active.
        let mut active = self.inner.active_id.write();
        if active.is_none() {
            *active = Some(id);
        }
        drop(active);

        self.inner.bus.publish(
            orchid_core::EventSource::Subsystem("widgets".into()),
            WorkspaceCreated {
                workspace_id: id,
                name,
            },
        );
        debug!(workspace_id = %id, "workspace created");
        Ok(id)
    }

    /// Rename a workspace.
    ///
    /// # Errors
    ///
    /// [`WidgetError::WorkspaceNotFound`].
    pub async fn rename(&self, id: Uuid, name: String) -> Result<()> {
        let ws_snapshot = {
            let mut list = self.inner.workspaces.write();
            let ws = list
                .iter_mut()
                .find(|w| w.id == id)
                .ok_or(WidgetError::WorkspaceNotFound(id))?;
            ws.name = name.clone();
            persistence::touch(ws);
            ws.clone()
        };
        persistence::save(&self.inner.storage, &ws_snapshot)?;
        self.inner.bus.publish(
            orchid_core::EventSource::Subsystem("widgets".into()),
            WorkspaceRenamed {
                workspace_id: id,
                name,
            },
        );
        Ok(())
    }

    /// Delete a workspace.
    ///
    /// # Errors
    ///
    /// * [`WidgetError::WorkspaceNotFound`].
    /// * [`WidgetError::InvalidStateForOperation`] when trying to delete the
    ///   active or the last remaining workspace.
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        let active = *self.inner.active_id.read();
        if active == Some(id) {
            return Err(WidgetError::InvalidStateForOperation(
                "cannot delete the currently active workspace".into(),
            ));
        }
        let mut list = self.inner.workspaces.write();
        if list.len() <= 1 {
            return Err(WidgetError::InvalidStateForOperation(
                "cannot delete the last remaining workspace".into(),
            ));
        }
        let idx = list
            .iter()
            .position(|w| w.id == id)
            .ok_or(WidgetError::WorkspaceNotFound(id))?;
        list.remove(idx);
        operations::redensify(&mut list);
        let snapshot = list.clone();
        drop(list);
        persistence::delete(&self.inner.storage, id)?;
        persistence::save_all(&self.inner.storage, &snapshot)?;
        self.inner.bus.publish(
            orchid_core::EventSource::Subsystem("widgets".into()),
            WorkspaceDeleted { workspace_id: id },
        );
        Ok(())
    }

    /// Switch the active workspace.
    ///
    /// # Errors
    ///
    /// [`WidgetError::WorkspaceNotFound`].
    pub async fn switch_to(&self, id: Uuid) -> Result<()> {
        {
            let list = self.inner.workspaces.read();
            if !list.iter().any(|w| w.id == id) {
                return Err(WidgetError::WorkspaceNotFound(id));
            }
        }
        let previous = {
            let mut active = self.inner.active_id.write();
            let prev = *active;
            *active = Some(id);
            prev
        };
        if previous != Some(id) {
            self.inner.bus.publish(
                orchid_core::EventSource::Subsystem("widgets".into()),
                WorkspaceSwitched {
                    from: previous,
                    to: id,
                },
            );
        }
        Ok(())
    }

    /// Switch to the workspace with the given ordinal.
    ///
    /// # Errors
    ///
    /// [`WidgetError::WorkspaceNotFound`] when no workspace has that ordinal.
    pub async fn switch_by_ordinal(&self, ordinal: u8) -> Result<()> {
        let id = self
            .inner
            .workspaces
            .read()
            .iter()
            .find(|w| w.ordinal == ordinal)
            .map(|w| w.id)
            .ok_or(WidgetError::WorkspaceNotFound(Uuid::nil()))?;
        self.switch_to(id).await
    }

    /// Switch to the next workspace (wraps around).
    ///
    /// # Errors
    ///
    /// [`WidgetError::WorkspaceNotFound`] when no workspaces exist.
    pub async fn switch_next(&self) -> Result<()> {
        self.relative_switch(1).await
    }

    /// Switch to the previous workspace (wraps around).
    ///
    /// # Errors
    ///
    /// [`WidgetError::WorkspaceNotFound`] when no workspaces exist.
    pub async fn switch_previous(&self) -> Result<()> {
        self.relative_switch(-1).await
    }

    async fn relative_switch(&self, delta: i32) -> Result<()> {
        let id = {
            let list = self.inner.workspaces.read();
            if list.is_empty() {
                return Err(WidgetError::WorkspaceNotFound(Uuid::nil()));
            }
            let active = *self.inner.active_id.read();
            let idx = match active {
                Some(a) => list.iter().position(|w| w.id == a).unwrap_or(0),
                None => 0,
            };
            let len = list.len() as i32;
            let next = ((idx as i32 + delta).rem_euclid(len)) as usize;
            list[next].id
        };
        self.switch_to(id).await
    }

    /// Move a workspace to a new ordinal.
    ///
    /// # Errors
    ///
    /// [`WidgetError::WorkspaceNotFound`].
    pub async fn reorder(&self, id: Uuid, new_ordinal: u8) -> Result<()> {
        let snapshot = {
            let mut list = self.inner.workspaces.write();
            operations::reorder(&mut list, id, new_ordinal)?;
            list.clone()
        };
        persistence::save_all(&self.inner.storage, &snapshot)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> Arc<StateStore> {
        Arc::new(StateStore::open_in_memory("test").unwrap())
    }

    #[tokio::test]
    async fn create_and_switch() {
        let bus = Arc::new(orchid_core::EventBus::new(Default::default()));
        let storage = store().await;
        let mgr = WorkspaceManager::new(bus, storage);

        let a = mgr.create("A".into()).await.unwrap();
        let b = mgr.create("B".into()).await.unwrap();
        assert_eq!(mgr.list().len(), 2);
        assert_eq!(mgr.active().unwrap().id, a);

        mgr.switch_to(b).await.unwrap();
        assert_eq!(mgr.active().unwrap().id, b);

        mgr.switch_previous().await.unwrap();
        assert_eq!(mgr.active().unwrap().id, a);
    }

    #[tokio::test]
    async fn delete_active_rejected() {
        let bus = Arc::new(orchid_core::EventBus::new(Default::default()));
        let storage = store().await;
        let mgr = WorkspaceManager::new(bus, storage);
        let a = mgr.create("A".into()).await.unwrap();
        let _b = mgr.create("B".into()).await.unwrap();
        let err = mgr.delete(a).await.unwrap_err();
        assert!(matches!(err, WidgetError::InvalidStateForOperation(_)));
    }

    #[tokio::test]
    async fn cannot_delete_last_workspace() {
        let bus = Arc::new(orchid_core::EventBus::new(Default::default()));
        let storage = store().await;
        let mgr = WorkspaceManager::new(bus, storage);
        let a = mgr.create("A".into()).await.unwrap();
        let b = mgr.create("B".into()).await.unwrap();
        mgr.switch_to(b).await.unwrap();
        mgr.delete(a).await.unwrap();
        // Now only one workspace left (b); can't delete it.
        let err = mgr.delete(b).await.unwrap_err();
        assert!(matches!(err, WidgetError::InvalidStateForOperation(_)));
    }
}
