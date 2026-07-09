//! Widget groups — tab stacks of widgets that share a layout slot.

pub mod operations;

use std::sync::Arc;

use bincode::{Decode, Encode};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use orchid_storage::{GridPosition, WidgetSize};
use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use uuid::Uuid;

use crate::error::{Result, WidgetError};
use crate::events::{GroupActiveChanged, GroupCreated, GroupDissolved};

/// Group table backed by `raw_database()` on [`orchid_storage::StateStore`].
pub(crate) const GROUPS_TABLE: TableDefinition<'_, &[u8; 16], orchid_storage::Value<WidgetGroup>> =
    TableDefinition::new("widget_groups");

/// A tab-stack of widgets occupying a single layout slot.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct WidgetGroup {
    /// Group id.
    #[bincode(with_serde)]
    pub id: Uuid,
    /// Workspace this group belongs to.
    #[bincode(with_serde)]
    pub workspace_id: Uuid,
    /// Widget instance ids, left-to-right tab order.
    #[bincode(with_serde)]
    pub members: SmallVec<[Uuid; 4]>,
    /// Index of the currently active tab.
    pub active_member: u16,
    /// Position that applies to the group as a whole.
    pub position: GridPosition,
    /// Size that applies to the group as a whole.
    pub size: WidgetSize,
    /// When the group was created.
    #[bincode(with_serde)]
    pub created_at: DateTime<Utc>,
}

impl WidgetGroup {
    /// Construct an empty group.
    #[must_use]
    pub fn new(workspace_id: Uuid, position: GridPosition, size: WidgetSize) -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id,
            members: SmallVec::new(),
            active_member: 0,
            position,
            size,
            created_at: Utc::now(),
        }
    }

    /// Append `instance_id` to the member list.
    pub fn add_member(&mut self, instance_id: Uuid) {
        if !self.members.contains(&instance_id) {
            self.members.push(instance_id);
        }
    }

    /// Remove `instance_id` from the group.
    ///
    /// # Errors
    ///
    /// Returns [`WidgetError::WidgetNotInGroup`] when `instance_id` is not a
    /// member.
    pub fn remove_member(&mut self, instance_id: Uuid) -> Result<()> {
        let idx = self
            .members
            .iter()
            .position(|m| *m == instance_id)
            .ok_or(WidgetError::WidgetNotInGroup)?;
        self.members.remove(idx);
        if self.active_member as usize >= self.members.len() {
            self.active_member = self.members.len().saturating_sub(1) as u16;
        }
        Ok(())
    }

    /// Reorder a member from `from` to `to` (both zero-based indices).
    ///
    /// # Errors
    ///
    /// Returns [`WidgetError::GroupMoveError`] on out-of-range indices.
    pub fn reorder(&mut self, from: usize, to: usize) -> Result<()> {
        if from >= self.members.len() || to >= self.members.len() {
            return Err(WidgetError::GroupMoveError(format!(
                "reorder indices ({from}, {to}) out of range for {} members",
                self.members.len()
            )));
        }
        let active_id = self.active_instance();
        let member = self.members.remove(from);
        self.members.insert(to, member);
        if let Some(id) = active_id {
            if let Some(idx) = self.members.iter().position(|m| *m == id) {
                self.active_member = idx as u16;
            }
        }
        Ok(())
    }

    /// Activate the given member.
    ///
    /// # Errors
    ///
    /// Returns [`WidgetError::WidgetNotInGroup`] when `instance_id` is not a
    /// member.
    pub fn activate(&mut self, instance_id: Uuid) -> Result<()> {
        let idx = self
            .members
            .iter()
            .position(|m| *m == instance_id)
            .ok_or(WidgetError::WidgetNotInGroup)?;
        self.active_member = idx as u16;
        Ok(())
    }

    /// Currently active member (or `None` if the group is empty).
    #[must_use]
    pub fn active_instance(&self) -> Option<Uuid> {
        self.members.get(self.active_member as usize).copied()
    }
}

/// Owns all groups and persists them through [`orchid_storage::StateStore::raw_database`].
pub struct GroupManager {
    groups: DashMap<Uuid, WidgetGroup>,
    storage: Arc<orchid_storage::StateStore>,
    bus: Arc<orchid_core::EventBus>,
}

impl std::fmt::Debug for GroupManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupManager")
            .field("groups", &self.groups.len())
            .finish_non_exhaustive()
    }
}

impl GroupManager {
    /// Build a fresh manager. Call [`GroupManager::restore_from_storage`]
    /// afterwards to load any persisted groups.
    #[must_use]
    pub fn new(bus: Arc<orchid_core::EventBus>, storage: Arc<orchid_storage::StateStore>) -> Self {
        Self {
            groups: DashMap::new(),
            storage,
            bus,
        }
    }

    /// Load every persisted group into memory.
    ///
    /// # Errors
    ///
    /// Propagates redb errors.
    pub fn restore_from_storage(&self) -> Result<usize> {
        let db = self.storage.raw_database();
        let txn = db.begin_read().map_err(orchid_storage::StorageError::from)?;
        let table = match txn.open_table(GROUPS_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(0),
            Err(e) => return Err(orchid_storage::StorageError::from(e).into()),
        };
        let mut count = 0;
        for entry in table.iter().map_err(orchid_storage::StorageError::from)? {
            let (_k, v) = entry.map_err(orchid_storage::StorageError::from)?;
            let group = v.value();
            self.groups.insert(group.id, group);
            count += 1;
        }
        Ok(count)
    }

    /// Persist a single group (insert or overwrite).
    pub(crate) fn persist(&self, group: &WidgetGroup) -> Result<()> {
        let db = self.storage.raw_database();
        let txn = db.begin_write().map_err(orchid_storage::StorageError::from)?;
        {
            let mut table = txn.open_table(GROUPS_TABLE).map_err(orchid_storage::StorageError::from)?;
            let key = uuid_key(group.id);
            table
                .insert(&key, group)
                .map_err(orchid_storage::StorageError::from)?;
        }
        txn.commit().map_err(orchid_storage::StorageError::from)?;
        Ok(())
    }

    pub(crate) fn delete_persisted(&self, id: Uuid) -> Result<()> {
        let db = self.storage.raw_database();
        let txn = db.begin_write().map_err(orchid_storage::StorageError::from)?;
        let key = uuid_key(id);
        let missing_table = {
            match txn.open_table(GROUPS_TABLE) {
                Ok(mut table) => {
                    let _ = table
                        .remove(&key)
                        .map_err(orchid_storage::StorageError::from)?;
                    false
                }
                Err(redb::TableError::TableDoesNotExist(_)) => true,
                Err(e) => return Err(orchid_storage::StorageError::from(e).into()),
            }
        };
        if missing_table {
            // Table does not exist yet — nothing to delete, but we still
            // commit an empty transaction so the caller's contract (
            // "storage reflects the in-memory state") holds.
        }
        txn.commit().map_err(orchid_storage::StorageError::from)?;
        Ok(())
    }

    /// Form a new group from the given `members`.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub async fn create_group(
        &self,
        workspace_id: Uuid,
        members: Vec<Uuid>,
        position: GridPosition,
        size: WidgetSize,
    ) -> Result<Uuid> {
        let mut group = WidgetGroup::new(workspace_id, position, size);
        for m in members {
            group.add_member(m);
        }
        let id = group.id;
        self.persist(&group)?;
        self.groups.insert(id, group);

        self.bus.publish(
            orchid_core::EventSource::Subsystem("widgets".into()),
            GroupCreated {
                group_id: id,
                workspace_id,
            },
        );
        Ok(id)
    }

    /// Dissolve a group, returning the list of its former members so the
    /// caller can re-place them on the workspace.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub async fn dissolve_group(&self, group_id: Uuid) -> Result<Vec<Uuid>> {
        let (_, group) = self
            .groups
            .remove(&group_id)
            .ok_or(WidgetError::GroupNotFound(group_id))?;
        self.delete_persisted(group_id)?;
        self.bus.publish(
            orchid_core::EventSource::Subsystem("widgets".into()),
            GroupDissolved { group_id },
        );
        Ok(group.members.into_iter().collect())
    }

    /// Append a member to a group.
    ///
    /// # Errors
    ///
    /// Returns [`WidgetError::GroupNotFound`] when `group_id` is unknown.
    pub async fn add_to_group(&self, group_id: Uuid, instance_id: Uuid) -> Result<()> {
        let mut entry = self
            .groups
            .get_mut(&group_id)
            .ok_or(WidgetError::GroupNotFound(group_id))?;
        entry.value_mut().add_member(instance_id);
        let group = entry.value().clone();
        drop(entry);
        self.persist(&group)?;
        Ok(())
    }

    /// Remove a member from a group.
    ///
    /// # Errors
    ///
    /// Returns [`WidgetError::GroupNotFound`] or [`WidgetError::WidgetNotInGroup`].
    pub async fn remove_from_group(&self, group_id: Uuid, instance_id: Uuid) -> Result<()> {
        let mut entry = self
            .groups
            .get_mut(&group_id)
            .ok_or(WidgetError::GroupNotFound(group_id))?;
        entry.value_mut().remove_member(instance_id)?;
        let group = entry.value().clone();
        drop(entry);
        self.persist(&group)?;
        Ok(())
    }

    /// Switch the active tab of the group.
    ///
    /// # Errors
    ///
    /// Returns [`WidgetError::GroupNotFound`] or [`WidgetError::WidgetNotInGroup`].
    pub async fn switch_active(&self, group_id: Uuid, instance_id: Uuid) -> Result<()> {
        let mut entry = self
            .groups
            .get_mut(&group_id)
            .ok_or(WidgetError::GroupNotFound(group_id))?;
        entry.value_mut().activate(instance_id)?;
        let group = entry.value().clone();
        drop(entry);
        self.persist(&group)?;
        self.bus.publish(
            orchid_core::EventSource::Subsystem("widgets".into()),
            GroupActiveChanged {
                group_id,
                instance_id,
            },
        );
        Ok(())
    }

    /// Update the shared layout slot for a group.
    ///
    /// # Errors
    ///
    /// Returns [`WidgetError::GroupNotFound`].
    pub async fn update_slot(
        &self,
        group_id: Uuid,
        position: GridPosition,
        size: WidgetSize,
    ) -> Result<()> {
        let mut entry = self
            .groups
            .get_mut(&group_id)
            .ok_or(WidgetError::GroupNotFound(group_id))?;
        entry.value_mut().position = position;
        entry.value_mut().size = size;
        let group = entry.value().clone();
        drop(entry);
        self.persist(&group)?;
        Ok(())
    }

    /// Reorder a member within a group (tab strip order).
    ///
    /// # Errors
    ///
    /// Returns [`WidgetError::GroupNotFound`] or [`WidgetError::GroupMoveError`].
    pub async fn reorder_members(
        &self,
        group_id: Uuid,
        from: usize,
        to: usize,
    ) -> Result<()> {
        let mut entry = self
            .groups
            .get_mut(&group_id)
            .ok_or(WidgetError::GroupNotFound(group_id))?;
        entry.value_mut().reorder(from, to)?;
        let group = entry.value().clone();
        drop(entry);
        self.persist(&group)?;
        Ok(())
    }

    /// Fetch a group by id.
    ///
    /// # Errors
    ///
    /// Returns [`WidgetError::GroupNotFound`].
    pub fn get(&self, group_id: Uuid) -> Result<WidgetGroup> {
        self.groups
            .get(&group_id)
            .map(|g| g.value().clone())
            .ok_or(WidgetError::GroupNotFound(group_id))
    }

    /// Every group attached to the given workspace.
    #[must_use]
    pub fn list_for_workspace(&self, workspace_id: Uuid) -> Vec<WidgetGroup> {
        self.groups
            .iter()
            .filter(|g| g.value().workspace_id == workspace_id)
            .map(|g| g.value().clone())
            .collect()
    }

    /// Find the group that contains `instance_id`, if any.
    #[must_use]
    pub fn find_for_instance(&self, instance_id: Uuid) -> Option<WidgetGroup> {
        self.groups
            .iter()
            .find(|g| g.value().members.contains(&instance_id))
            .map(|g| g.value().clone())
    }

    /// Every in-memory group (all workspaces).
    #[must_use]
    pub fn list_all(&self) -> Vec<WidgetGroup> {
        self.groups.iter().map(|g| g.value().clone()).collect()
    }
}

#[inline]
fn uuid_key(id: Uuid) -> [u8; 16] {
    *id.as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_remove_reorder_members() {
        let mut g = WidgetGroup::new(
            Uuid::new_v4(),
            GridPosition { col: 0, row: 0 },
            WidgetSize::Small,
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        g.add_member(a);
        g.add_member(b);
        g.add_member(c);
        assert_eq!(g.members.len(), 3);

        g.reorder(0, 2).unwrap();
        assert_eq!(g.members[2], a);

        g.remove_member(b).unwrap();
        assert_eq!(g.members.len(), 2);

        assert!(matches!(
            g.remove_member(b),
            Err(WidgetError::WidgetNotInGroup)
        ));
    }

    #[test]
    fn activate_tracks_index() {
        let mut g = WidgetGroup::new(
            Uuid::new_v4(),
            GridPosition { col: 0, row: 0 },
            WidgetSize::Small,
        );
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        g.add_member(a);
        g.add_member(b);
        g.activate(b).unwrap();
        assert_eq!(g.active_instance(), Some(b));
    }
}
