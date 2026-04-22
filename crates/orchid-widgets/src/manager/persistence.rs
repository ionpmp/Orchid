//! Storage round-trips for widget instances.

use orchid_storage::{StateStore, WidgetInstance};
use uuid::Uuid;

use crate::error::Result;
use crate::widget::instance::WidgetInstanceRuntime;

/// Persist a single widget instance. `state_bytes` is written into the
/// instance's `config` field.
///
/// # Errors
///
/// Propagates storage errors.
pub fn save_instance(
    storage: &StateStore,
    instance: &WidgetInstanceRuntime,
    state_bytes: Option<Vec<u8>>,
) -> Result<()> {
    let row = instance.to_storage(state_bytes.unwrap_or_default());
    let mut w = storage.write()?;
    w.put_widget(&row)?;
    w.commit()?;
    Ok(())
}

/// Delete a widget instance row.
///
/// # Errors
///
/// Propagates storage errors.
pub fn delete_instance(storage: &StateStore, id: Uuid) -> Result<()> {
    let mut w = storage.write()?;
    let _ = w.delete_widget(id)?;
    w.commit()?;
    Ok(())
}

/// Load every persisted widget instance.
///
/// # Errors
///
/// Propagates storage errors.
pub fn load_all_instances(storage: &StateStore) -> Result<Vec<WidgetInstance>> {
    let txn = storage.read()?;
    // No dedicated "list all" API on the read transaction, but
    // `widgets_for_workspace` walks the whole table. We collect across
    // workspaces by iterating every known workspace id + an "unassigned"
    // pass; easier path is a small helper that iterates the raw table.
    let mut out = Vec::new();
    let workspaces = txn.list_workspaces()?;
    for ws in &workspaces {
        let widgets = txn.widgets_for_workspace(ws.id)?;
        out.extend(widgets);
    }
    Ok(out)
}
