//! Execution context passed to every [`crate::Action`].

use std::any::Any;
use std::sync::Arc;

use parking_lot::RwLock;
use uuid::Uuid;

use crate::event::{EventBus, EventSource};

/// Shared state every action sees on dispatch.
///
/// Clone is cheap: every field is either an `Arc` or a small value. The
/// dispatcher clones the context when it has to hand ownership to a spawned
/// task for panic-catching.
#[derive(Clone)]
pub struct ActionContext {
    /// Bus on which the action may publish events.
    pub bus: Arc<EventBus>,
    /// Shared state database.
    pub storage: Arc<orchid_storage::StateStore>,
    /// Currently loaded configuration.
    pub config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    /// Where the action was triggered from, when known.
    pub source: EventSource,
    /// Correlation id that lets multi-step actions show up grouped in
    /// history.
    pub correlation_id: Option<Uuid>,
}

impl std::fmt::Debug for ActionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionContext")
            .field("source", &self.source)
            .field("correlation_id", &self.correlation_id)
            .finish_non_exhaustive()
    }
}

impl ActionContext {
    /// Build a new context with a default source ([`EventSource::Command`])
    /// and no correlation id.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use orchid_core::{ActionContext, EventBus, EventBusConfig};
    /// use orchid_storage::{OrchidConfig, StateStore};
    ///
    /// let bus = Arc::new(EventBus::new(EventBusConfig::default()));
    /// let storage = Arc::new(StateStore::open_in_memory("0").unwrap());
    /// let config = Arc::new(parking_lot::RwLock::new(OrchidConfig::default()));
    /// let _ctx = ActionContext::new(bus, storage, config);
    /// ```
    #[must_use]
    pub fn new(
        bus: Arc<EventBus>,
        storage: Arc<orchid_storage::StateStore>,
        config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    ) -> Self {
        Self {
            bus,
            storage,
            config,
            source: EventSource::Command,
            correlation_id: None,
        }
    }

    /// Return a new context with `source` replaced.
    #[must_use]
    pub fn with_source(mut self, source: EventSource) -> Self {
        self.source = source;
        self
    }

    /// Return a new context with a correlation id attached.
    #[must_use]
    pub fn with_correlation(mut self, id: Uuid) -> Self {
        self.correlation_id = Some(id);
        self
    }
}

/// Result returned from [`crate::Action::execute`].
#[derive(Clone, Default)]
pub struct ActionOutcome {
    /// Whether the action considers itself successful.
    pub success: bool,
    /// Short, human-readable message for logs / UI toasts.
    pub message: Option<String>,
    /// Arbitrary payload the caller may downcast.
    pub data: Option<Arc<dyn Any + Send + Sync>>,
    /// If set, running this command text reverses the forward action.
    pub reverse_command_text: Option<String>,
}

impl std::fmt::Debug for ActionOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionOutcome")
            .field("success", &self.success)
            .field("message", &self.message)
            .field("reverse_command_text", &self.reverse_command_text)
            .finish_non_exhaustive()
    }
}

impl ActionOutcome {
    /// Successful outcome with no message or data.
    #[must_use]
    pub fn ok() -> Self {
        Self {
            success: true,
            ..Self::default()
        }
    }

    /// Successful outcome with a human-readable message.
    #[must_use]
    pub fn ok_with_message(msg: impl Into<String>) -> Self {
        Self {
            success: true,
            message: Some(msg.into()),
            ..Self::default()
        }
    }

    /// Failed outcome with a human-readable message.
    #[must_use]
    pub fn failed(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            message: Some(msg.into()),
            ..Self::default()
        }
    }

    /// Builder-style helper that attaches a reverse command text.
    #[must_use]
    pub fn with_reverse(mut self, command_text: impl Into<String>) -> Self {
        self.reverse_command_text = Some(command_text.into());
        self
    }
}
