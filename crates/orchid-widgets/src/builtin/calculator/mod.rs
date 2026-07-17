//! Quick Calculator — standard + scientific modes with history and memory.

pub mod config;
pub mod engine;
pub mod expr;

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{CalcHistoryRow, CalculatorPayload};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::CalculatorConfig;
pub use engine::{AngleMode, CalcError, CalcKey, CalcMode, Calculator, HistoryEntry};
pub use expr::{evaluate_expression, format_result};

/// Stable type id.
pub const TYPE_ID: &str = "calculator";

static CALC_LIVE: LazyLock<DashMap<Uuid, Arc<CalcHandle>>> = LazyLock::new(DashMap::new);

struct CalcHandle {
    instance_id: Uuid,
    config: Arc<RwLock<CalculatorConfig>>,
    engine: Arc<RwLock<Calculator>>,
    bus: Arc<orchid_core::EventBus>,
}

impl CalcHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn sync_config_from_engine(&self) {
        let eng = self.engine.read();
        let mut cfg = self.config.write();
        cfg.mode = eng.mode as u8;
        cfg.angle_mode = eng.angle as u8;
        cfg.memory = eng.memory;
        cfg.memory_set = eng.memory_set;
        cfg.history = eng.history.iter().map(Into::into).collect();
    }
}

/// Snapshot the live config for the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<CalculatorConfig> {
    CALC_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Apply a settings-dialog mutation to the live config.
pub fn update_config(instance_id: Uuid, mutate: impl FnOnce(&mut CalculatorConfig)) {
    let Some(h) = CALC_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        mutate(&mut cfg);
        let mut eng = h.engine.write();
        eng.mode = cfg.calc_mode();
        eng.angle = cfg.angle();
    }
    h.publish();
}

/// Press a calculator button by stable id (`"add"`, `"sin"`, …).
pub fn press_id(instance_id: Uuid, id: &str) {
    let Some(h) = CALC_LIVE.get(&instance_id) else {
        return;
    };
    let Some(key) = Calculator::key_from_id(id) else {
        return;
    };
    h.engine.write().press(key);
    h.sync_config_from_engine();
    h.publish();
}

/// Press a key from keyboard text.
pub fn press_text(instance_id: Uuid, text: &str, ctrl: bool, shift: bool) {
    let Some(h) = CALC_LIVE.get(&instance_id) else {
        return;
    };
    let Some(key) = Calculator::key_from_text(text, ctrl, shift) else {
        return;
    };
    h.engine.write().press(key);
    h.sync_config_from_engine();
    h.publish();
}

/// Recall a history entry by index.
pub fn recall_history(instance_id: Uuid, index: i32) {
    if index < 0 {
        return;
    }
    let Some(h) = CALC_LIVE.get(&instance_id) else {
        return;
    };
    h.engine
        .write()
        .press(CalcKey::HistoryRecall(index as usize));
    h.sync_config_from_engine();
    h.publish();
}

/// Current display string (for clipboard copy).
#[must_use]
pub fn current_display(instance_id: Uuid) -> Option<String> {
    CALC_LIVE.get(&instance_id).map(|h| {
        let eng = h.engine.read();
        if eng.error.is_some() {
            String::new()
        } else {
            eng.display.clone()
        }
    })
}

/// Calculator widget implementation.
pub struct CalculatorWidget {
    instance_id: Uuid,
    handle: Arc<CalcHandle>,
}

impl std::fmt::Debug for CalculatorWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalculatorWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl CalculatorWidget {
    /// Construct with config and optional restored engine state (display only via config).
    pub fn new(
        instance_id: Uuid,
        config: CalculatorConfig,
        bus: Arc<orchid_core::EventBus>,
    ) -> Self {
        let mut engine = Calculator::new();
        engine.mode = config.calc_mode();
        engine.angle = config.angle();
        engine.memory = config.memory;
        engine.memory_set = config.memory_set;
        engine.history = config.history.iter().cloned().map(Into::into).collect();
        let handle = Arc::new(CalcHandle {
            instance_id,
            config: Arc::new(RwLock::new(config)),
            engine: Arc::new(RwLock::new(engine)),
            bus,
        });
        CALC_LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
        }
    }

    fn build_payload(&self) -> CalculatorPayload {
        let eng = self.handle.engine.read();
        let cfg = self.handle.config.read();
        CalculatorPayload {
            mode: eng.mode.as_index(),
            angle: eng.angle.as_index(),
            second: eng.second,
            display: eng.display.clone(),
            expression: eng.expression.clone(),
            memory_set: eng.memory_set,
            error_key: eng.error.map(CalcError::i18n_key),
            history: eng.history.iter().map(CalcHistoryRow::from).collect(),
            show_history: cfg.show_history,
        }
    }
}

#[async_trait]
impl Widget for CalculatorWidget {
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
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        CALC_LIVE.remove(&self.instance_id);
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
            title: String::new(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Calculator(payload),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let cfg: CalculatorConfig = state_codec::restore_state(bytes).unwrap_or_default();
        {
            let mut slot = self.handle.config.write();
            *slot = cfg.clone();
            let mut eng = self.handle.engine.write();
            eng.mode = cfg.calc_mode();
            eng.angle = cfg.angle();
            eng.memory = cfg.memory;
            eng.memory_set = cfg.memory_set;
            eng.history = cfg.history.into_iter().map(Into::into).collect();
        }
        Ok(())
    }

    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Small),
            max_size: None,
            preferred_size: Some(WidgetSize::Medium),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: true,
        }
    }
}

impl From<&HistoryEntry> for CalcHistoryRow {
    fn from(h: &HistoryEntry) -> Self {
        Self {
            expression: h.expression.clone(),
            result: h.result.clone(),
        }
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let cfg = match state_bytes {
            Some(bytes) => {
                state_codec::restore_state::<CalculatorConfig>(bytes).unwrap_or_default()
            }
            None => CalculatorConfig::default(),
        };
        Ok(Box::new(CalculatorWidget::new(
            ctx.instance_id,
            cfg,
            ctx.bus.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-calculator-name",
        description_key: "widget-calculator-desc",
        icon_name: "calculator",
        category: WidgetCategory::Productivity,
        default_size: WidgetSize::Medium,
        min_size: Some(WidgetSize::Small),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}
