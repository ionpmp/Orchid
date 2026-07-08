//! Shared test fixtures for orchid-widgets integration tests.

#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use orchid_widgets::{
    Result, Widget, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetPayload,
    WidgetRegistry, WidgetSnapshot, WidgetStatus,
};
use uuid::Uuid;

/// Default locale manager for widget tests.
pub fn test_locale() -> Arc<orchid_i18n::LocaleManager> {
    Arc::new(
        orchid_i18n::LocaleManager::new(orchid_i18n::default_language(), None)
            .expect("test locale"),
    )
}

/// Dummy widget used in tests. Counts how many times each callback fires so
/// assertions can verify lifecycle transitions actually ran.
pub struct DummyWidget {
    pub instance_id: Uuid,
    pub counters: Arc<DummyCounters>,
    pub payload: String,
}

#[derive(Default)]
pub struct DummyCounters {
    pub on_create: AtomicUsize,
    pub on_activate: AtomicUsize,
    pub on_sleep: AtomicUsize,
    pub on_unload: AtomicUsize,
    pub on_close: AtomicUsize,
    pub on_resize: AtomicUsize,
}

#[async_trait]
impl Widget for DummyWidget {
    fn type_id(&self) -> &'static str {
        "test.dummy"
    }
    fn instance_id(&self) -> Uuid {
        self.instance_id
    }
    async fn on_create(&mut self, _ctx: &WidgetContext) -> Result<()> {
        self.counters.on_create.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn on_activate(&mut self, _ctx: &WidgetContext) -> Result<()> {
        self.counters.on_activate.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> Result<()> {
        self.counters.on_sleep.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn on_unload(&mut self, _ctx: &WidgetContext) -> Result<()> {
        self.counters.on_unload.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn on_close(&mut self, _ctx: &WidgetContext) -> Result<()> {
        self.counters.on_close.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn on_resize(
        &mut self,
        _ctx: &WidgetContext,
        _size: orchid_storage::WidgetSize,
    ) -> Result<()> {
        self.counters.on_resize.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn snapshot(&self) -> Option<WidgetSnapshot> {
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: "test.dummy",
            title: "Dummy".into(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Text {
                lines: vec![self.payload.clone()],
            },
        })
    }
    fn save_state(&self) -> Result<Vec<u8>> {
        Ok(self.payload.clone().into_bytes())
    }
    fn restore_state(&mut self, bytes: &[u8]) -> Result<()> {
        self.payload = String::from_utf8_lossy(bytes).into_owned();
        Ok(())
    }
}

/// Register a dummy widget type with the given registry.
///
/// Returns a shared handle to the counters so the test can inspect them.
pub fn register_dummy(registry: &WidgetRegistry) -> Arc<DummyCounters> {
    let counters = Arc::new(DummyCounters::default());
    let counters_for_factory = counters.clone();
    let factory = Arc::new(move |ctx: WidgetContext, state_bytes: Option<&[u8]>| {
        let mut widget = DummyWidget {
            instance_id: ctx.instance_id,
            counters: counters_for_factory.clone(),
            payload: "initial".into(),
        };
        if let Some(bytes) = state_bytes {
            widget.restore_state(bytes)?;
        }
        Ok(Box::new(widget) as Box<dyn Widget>)
    });
    let descriptor = WidgetDescriptor {
        type_id: "test.dummy",
        display_name_key: "test.dummy.name",
        description_key: "test.dummy.desc",
        icon_name: "dummy",
        category: WidgetCategory::Developer,
        default_size: orchid_storage::WidgetSize::Small,
        min_size: Some(orchid_storage::WidgetSize::Small),
        max_size: None,
        default_lifecycle: orchid_storage::LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    };
    registry.register(descriptor).unwrap();
    counters
}

/// Tempdir-backed `StateStore` + an on-disk path so we can drop one handle
/// and reopen the database.
pub struct DiskStorage {
    pub tmp: tempfile::TempDir,
    pub path: std::path::PathBuf,
}

impl DiskStorage {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.redb");
        Self { tmp, path }
    }
    pub fn open(&self) -> Arc<orchid_storage::StateStore> {
        Arc::new(orchid_storage::StateStore::open(&self.path, "0.0-test").unwrap())
    }
}
