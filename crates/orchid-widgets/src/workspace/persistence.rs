//! Storage round-trips for workspaces.

use chrono::Utc;
use orchid_storage::{StateStore, Workspace};
use uuid::Uuid;

use crate::error::Result;

/// Load every persisted workspace ordered by ordinal.
///
/// # Errors
///
/// Propagates redb errors.
pub fn load_all(storage: &StateStore) -> Result<Vec<Workspace>> {
    let txn = storage.read()?;
    let mut list = txn.list_workspaces()?;
    list.sort_by_key(|w| w.ordinal);
    Ok(list)
}

/// Persist a workspace.
///
/// # Errors
///
/// Propagates redb errors.
pub fn save(storage: &StateStore, ws: &Workspace) -> Result<()> {
    let mut w = storage.write()?;
    w.put_workspace(ws)?;
    w.commit()?;
    Ok(())
}

/// Delete a workspace by id.
///
/// # Errors
///
/// Propagates redb errors.
pub fn delete(storage: &StateStore, id: Uuid) -> Result<bool> {
    let mut w = storage.write()?;
    let existed = w.delete_workspace(id)?;
    w.commit()?;
    Ok(existed)
}

/// Persist a list of workspaces atomically, overwriting prior rows.
///
/// # Errors
///
/// Propagates redb errors.
pub fn save_all(storage: &StateStore, workspaces: &[Workspace]) -> Result<()> {
    let mut w = storage.write()?;
    for ws in workspaces {
        w.put_workspace(ws)?;
    }
    w.commit()?;
    Ok(())
}

/// Convenience: timestamp utility used by operation helpers.
#[inline]
pub fn touch(ws: &mut Workspace) {
    ws.updated_at = Utc::now();
}
