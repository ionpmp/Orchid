//! Ordinal-maintenance helpers for [`super::WorkspaceManager`].

use orchid_storage::Workspace;

use crate::error::{Result, WidgetError};

/// Enforce dense ordinals `1..=n` across the list, in-place, keeping the
/// relative order defined by the vector.
pub fn redensify(list: &mut [Workspace]) {
    for (idx, ws) in list.iter_mut().enumerate() {
        ws.ordinal = (idx as u8).saturating_add(1);
    }
}

/// Move the workspace with the given id to `new_ordinal`, shifting others.
///
/// # Errors
///
/// * [`WidgetError::WorkspaceNotFound`] when the id is not present.
pub fn reorder(list: &mut Vec<Workspace>, id: uuid::Uuid, new_ordinal: u8) -> Result<()> {
    let current_idx = list
        .iter()
        .position(|w| w.id == id)
        .ok_or(WidgetError::WorkspaceNotFound(id))?;
    let target_idx = (new_ordinal.saturating_sub(1) as usize).min(list.len() - 1);
    if current_idx == target_idx {
        return Ok(());
    }
    let moved = list.remove(current_idx);
    list.insert(target_idx, moved);
    redensify(list);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn ws(name: &str, ordinal: u8) -> Workspace {
        Workspace {
            id: Uuid::new_v4(),
            name: name.into(),
            ordinal,
            wallpaper: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn redensify_rewrites_ordinals() {
        let mut list = vec![ws("a", 3), ws("b", 9), ws("c", 1)];
        redensify(&mut list);
        assert_eq!(list[0].ordinal, 1);
        assert_eq!(list[1].ordinal, 2);
        assert_eq!(list[2].ordinal, 3);
    }

    #[test]
    fn reorder_maintains_density() {
        let mut list = vec![ws("a", 1), ws("b", 2), ws("c", 3)];
        let id = list[2].id;
        reorder(&mut list, id, 1).unwrap();
        assert_eq!(list[0].name, "c");
        assert_eq!(list[0].ordinal, 1);
        assert_eq!(list[1].ordinal, 2);
        assert_eq!(list[2].ordinal, 3);
    }
}
