//! Event trait, envelope, and built-in event types.

use std::any::Any;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Marker trait implemented by every value flowing through [`crate::EventBus`].
///
/// Events must be cheap to clone (so multiple subscribers can each receive
/// their own copy), thread-safe, and carry a stable textual type id for
/// filtering and logging.
///
/// # Examples
///
/// ```
/// use orchid_core::Event;
///
/// #[derive(Debug, Clone)]
/// struct FileSaved { pub path: String }
///
/// impl Event for FileSaved {
///     fn event_type() -> &'static str { "fs.file_saved" }
/// }
/// ```
pub trait Event: Clone + Send + Sync + 'static {
    /// Stable, dotted event identifier. Convention: `"domain.kind"`.
    fn event_type() -> &'static str
    where
        Self: Sized;
}

/// Type-erased wrapper used by the bus to transport arbitrary events.
#[derive(Clone)]
pub struct EventEnvelope {
    /// Unique id assigned at publish time.
    pub id: Uuid,
    /// When the event entered the bus.
    pub timestamp: DateTime<Utc>,
    /// Origin of the event (user, system, subsystem, ...).
    pub source: EventSource,
    /// The `event_type()` string of the carried event.
    pub event_type: &'static str,
    /// Type-erased payload.
    pub payload: Arc<dyn Any + Send + Sync>,
}

impl std::fmt::Debug for EventEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventEnvelope")
            .field("id", &self.id)
            .field("timestamp", &self.timestamp)
            .field("source", &self.source)
            .field("event_type", &self.event_type)
            .finish_non_exhaustive()
    }
}

impl EventEnvelope {
    /// Wrap a concrete event in an envelope.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::{EventEnvelope, EventSource};
    /// use orchid_core::event::AppStarted;
    ///
    /// let env = EventEnvelope::new(EventSource::System, AppStarted { version: "0.1".into() });
    /// assert_eq!(env.event_type, "app.started");
    /// ```
    #[must_use]
    pub fn new<E: Event>(source: EventSource, event: E) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            source,
            event_type: E::event_type(),
            payload: Arc::new(event),
        }
    }

    /// Attempt to borrow the payload as a concrete event type.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::{EventEnvelope, EventSource};
    /// use orchid_core::event::AppStarted;
    ///
    /// let env = EventEnvelope::new(EventSource::System, AppStarted { version: "x".into() });
    /// assert!(env.downcast::<AppStarted>().is_some());
    /// ```
    #[must_use]
    pub fn downcast<E: Event>(&self) -> Option<&E> {
        self.payload.downcast_ref::<E>()
    }

    /// Attempt to clone the payload as a strongly-typed [`Arc`].
    #[must_use]
    pub fn downcast_arc<E: Event>(&self) -> Option<Arc<E>> {
        Arc::clone(&self.payload).downcast::<E>().ok()
    }
}

/// Origin tag attached to every event.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventSource {
    /// User-initiated interaction (touch, mouse, keyboard, pen).
    User,
    /// System / internal origin (timers, file watcher, ...).
    System,
    /// Stable name of a subsystem, e.g. `"fs"`, `"search"`.
    Subsystem(String),
    /// A specific widget instance identified by its id.
    Widget(Uuid),
    /// A command executed from the terminal or command palette.
    Command,
}

impl EventSource {
    /// Short label suitable for structured logging.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::User => "user".into(),
            Self::System => "system".into(),
            Self::Subsystem(name) => format!("subsystem:{name}"),
            Self::Widget(id) => format!("widget:{id}"),
            Self::Command => "command".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in example / lifecycle events
// ---------------------------------------------------------------------------

/// Emitted once when Orchid finishes starting up.
#[derive(Debug, Clone)]
pub struct AppStarted {
    /// Version string of the running Orchid build.
    pub version: String,
}

impl Event for AppStarted {
    fn event_type() -> &'static str {
        "app.started"
    }
}

/// Emitted when Orchid is about to shut down.
#[derive(Debug, Clone)]
pub struct AppShuttingDown;

impl Event for AppShuttingDown {
    fn event_type() -> &'static str {
        "app.shutting_down"
    }
}

/// Emitted after `config.toml` is reloaded and validated.
#[derive(Debug, Clone)]
pub struct ConfigUpdated;

impl Event for ConfigUpdated {
    fn event_type() -> &'static str {
        "app.config_updated"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_roundtrip_downcast() {
        let env = EventEnvelope::new(
            EventSource::System,
            AppStarted {
                version: "1.0".into(),
            },
        );
        assert_eq!(env.event_type, "app.started");
        let got = env.downcast::<AppStarted>().expect("downcast");
        assert_eq!(got.version, "1.0");
    }

    #[test]
    fn envelope_downcast_wrong_type_returns_none() {
        let env = EventEnvelope::new(EventSource::System, AppShuttingDown);
        assert!(env.downcast::<AppStarted>().is_none());
    }

    #[test]
    fn source_label_formats() {
        assert_eq!(EventSource::User.label(), "user");
        assert_eq!(EventSource::Subsystem("fs".into()).label(), "subsystem:fs");
    }
}
