//! Agent panel — the shared local conversation.

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use dashmap::DashMap;
use uuid::Uuid;

use crate::agent;
use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::payloads::AgentPayload;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

/// Stable type id.
pub const TYPE_ID: &str = "agent";

static AGENT_LIVE: LazyLock<DashMap<Uuid, Arc<AgentHandle>>> = LazyLock::new(DashMap::new);

struct AgentHandle {
    instance_id: Uuid,
    bus: Arc<orchid_core::EventBus>,
}

impl AgentHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }
}

fn publish_all() {
    for item in AGENT_LIVE.iter() {
        item.publish();
    }
}

/// Snapshot the shared transcript for the settings-free Agent panel.
#[must_use]
pub fn current_payload() -> AgentPayload {
    let view = agent::view();
    AgentPayload {
        lines: view
            .messages
            .iter()
            .filter(|message| !message.content.trim().is_empty() || !message.tool_calls.is_empty())
            .map(|message| crate::widget::payloads::AgentLine {
                role: message.role.clone(),
                text: if message.content.trim().is_empty() {
                    message
                        .tool_calls
                        .iter()
                        .map(|call| call.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    clip_panel(&message.content, 2_000)
                },
            })
            .collect(),
        pending_path: view.pending_path,
        pending_preview: view.pending_preview,
        status: view.status,
    }
}

fn clip_panel(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}

struct AgentWidget {
    instance_id: Uuid,
}

impl AgentWidget {
    fn new(instance_id: Uuid, bus: Arc<orchid_core::EventBus>) -> Self {
        AGENT_LIVE.insert(instance_id, Arc::new(AgentHandle { instance_id, bus }));
        Self { instance_id }
    }
}

#[async_trait]
impl Widget for AgentWidget {
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
        AGENT_LIVE.remove(&self.instance_id);
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: "Agent".into(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Agent(current_payload()),
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
            preferred_size: Some(WidgetSize::Large),
            allows_grouping: true,
            keeps_state_when_unloaded: false,
            has_settings_panel: false,
        }
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    agent::set_turn_hook(publish_all);
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, _state_bytes| {
        Ok(Box::new(AgentWidget::new(ctx.instance_id, ctx.bus.clone())) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-agent-name",
        description_key: "widget-agent-desc",
        icon_name: "search",
        category: WidgetCategory::Productivity,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Small),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}
