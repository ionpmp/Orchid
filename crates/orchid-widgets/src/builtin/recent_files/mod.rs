//! Recent files widget — shows the application-wide MRU list.

use std::sync::Arc;

use async_trait::async_trait;
use orchid_core::{Event, EventFilter, HandlerPriority, SubscriptionHandle};
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::recent_files::{RecentFilesStore, RecentFilesUpdated};
use crate::widget::payloads::RecentFilesPayload;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory};
use orchid_storage::{LifecycleState, WidgetSize};

/// Stable type id.
pub const TYPE_ID: &str = "recent-files";

/// Recent-files widget.
pub struct RecentFilesWidget {
    instance_id: Uuid,
    store: Arc<RecentFilesStore>,
    bus: Arc<orchid_core::EventBus>,
    locale: Arc<orchid_i18n::LocaleManager>,
    max_items: u32,
    _recent_sub: Option<SubscriptionHandle>,
}

impl std::fmt::Debug for RecentFilesWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecentFilesWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl RecentFilesWidget {
    fn new(
        instance_id: Uuid,
        store: Arc<RecentFilesStore>,
        bus: Arc<orchid_core::EventBus>,
        locale: Arc<orchid_i18n::LocaleManager>,
    ) -> Self {
        Self {
            instance_id,
            store,
            bus,
            locale,
            max_items: 20,
            _recent_sub: None,
        }
    }

    fn build_payload(&self) -> RecentFilesPayload {
        let limit = self.max_items.max(1) as usize;
        RecentFilesPayload::from_entries(&self.store.list(limit), &self.locale)
    }
}

#[async_trait]
impl Widget for RecentFilesWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }
    fn instance_id(&self) -> Uuid {
        self.instance_id
    }
    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }
    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        let bus = self.bus.clone();
        let instance_id = self.instance_id;
        let bus_cb = bus.clone();
        let sub = bus
            .subscribe_async(
                EventFilter::of_type(RecentFilesUpdated::event_type()),
                HandlerPriority::Normal,
                move |_env| {
                    let bus = bus_cb.clone();
                    async move {
                        bus.publish(
                            orchid_core::EventSource::Widget(instance_id),
                            WidgetSnapshotUpdated { instance_id },
                        );
                    }
                },
            )
            .map_err(|e| crate::error::WidgetError::CreationFailed(format!("recent sub: {e}")))?;
        self._recent_sub = Some(sub);
        Ok(())
    }
    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self._recent_sub = None;
        Ok(())
    }
    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self._recent_sub = None;
        Ok(())
    }
    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self._recent_sub = None;
        Ok(())
    }
    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }
    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let payload = self.build_payload();
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: self.locale.tr("widget-recent-files-name").into(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::RecentFiles(payload),
        })
    }
    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        Ok(Vec::new())
    }
    fn restore_state(&mut self, _bytes: &[u8]) -> WidgetResult<()> {
        Ok(())
    }
    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Small),
            max_size: None,
            preferred_size: Some(WidgetSize::Medium),
            allows_grouping: true,
            keeps_state_when_unloaded: false,
            has_settings_panel: false,
        }
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor(store: Arc<RecentFilesStore>) -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, _state_bytes| {
        Ok(Box::new(RecentFilesWidget::new(
            ctx.instance_id,
            store.clone(),
            ctx.bus.clone(),
            ctx.locale.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-recent-files-name",
        description_key: "widget-recent-files-desc",
        icon_name: "recent-files",
        category: WidgetCategory::Productivity,
        default_size: WidgetSize::Medium,
        min_size: Some(WidgetSize::Small),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: false,
        factory,
    }
}
