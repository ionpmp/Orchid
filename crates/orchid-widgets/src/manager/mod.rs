//! Widget-instance lifecycle and book-keeping.

pub mod operations;
pub mod persistence;

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use dashmap::DashMap;
use orchid_storage::{LifecycleState, StateStore};
use parking_lot::RwLock;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::error::{Result, WidgetError};
use crate::events::WidgetClosed;
use crate::registry::WidgetRegistry;
use crate::widget::instance::{SharedInstance, WidgetInstanceRuntime};
use crate::widget::lifecycle::LifecycleController;
use crate::widget::WidgetContext;

pub use operations::CreateWidgetRequest;

/// Knobs for the background sweepers.
#[derive(Debug, Clone)]
pub struct WidgetManagerOptions {
    /// How long a widget must sit idle before being moved to `Sleeping`.
    pub idle_sleep_after: Duration,
    /// How long a widget must sit in `Sleeping` before being unloaded.
    pub idle_unload_after: Duration,
    /// How often the sweepers run.
    pub sweeper_interval: Duration,
}

impl Default for WidgetManagerOptions {
    fn default() -> Self {
        Self {
            idle_sleep_after: Duration::from_secs(5 * 60),
            idle_unload_after: Duration::from_secs(30 * 60),
            sweeper_interval: Duration::from_secs(30),
        }
    }
}

/// Inner state owned through an `Arc` by both the public handle and the
/// background sweeper.
pub(crate) struct WidgetManagerInner {
    registry: Arc<WidgetRegistry>,
    bus: Arc<orchid_core::EventBus>,
    storage: Arc<StateStore>,
    config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    instances: DashMap<Uuid, SharedInstance>,
    lifecycle: LifecycleController,
    options: RwLock<WidgetManagerOptions>,
    sweeper: Mutex<Option<JoinHandle<()>>>,
}

/// Public handle to the widget manager.
#[derive(Clone)]
pub struct WidgetManager {
    pub(crate) inner: Arc<WidgetManagerInner>,
}

impl std::fmt::Debug for WidgetManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WidgetManager")
            .field("instances", &self.inner.instances.len())
            .finish_non_exhaustive()
    }
}

impl WidgetManager {
    /// Build a new manager. Call [`WidgetManager::restore_from_storage`] to
    /// rehydrate, and [`WidgetManager::start`] to run the background
    /// sweepers.
    #[must_use]
    pub fn new(
        registry: Arc<WidgetRegistry>,
        bus: Arc<orchid_core::EventBus>,
        storage: Arc<StateStore>,
        config: Arc<RwLock<orchid_storage::OrchidConfig>>,
        options: WidgetManagerOptions,
    ) -> Self {
        let lifecycle = LifecycleController::new(bus.clone());
        Self {
            inner: Arc::new(WidgetManagerInner {
                registry,
                bus,
                storage,
                config,
                instances: DashMap::new(),
                lifecycle,
                options: RwLock::new(options),
                sweeper: Mutex::new(None),
            }),
        }
    }

    /// Access to the underlying registry (for commands and UI helpers).
    #[must_use]
    pub fn registry(&self) -> &Arc<WidgetRegistry> {
        &self.inner.registry
    }

    /// Shared event bus.
    #[must_use]
    pub fn bus(&self) -> &Arc<orchid_core::EventBus> {
        &self.inner.bus
    }

    /// Build a [`WidgetContext`] for the given instance.
    pub(crate) fn context_for(&self, instance: &WidgetInstanceRuntime) -> WidgetContext {
        WidgetContext {
            bus: self.inner.bus.clone(),
            storage: self.inner.storage.clone(),
            config: self.inner.config.clone(),
            instance_id: instance.id,
            workspace_id: instance.workspace_id,
        }
    }

    /// Every currently-known instance, regardless of workspace.
    #[must_use]
    pub fn list_instances(&self) -> Vec<SharedInstance> {
        self.inner
            .instances
            .iter()
            .map(|e| Arc::clone(e.value()))
            .collect()
    }

    /// Fetch an instance by id.
    ///
    /// # Errors
    ///
    /// [`WidgetError::InstanceNotFound`] when the id is unknown.
    pub fn get_instance(&self, id: Uuid) -> Result<SharedInstance> {
        self.inner
            .instances
            .get(&id)
            .map(|e| Arc::clone(e.value()))
            .ok_or(WidgetError::InstanceNotFound(id))
    }

    /// Every instance that belongs to `workspace_id`.
    #[must_use]
    pub fn instances_for_workspace(&self, workspace_id: Uuid) -> Vec<SharedInstance> {
        self.inner
            .instances
            .iter()
            .filter(|e| e.value().workspace_id == workspace_id)
            .map(|e| Arc::clone(e.value()))
            .collect()
    }

    /// Reset the idle timer on the given instance.
    pub fn touch(&self, id: Uuid) {
        if let Some(entry) = self.inner.instances.get(&id) {
            *entry.value().last_touched.write() = Utc::now();
        }
    }

    /// Snapshot every instance's persistent state to storage.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub async fn snapshot_to_storage(&self) -> Result<()> {
        for entry in self.inner.instances.iter() {
            let instance = Arc::clone(entry.value());
            let bytes = {
                let widget = instance.widget.lock().await;
                widget.save_state().unwrap_or_default()
            };
            persistence::save_instance(&self.inner.storage, &instance, Some(bytes))?;
        }
        Ok(())
    }

    /// Restore every persisted instance, invoking the factory for each.
    ///
    /// Returns the number of instances successfully restored.
    ///
    /// # Errors
    ///
    /// Propagates storage errors. Individual factory failures are logged
    /// and skipped; the rest of the restore proceeds.
    pub async fn restore_from_storage(&self) -> Result<usize> {
        let rows = persistence::load_all_instances(&self.inner.storage)?;
        let mut count = 0;
        for row in rows {
            let Some(desc) = self.inner.registry.get(&row.widget_type) else {
                warn!(
                    widget_id = %row.id,
                    widget_type = %row.widget_type,
                    "restore: unknown widget type; skipping"
                );
                continue;
            };

            let ctx = WidgetContext {
                bus: self.inner.bus.clone(),
                storage: self.inner.storage.clone(),
                config: self.inner.config.clone(),
                instance_id: row.id,
                workspace_id: row.workspace_id,
            };
            let widget = match (desc.factory)(ctx.clone(), Some(&row.config)) {
                Ok(w) => w,
                Err(e) => {
                    warn!(widget_id = %row.id, error = %e, "restore: factory failed");
                    continue;
                }
            };

            let now = Utc::now();
            let runtime = Arc::new(WidgetInstanceRuntime {
                id: row.id,
                workspace_id: row.workspace_id,
                type_id: row.widget_type.clone(),
                position: RwLock::new(row.position),
                size: RwLock::new(row.size),
                lifecycle: RwLock::new(row.lifecycle),
                group_id: RwLock::new(None),
                created_at: row.created_at,
                updated_at: RwLock::new(row.updated_at),
                widget: Mutex::new(widget),
                last_snapshot: RwLock::new(None),
                last_touched: RwLock::new(now),
            });

            // Give the widget a chance to warm up.
            {
                let ctx = self.context_for(&runtime);
                let mut w = runtime.widget.lock().await;
                if let Err(e) = w.on_create(&ctx).await {
                    warn!(widget_id = %row.id, error = %e, "restore: on_create failed");
                    continue;
                }
                if row.lifecycle == LifecycleState::Active {
                    if let Err(e) = w.on_activate(&ctx).await {
                        warn!(widget_id = %row.id, error = %e, "restore: on_activate failed");
                    }
                }
            }

            self.inner.instances.insert(row.id, runtime);
            count += 1;
        }
        Ok(count)
    }

    /// Spawn the idle sweeper.
    ///
    /// # Errors
    ///
    /// Always `Ok` for now; reserved for future failure modes.
    pub async fn start(&self) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let handle = tokio::spawn(async move {
            let interval = inner.options.read().sweeper_interval;
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                run_sweepers(&inner).await;
            }
        });
        *self.inner.sweeper.lock().await = Some(handle);
        Ok(())
    }

    /// Stop the sweeper and close every remaining instance.
    ///
    /// # Errors
    ///
    /// Propagates storage errors encountered while writing the final
    /// snapshot.
    pub async fn shutdown(&self) -> Result<()> {
        if let Some(h) = self.inner.sweeper.lock().await.take() {
            h.abort();
        }
        // Final snapshot before closing.
        if let Err(e) = self.snapshot_to_storage().await {
            warn!(error = %e, "final snapshot failed during shutdown");
        }
        // Walk every instance and invoke on_close.
        let ids: Vec<Uuid> = self
            .inner
            .instances
            .iter()
            .map(|e| *e.key())
            .collect();
        for id in ids {
            if let Err(e) = self.close(id).await {
                warn!(widget_id = %id, error = %e, "close during shutdown failed");
            }
        }
        Ok(())
    }
}

async fn run_sweepers(inner: &WidgetManagerInner) {
    let (idle, stale) = {
        let opts = inner.options.read();
        (opts.idle_sleep_after, opts.idle_unload_after)
    };
    let instances: Vec<SharedInstance> = inner
        .instances
        .iter()
        .map(|e| Arc::clone(e.value()))
        .collect();

    let ctx_for = |instance: &WidgetInstanceRuntime| -> WidgetContext {
        WidgetContext {
            bus: inner.bus.clone(),
            storage: inner.storage.clone(),
            config: inner.config.clone(),
            instance_id: instance.id,
            workspace_id: instance.workspace_id,
        }
    };

    if let Err(e) = inner.lifecycle.sleep_idle(&instances, idle, &ctx_for).await {
        warn!(error = %e, "sleep_idle sweep failed");
    }
    if let Err(e) = inner
        .lifecycle
        .unload_stale(&instances, stale, &ctx_for)
        .await
    {
        warn!(error = %e, "unload_stale sweep failed");
    }
    debug!("sweeper tick complete");
}

impl WidgetManager {
    /// Close an instance — invokes `on_close`, removes it from the in-memory
    /// map, and deletes its persisted row.
    pub(crate) async fn close_locked(&self, id: Uuid) -> Result<()> {
        let (_, instance) = self
            .inner
            .instances
            .remove(&id)
            .ok_or(WidgetError::InstanceNotFound(id))?;

        let ctx = self.context_for(&instance);
        {
            let mut w = instance.widget.lock().await;
            w.on_close(&ctx).await?;
        }
        persistence::delete_instance(&self.inner.storage, id)?;
        self.inner.bus.publish(
            orchid_core::EventSource::Subsystem("widgets".into()),
            WidgetClosed { instance_id: id },
        );
        Ok(())
    }
}
