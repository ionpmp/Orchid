//! Widget-instance lifecycle and book-keeping.

pub mod operations;
pub mod persistence;
mod snapshot_eq;

use std::collections::HashSet;
use std::mem;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use dashmap::DashMap;
use orchid_core::{Event, EventEnvelope, EventFilter, HandlerPriority, SubscriptionHandle};
use orchid_storage::{LifecycleState, StateStore};
use parking_lot::RwLock;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, trace, warn};
use uuid::Uuid;

use crate::error::{Result, WidgetError};
use crate::events::{WidgetClosed, WidgetSnapshotUpdated};
use crate::registry::WidgetRegistry;
use crate::widget::instance::{SharedInstance, WidgetInstanceRuntime};
use crate::widget::lifecycle::LifecycleController;
use crate::widget::snapshot::WidgetSnapshot;
use crate::widget::WidgetContext;
use crate::widget::WidgetSnapshotCache;
use snapshot_eq::payload_renders_equal;

pub use operations::CreateWidgetRequest;

/// Knobs for the background sweepers.
#[derive(Debug, Clone)]
pub struct WidgetManagerOptions {
    /// How long a widget may remain `Sleeping` before being unloaded.
    ///
    /// Active ↔ Sleeping is owned by visibility ([`crate::visibility`] /
    /// [`WidgetManager::apply_visibility`]); this option only gates
    /// Sleeping → Unloaded memory reclaim.
    pub idle_unload_after: Duration,
    /// How often the unload sweeper runs.
    pub sweeper_interval: Duration,
}

impl Default for WidgetManagerOptions {
    fn default() -> Self {
        Self {
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
    locale: Arc<orchid_i18n::LocaleManager>,
    jobs: Arc<orchid_core::BackgroundJobQueue>,
    instances: DashMap<Uuid, SharedInstance>,
    lifecycle: LifecycleController,
    options: RwLock<WidgetManagerOptions>,
    /// Idle lifecycle sweeper (Sleeping → Unloaded only).
    sweeper: Mutex<Option<JoinHandle<()>>>,
    /// Pumps [`Widget::snapshot`] for active instances into [`WidgetSnapshotCache`].
    snapshot_pump: Mutex<Option<JoinHandle<()>>>,
    /// Latest snapshots for UI consumption (updated off the main thread).
    pub snapshot_cache: Arc<WidgetSnapshotCache>,
    /// Instance ids with a new snapshot; drained on the UI thread to refresh those rows only.
    frame_dirty: parking_lot::Mutex<HashSet<Uuid>>,
    /// Instances that need a snapshot rebuild on the next pump tick (event-driven).
    /// Replaces polling every Active widget at 30 Hz.
    snapshot_dirty: parking_lot::Mutex<HashSet<Uuid>>,
    /// Unsubscribed in [`WidgetManager::shutdown`] so disk handles are not kept alive by the bus.
    snapshot_refresh_sub: parking_lot::Mutex<Option<SubscriptionHandle>>,
    /// Instances whose grid/float layout changed and need a batched persist.
    layout_dirty: parking_lot::Mutex<HashSet<Uuid>>,
    /// Debounced flusher for [`layout_dirty`]; aborted on shutdown.
    layout_flush: parking_lot::Mutex<Option<JoinHandle<()>>>,
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
        locale: Arc<orchid_i18n::LocaleManager>,
        jobs: Arc<orchid_core::BackgroundJobQueue>,
        options: WidgetManagerOptions,
    ) -> Self {
        let lifecycle = LifecycleController::new(bus.clone());
        let snapshot_cache = Arc::new(WidgetSnapshotCache::new());
        Self {
            inner: Arc::new(WidgetManagerInner {
                registry,
                bus,
                storage,
                config,
                locale,
                jobs,
                instances: DashMap::new(),
                lifecycle,
                options: RwLock::new(options),
                sweeper: Mutex::new(None),
                snapshot_pump: Mutex::new(None),
                snapshot_cache,
                frame_dirty: parking_lot::Mutex::new(HashSet::new()),
                snapshot_dirty: parking_lot::Mutex::new(HashSet::new()),
                snapshot_refresh_sub: parking_lot::Mutex::new(None),
                layout_dirty: parking_lot::Mutex::new(HashSet::new()),
                layout_flush: parking_lot::Mutex::new(None),
            }),
        }
    }

    /// Mark `instance_id` so the snapshot pump rebuilds it on the next tick.
    ///
    /// Prefer publishing [`WidgetSnapshotUpdated`] when the widget already knows
    /// its payload changed; use this for subsystems (e.g. terminal PTY output)
    /// that update shared state without going through the widget event path.
    pub fn mark_snapshot_dirty(&self, instance_id: Uuid) {
        self.inner.snapshot_dirty.lock().insert(instance_id);
    }

    /// Always-on job queue shared with widget contexts.
    #[must_use]
    pub fn jobs(&self) -> &Arc<orchid_core::BackgroundJobQueue> {
        &self.inner.jobs
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

    /// Lock-free read path for the UI thread (filled by the snapshot pump).
    #[must_use]
    pub fn snapshot_cache(&self) -> &Arc<WidgetSnapshotCache> {
        &self.inner.snapshot_cache
    }

    /// Clears and returns which instances have new snapshot data since last drain.
    /// Used to patch a subset of `WidgetFrameModel` rows instead of a full layout rebuild.
    pub fn drain_frame_dirty_ids(&self) -> Vec<Uuid> {
        let mut g = self.inner.frame_dirty.lock();
        if g.is_empty() {
            return Vec::new();
        }
        mem::take(&mut *g).into_iter().collect()
    }

    /// Push one widget's current [`Widget::snapshot`] into [`WidgetSnapshotCache`]
    /// and mark its frame dirty when the rendered payload changed.
    ///
    /// Used by the UI after search input or [`crate::events::WidgetSnapshotUpdated`]
    /// so workspace rebuilds read fresh data without waiting for the periodic pump.
    ///
    /// # Errors
    ///
    /// [`WidgetError::InstanceNotFound`] when `instance_id` is unknown.
    pub async fn refresh_snapshot_cache(&self, instance_id: Uuid) -> Result<()> {
        let inst = self.get_instance(instance_id)?;
        let snapshot = {
            let guard = inst.widget.lock().await;
            guard.snapshot()
        };
        let Some(snap) = snapshot else {
            return Ok(());
        };
        // Callers of refresh_snapshot_cache (WidgetSnapshotUpdated, UI actions)
        // already decided the widget needs a paint. Do not drop the update when
        // coarse render-equality (rounded labels / bucketed %) matches the cache
        // — that froze System/Processes while samples kept changing underneath.
        self.store_snapshot(inst.id, snap, true);
        Ok(())
    }

    /// Write the current widget snapshot into the cache without marking the
    /// frame dirty.
    ///
    /// File-manager selection patches the Slint model directly, then needs the
    /// cache to catch up so a later layout rebuild does not resurrect an old
    /// highlight. Forcing dirty here would schedule a snapshot patch on the
    /// next tick and race the live rubber band.
    pub async fn sync_snapshot_cache(&self, instance_id: Uuid) -> Result<()> {
        let inst = self.get_instance(instance_id)?;
        let snapshot = {
            let guard = inst.widget.lock().await;
            guard.snapshot()
        };
        let Some(snap) = snapshot else {
            return Ok(());
        };
        self.inner.snapshot_cache.put(inst.id, snap);
        Ok(())
    }

    /// Fill [`WidgetSnapshotCache`] for every known instance (e.g. after restore).
    pub async fn prime_snapshot_caches(&self) -> Result<()> {
        let ids: Vec<Uuid> = self.inner.instances.iter().map(|e| *e.key()).collect();
        for id in ids {
            if let Err(e) = self.refresh_snapshot_cache(id).await {
                warn!(widget_id = %id, error = %e, "prime snapshot cache failed");
            }
        }
        Ok(())
    }

    fn store_snapshot(&self, id: Uuid, snap: WidgetSnapshot, force_dirty: bool) {
        let prev = self.inner.snapshot_cache.get(id);
        let changed = force_dirty
            || prev
                .as_deref()
                .is_none_or(|p| !snapshot_renders_unchanged(p, &snap));
        // Match the snapshot pump: skip put when render-identical to avoid
        // allocating a fresh Arc on every event-driven refresh.
        if changed {
            self.inner.snapshot_cache.put(id, snap);
            self.inner.frame_dirty.lock().insert(id);
        }
    }

    /// Build a [`WidgetContext`] for the given instance.
    pub(crate) fn context_for(&self, instance: &WidgetInstanceRuntime) -> WidgetContext {
        WidgetContext {
            bus: self.inner.bus.clone(),
            storage: self.inner.storage.clone(),
            config: self.inner.config.clone(),
            locale: self.inner.locale.clone(),
            jobs: self.inner.jobs.clone(),
            instance_id: instance.id,
            workspace_id: instance.workspace_id,
        }
    }

    /// Align every instance's lifecycle with canvas visibility.
    ///
    /// * ids in `visible` that are `Sleeping` or `Unloaded` → `Active` (+ touch)
    /// * ids not in `visible` that are `Active` → `Sleeping`
    ///
    /// Errors are logged per-instance; this never fails the whole sync.
    pub async fn apply_visibility(&self, visible: &[Uuid]) {
        let visible: HashSet<Uuid> = visible.iter().copied().collect();
        let all: Vec<SharedInstance> = self.list_instances();
        for inst in all {
            let id = inst.id;
            let state = *inst.lifecycle.read();
            if visible.contains(&id) {
                self.touch(id);
                if state == LifecycleState::Sleeping || state == LifecycleState::Unloaded {
                    if let Err(e) = self.change_lifecycle(id, LifecycleState::Active).await {
                        warn!(widget_id = %id, error = %e, "apply_visibility: wake failed");
                    } else {
                        // Wake path does not always publish WidgetSnapshotUpdated.
                        self.mark_snapshot_dirty(id);
                    }
                }
            } else if state == LifecycleState::Active {
                if let Err(e) = self.change_lifecycle(id, LifecycleState::Sleeping).await {
                    warn!(widget_id = %id, error = %e, "apply_visibility: sleep failed");
                }
            }
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
        self.inner.layout_dirty.lock().clear();
        let mut rows = Vec::with_capacity(self.inner.instances.len());
        for entry in self.inner.instances.iter() {
            let instance = Arc::clone(entry.value());
            let bytes = {
                let widget = instance.widget.lock().await;
                widget.save_state().unwrap_or_default()
            };
            *instance.last_config.write() = bytes.clone();
            rows.push(instance.to_storage(bytes));
        }
        persistence::save_instances_batch(&self.inner.storage, &rows)
    }

    /// Queue a layout-only persist (position / size / placement) using the
    /// cached config. Coalesces into one redb txn after 200 ms.
    pub(crate) fn queue_layout_save(&self, id: Uuid) {
        self.inner.layout_dirty.lock().insert(id);
        self.ensure_layout_flusher();
    }

    fn ensure_layout_flusher(&self) {
        let mut slot = self.inner.layout_flush.lock();
        if slot.is_some() {
            return;
        }
        let inner = Arc::clone(&self.inner);
        *slot = Some(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let wm = WidgetManager {
                inner: Arc::clone(&inner),
            };
            wm.flush_layout_saves();
            *inner.layout_flush.lock() = None;
            if !inner.layout_dirty.lock().is_empty() {
                WidgetManager { inner }.ensure_layout_flusher();
            }
        }));
    }

    fn flush_layout_saves(&self) {
        let ids: Vec<Uuid> = mem::take(&mut *self.inner.layout_dirty.lock())
            .into_iter()
            .collect();
        if ids.is_empty() {
            return;
        }
        let mut rows = Vec::with_capacity(ids.len());
        for id in ids {
            let Some(inst) = self.inner.instances.get(&id).map(|e| Arc::clone(e.value())) else {
                continue;
            };
            let config = inst.last_config.read().clone();
            rows.push(inst.to_storage(config));
        }
        if let Err(e) = persistence::save_instances_batch(&self.inner.storage, &rows) {
            warn!(error = %e, "batched layout persist failed");
            for row in &rows {
                self.inner.layout_dirty.lock().insert(row.id);
            }
        }
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
                locale: self.inner.locale.clone(),
                jobs: self.inner.jobs.clone(),
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
            let canonical_type =
                crate::registry::WidgetRegistry::canonical_type_id(&row.widget_type).to_string();
            // Defer Active until apply_visibility so heavy on_activate work
            // (PTY spawn, FM listing, process scan) only runs for visible widgets.
            let restore_lifecycle = match row.lifecycle {
                LifecycleState::Unloaded => LifecycleState::Unloaded,
                _ => LifecycleState::Sleeping,
            };
            let runtime = Arc::new(WidgetInstanceRuntime {
                id: row.id,
                workspace_id: row.workspace_id,
                type_id: canonical_type.clone(),
                position: RwLock::new(row.position),
                size: RwLock::new(row.size),
                lifecycle: RwLock::new(restore_lifecycle),
                placement: RwLock::new(row.placement.clone()),
                group_id: RwLock::new(None),
                created_at: row.created_at,
                updated_at: RwLock::new(row.updated_at),
                widget: Mutex::new(widget),
                last_snapshot: RwLock::new(None),
                last_touched: RwLock::new(now),
                last_config: RwLock::new(row.config.clone()),
            });

            // Give the widget a chance to warm up. Release the lock between
            // `on_create` and `on_activate` so the snapshot pump can run.
            {
                let ctx = self.context_for(&runtime);
                let mut w = runtime.widget.lock().await;
                if let Err(e) = w.on_create(&ctx).await {
                    warn!(widget_id = %row.id, error = %e, "restore: on_create failed");
                    continue;
                }
            }
            // Migrate legacy viewer float rects from widget config → placement.
            if matches!(
                *runtime.placement.read(),
                orchid_storage::WindowPlacement::Grid
            ) && canonical_type == crate::builtin::viewer::TYPE_ID
            {
                if let Some(bounds) = crate::builtin::viewer::floating_bounds(row.id) {
                    *runtime.placement.write() = orchid_storage::WindowPlacement::floating_normal(
                        crate::widget::instance::pixel_rect_from_bounds(bounds),
                    );
                    let _ = crate::builtin::viewer::set_floating_bounds(row.id, None);
                    let bytes = {
                        let w = runtime.widget.lock().await;
                        w.save_state().unwrap_or_default()
                    };
                    let _ = persistence::save_instance(&self.inner.storage, &runtime, Some(bytes));
                }
            }
            // on_activate runs later via apply_visibility for visible widgets.

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

        let inner_pump = Arc::clone(&self.inner);
        let pump = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(33));
            loop {
                interval.tick().await;
                run_snapshot_pump(&inner_pump).await;
            }
        });
        *self.inner.snapshot_pump.lock().await = Some(pump);

        let inner_weak = Arc::downgrade(&self.inner);
        let snap_sub = self.inner.bus.subscribe_async(
            EventFilter::of_type(WidgetSnapshotUpdated::event_type()),
            HandlerPriority::Normal,
            move |env: EventEnvelope| {
                let id = env
                    .downcast_arc::<WidgetSnapshotUpdated>()
                    .map(|e| e.instance_id);
                let inner_weak = inner_weak.clone();
                async move {
                    let Some(id) = id else {
                        return;
                    };
                    let Some(inner) = inner_weak.upgrade() else {
                        return;
                    };
                    let wm = WidgetManager { inner };
                    let _ = wm.refresh_snapshot_cache(id).await;
                }
            },
        )?;
        let mut slot = self.inner.snapshot_refresh_sub.lock();
        if slot.is_some() {
            warn!("widget manager: snapshot_refresh_sub replaced without shutdown");
        }
        *slot = Some(snap_sub);
        Ok(())
    }

    /// Stop the sweeper and close every remaining instance.
    ///
    /// # Errors
    ///
    /// Propagates storage errors encountered while writing the final
    /// snapshot.
    pub async fn shutdown(&self) -> Result<()> {
        if let Some(h) = self.inner.snapshot_refresh_sub.lock().take() {
            drop(h);
        }
        if let Some(h) = self.inner.sweeper.lock().await.take() {
            h.abort();
        }
        if let Some(h) = self.inner.snapshot_pump.lock().await.take() {
            h.abort();
        }
        if let Some(h) = self.inner.layout_flush.lock().take() {
            h.abort();
        }
        // Final snapshot before closing.
        if let Err(e) = self.snapshot_to_storage().await {
            warn!(error = %e, "final snapshot failed during shutdown");
        }
        // Walk every instance and invoke on_close.
        let ids: Vec<Uuid> = self.inner.instances.iter().map(|e| *e.key()).collect();
        for id in ids {
            // Final snapshot already wrote layout + state; only drop runtime
            // hooks here — deleting rows would wipe persisted widgets on the
            // next cold start.
            if let Err(e) = self.close_locked(id, false).await {
                warn!(widget_id = %id, error = %e, "close during shutdown failed");
            }
        }
        Ok(())
    }
}

/// `true` if the new snapshot would render the same as the previous one.
fn snapshot_renders_unchanged(prev: &WidgetSnapshot, new: &WidgetSnapshot) -> bool {
    if prev.title != new.title {
        return false;
    }
    if prev.status != new.status {
        return false;
    }
    payload_renders_equal(&prev.payload, &new.payload)
}

async fn run_snapshot_pump(inner: &WidgetManagerInner) {
    let dirty: Vec<Uuid> = {
        let mut g = inner.snapshot_dirty.lock();
        if g.is_empty() {
            return;
        }
        mem::take(&mut *g).into_iter().collect()
    };
    let mut rebuilt = 0usize;
    for id in dirty {
        let Some(inst) = inner.instances.get(&id).map(|e| Arc::clone(e.value())) else {
            continue;
        };
        if *inst.lifecycle.read() != LifecycleState::Active {
            continue;
        }
        let snapshot = {
            let guard = inst.widget.lock().await;
            guard.snapshot()
        };
        if let Some(snap) = snapshot {
            let prev = inner.snapshot_cache.get(id);
            let changed = prev
                .as_deref()
                .is_none_or(|p| !snapshot_renders_unchanged(p, &snap));
            if changed {
                inner.snapshot_cache.put(id, snap);
                inner.frame_dirty.lock().insert(id);
                rebuilt += 1;
            }
        }
    }
    if rebuilt > 0 {
        trace!(
            target: "orchid_widgets::snapshot_pump",
            rebuilt,
            cached = inner.snapshot_cache.len(),
            "snapshot pump tick"
        );
    }
}

async fn run_sweepers(inner: &WidgetManagerInner) {
    let stale = inner.options.read().idle_unload_after;
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
            locale: inner.locale.clone(),
            jobs: inner.jobs.clone(),
            instance_id: instance.id,
            workspace_id: instance.workspace_id,
        }
    };

    // Active ↔ Sleeping is owned by visibility (apply_visibility). The sweeper
    // only reclaims memory from long-sleeping instances.
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
    /// Close an instance — invokes `on_close` and removes it from the in-memory
    /// map. When `delete_storage_row` is `true`, also deletes the persisted row
    /// (user closed the widget). When `false`, the row is left intact so a prior
    /// [`Self::snapshot_to_storage`] remains visible after process exit.
    pub(crate) async fn close_locked(&self, id: Uuid, delete_storage_row: bool) -> Result<()> {
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
        self.inner.layout_dirty.lock().remove(&id);
        if delete_storage_row {
            persistence::delete_instance(&self.inner.storage, id)?;
        }
        self.inner.bus.publish(
            orchid_core::EventSource::Subsystem("widgets".into()),
            WidgetClosed { instance_id: id },
        );
        self.inner.snapshot_cache.remove(id);
        Ok(())
    }
}
