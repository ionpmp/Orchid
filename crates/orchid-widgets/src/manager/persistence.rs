//! Storage round-trips for widget instances.

use orchid_storage::{StateStore, WidgetInstance};
use uuid::Uuid;

use crate::error::Result;
use crate::widget::instance::WidgetInstanceRuntime;

/// Persist a single widget instance. `Some(state_bytes)` updates the
/// cached config; `None` reuses [`WidgetInstanceRuntime::last_config`].
///
/// # Errors
///
/// Propagates storage errors.
pub fn save_instance(
    storage: &StateStore,
    instance: &WidgetInstanceRuntime,
    state_bytes: Option<Vec<u8>>,
) -> Result<()> {
    let config = match state_bytes {
        Some(bytes) => {
            *instance.last_config.write() = bytes.clone();
            bytes
        }
        None => instance.last_config.read().clone(),
    };
    save_instances_batch(storage, &[instance.to_storage(config)])
}

/// Write many widget rows in one redb transaction (one fsync).
///
/// # Errors
///
/// Propagates storage errors.
pub fn save_instances_batch(storage: &StateStore, rows: &[WidgetInstance]) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut w = storage.write()?;
    for row in rows {
        w.put_widget(row)?;
    }
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
    Ok(txn.list_all_widgets()?)
}
