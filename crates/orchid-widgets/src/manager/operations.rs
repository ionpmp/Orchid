//! Verbs the rest of the system uses to manipulate widgets.

use std::sync::Arc;

use chrono::Utc;
use orchid_storage::{GridPosition, LifecycleState, WidgetSize};
use parking_lot::RwLock;
use tokio::sync::Mutex;
use tracing::debug;
use uuid::Uuid;

use crate::error::{Result, WidgetError};
use crate::events::{WidgetCreated, WidgetMoved, WidgetResized};
use crate::layout::grid::size_in_cells;
use crate::manager::{persistence, WidgetManager};
use crate::widget::instance::WidgetInstanceRuntime;
use crate::widget::WidgetContext;

/// Arguments to [`WidgetManager::create`].
#[derive(Debug, Clone)]
pub struct CreateWidgetRequest {
    /// Widget type id (must exist in the registry).
    pub type_id: String,
    /// Workspace the instance is created on.
    pub workspace_id: Uuid,
    /// Optional position; `None` → auto-placement via [`LayoutEngine`].
    pub position: Option<GridPosition>,
    /// Optional initial size; `None` → descriptor default.
    pub size: Option<WidgetSize>,
    /// Initial lifecycle; `None` → descriptor default.
    pub initial_lifecycle: Option<LifecycleState>,
    /// Optional bytes produced by a previous [`crate::Widget::save_state`].
    pub config_bytes: Option<Vec<u8>>,
}

impl WidgetManager {
    /// Create a widget instance.
    ///
    /// # Errors
    ///
    /// * [`WidgetError::UnknownWidgetType`] when the type is not registered.
    /// * [`WidgetError::InvalidSize`] when the requested size is out of the
    ///   descriptor's min/max.
    /// * [`WidgetError::CreationFailed`] when the factory fails.
    /// * Layout errors from auto-placement.
    pub async fn create(&self, request: CreateWidgetRequest) -> Result<Uuid> {
        let CreateWidgetRequest {
            type_id,
            workspace_id,
            position,
            size,
            initial_lifecycle,
            config_bytes,
        } = request;

        let descriptor = self
            .inner
            .registry
            .get(&type_id)
            .ok_or_else(|| WidgetError::UnknownWidgetType(type_id.clone()))?;

        let canonical_type_id = descriptor.type_id.to_string();
        let created_type_id = canonical_type_id.clone();

        let size = size.unwrap_or(descriptor.default_size);
        validate_size(&descriptor, size)?;

        // `position: None` uses a temporary grid origin; the UI (or any host with
        // a [`crate::LayoutEngine`]) should call
        // [`LayoutEngine::auto_place_excluding`] and [`Self::move_to`] immediately
        // after create so each instance gets a free cell.
        let position = position.unwrap_or(GridPosition { col: 0, row: 0 });

        let instance_id = Uuid::new_v4();
        let ctx = WidgetContext {
            bus: self.inner.bus.clone(),
            storage: self.inner.storage.clone(),
            config: self.inner.config.clone(),
            locale: self.inner.locale.clone(),
            instance_id,
            workspace_id,
        };
        let widget = (descriptor.factory)(ctx.clone(), config_bytes.as_deref())
            .map_err(|e| WidgetError::CreationFailed(e.to_string()))?;

        let now = Utc::now();
        let initial_lifecycle = initial_lifecycle.unwrap_or(descriptor.default_lifecycle);
        let runtime = Arc::new(WidgetInstanceRuntime {
            id: instance_id,
            workspace_id,
            type_id: canonical_type_id,
            position: RwLock::new(position),
            size: RwLock::new(size),
            lifecycle: RwLock::new(LifecycleState::Active),
            group_id: RwLock::new(None),
            created_at: now,
            updated_at: RwLock::new(now),
            widget: Mutex::new(widget),
            last_snapshot: RwLock::new(None),
            last_touched: RwLock::new(now),
        });

        // Do not hold the widget mutex across both hooks: `on_*` methods may
        // `.await` work that needs the snapshot pump (or other tasks) to make
        // progress while they still hold interior locks.
        {
            let mut w = runtime.widget.lock().await;
            w.on_create(&ctx).await?;
        }
        {
            let mut w = runtime.widget.lock().await;
            w.on_activate(&ctx).await?;
        }

        // Transition to the requested initial lifecycle if not Active.
        if initial_lifecycle != LifecycleState::Active {
            let ctx = self.context_for(&runtime);
            self.inner
                .lifecycle
                .transition(&runtime, &ctx, initial_lifecycle)
                .await?;
        }

        // Persist first so storage mirrors in-memory state before the
        // creation event fires.
        let bytes = {
            let w = runtime.widget.lock().await;
            w.save_state().unwrap_or_default()
        };
        persistence::save_instance(&self.inner.storage, &runtime, Some(bytes))?;

        self.inner.instances.insert(instance_id, runtime);
        self.inner.bus.publish(
            orchid_core::EventSource::Subsystem("widgets".into()),
            WidgetCreated {
                instance_id,
                workspace_id,
                type_id: created_type_id,
            },
        );
        debug!(%instance_id, %workspace_id, "widget created");
        Ok(instance_id)
    }

    /// Close a widget.
    ///
    /// # Errors
    ///
    /// Propagates the `on_close` error and storage errors.
    pub async fn close(&self, id: Uuid) -> Result<()> {
        self.close_locked(id, true).await
    }

    /// Move a widget to a new grid position. The caller is responsible for
    /// collision validation (use [`crate::LayoutEngine::can_place`] ahead of
    /// this call).
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub async fn move_to(&self, id: Uuid, position: GridPosition) -> Result<()> {
        let instance = self.get_instance(id)?;
        let from = *instance.position.read();
        if from == position {
            return Ok(());
        }
        *instance.position.write() = position;
        *instance.updated_at.write() = Utc::now();

        let bytes = {
            let w = instance.widget.lock().await;
            w.save_state().unwrap_or_default()
        };
        persistence::save_instance(&self.inner.storage, &instance, Some(bytes))?;

        self.inner.bus.publish(
            orchid_core::EventSource::Subsystem("widgets".into()),
            WidgetMoved {
                instance_id: id,
                from,
                to: position,
            },
        );
        Ok(())
    }

    /// Resize a widget.
    ///
    /// # Errors
    ///
    /// * [`WidgetError::InvalidSize`] when the size violates capabilities.
    /// * Propagates `on_resize` and storage errors.
    pub async fn resize(&self, id: Uuid, size: WidgetSize) -> Result<()> {
        let instance = self.get_instance(id)?;
        let descriptor = self
            .inner
            .registry
            .get(&instance.type_id)
            .ok_or_else(|| WidgetError::UnknownWidgetType(instance.type_id.clone()))?;
        validate_size(&descriptor, size)?;

        let from = *instance.size.read();
        if from == size {
            return Ok(());
        }

        let ctx = self.context_for(&instance);
        {
            let mut w = instance.widget.lock().await;
            w.on_resize(&ctx, size).await?;
        }
        *instance.size.write() = size;
        *instance.updated_at.write() = Utc::now();

        let bytes = {
            let w = instance.widget.lock().await;
            w.save_state().unwrap_or_default()
        };
        persistence::save_instance(&self.inner.storage, &instance, Some(bytes))?;

        self.inner.bus.publish(
            orchid_core::EventSource::Subsystem("widgets".into()),
            WidgetResized {
                instance_id: id,
                from,
                to: size,
            },
        );
        Ok(())
    }

    /// Change the lifecycle state of a widget.
    ///
    /// # Errors
    ///
    /// Propagates lifecycle / storage errors.
    pub async fn change_lifecycle(&self, id: Uuid, target: LifecycleState) -> Result<()> {
        let instance = self.get_instance(id)?;
        let ctx = self.context_for(&instance);
        self.inner.lifecycle.transition(&instance, &ctx, target).await?;

        // Persist lifecycle change.
        let bytes = {
            let w = instance.widget.lock().await;
            w.save_state().unwrap_or_default()
        };
        persistence::save_instance(&self.inner.storage, &instance, Some(bytes))?;
        Ok(())
    }

    /// Move a widget to a different workspace.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub async fn assign_to_workspace(&self, id: Uuid, workspace_id: Uuid) -> Result<()> {
        let instance = self.get_instance(id)?;
        if instance.workspace_id == workspace_id {
            return Ok(());
        }
        // We rebuild the instance — `workspace_id` is immutable in the
        // runtime to keep locking straightforward. Move the widget row in
        // storage, drop the old in-memory entry, and insert a new one.
        let bytes = {
            let w = instance.widget.lock().await;
            w.save_state().unwrap_or_default()
        };
        let now = Utc::now();
        let new_runtime = Arc::new(WidgetInstanceRuntime {
            id: instance.id,
            workspace_id,
            type_id: instance.type_id.clone(),
            position: RwLock::new(*instance.position.read()),
            size: RwLock::new(*instance.size.read()),
            lifecycle: RwLock::new(*instance.lifecycle.read()),
            group_id: RwLock::new(*instance.group_id.read()),
            created_at: instance.created_at,
            updated_at: RwLock::new(now),
            widget: Mutex::new({
                // Swap the widget out of the old runtime so we don't double-
                // own it. We need a fresh `Box<dyn Widget>` here; because the
                // old runtime is still referenced by `instance`, we can't
                // move its inner widget out. For MVP we fall back to the
                // descriptor factory with the saved state bytes.
                let descriptor = self
                    .inner
                    .registry
                    .get(&instance.type_id)
                    .ok_or_else(|| {
                        WidgetError::UnknownWidgetType(instance.type_id.clone())
                    })?;
                let ctx = WidgetContext {
                    bus: self.inner.bus.clone(),
                    storage: self.inner.storage.clone(),
                    config: self.inner.config.clone(),
                    locale: self.inner.locale.clone(),
                    instance_id: instance.id,
                    workspace_id,
                };
                (descriptor.factory)(ctx, Some(&bytes))
                    .map_err(|e| WidgetError::CreationFailed(e.to_string()))?
            }),
            last_snapshot: RwLock::new(instance.last_snapshot.read().clone()),
            last_touched: RwLock::new(now),
        });
        persistence::save_instance(&self.inner.storage, &new_runtime, Some(bytes))?;
        self.inner.instances.insert(instance.id, new_runtime);
        Ok(())
    }

    /// Flush the widget's current state to storage.
    ///
    /// # Errors
    ///
    /// Propagates `save_state` / storage errors.
    pub async fn save_widget_state(&self, id: Uuid) -> Result<()> {
        let instance = self.get_instance(id)?;
        let bytes = {
            let w = instance.widget.lock().await;
            w.save_state()?
        };
        persistence::save_instance(&self.inner.storage, &instance, Some(bytes))?;
        Ok(())
    }
}

fn validate_size(
    descriptor: &crate::widget::descriptor::WidgetDescriptor,
    size: WidgetSize,
) -> Result<()> {
    let (w, h) = size_in_cells(size);
    if let Some(min) = descriptor.min_size {
        let (mw, mh) = size_in_cells(min);
        if w < mw || h < mh {
            return Err(WidgetError::InvalidSize {
                reason: format!("below descriptor min {min:?}"),
            });
        }
    }
    if let Some(max) = descriptor.max_size {
        let (mw, mh) = size_in_cells(max);
        if w > mw || h > mh {
            return Err(WidgetError::InvalidSize {
                reason: format!("above descriptor max {max:?}"),
            });
        }
    }
    Ok(())
}
