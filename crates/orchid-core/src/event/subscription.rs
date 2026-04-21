//! Subscription handles and filter types.

use std::sync::{Arc, Weak};

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use uuid::Uuid;

use crate::event::bus::EventBusInner;
use crate::event::event::{EventEnvelope, EventSource};

/// Opaque identifier for a bus subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubscriptionId(pub Uuid);

impl SubscriptionId {
    /// Generate a fresh id.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::SubscriptionId;
    /// let a = SubscriptionId::new();
    /// let b = SubscriptionId::new();
    /// assert_ne!(a, b);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SubscriptionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Custom predicate stored on an [`EventFilter`].
pub type DynPredicate = Arc<dyn Fn(&EventEnvelope) -> bool + Send + Sync>;

/// Filter deciding whether an event should be delivered to a subscription.
///
/// The empty filter created by [`EventFilter::any`] matches every event.
/// Non-empty `types` and `sources` act as allow-lists; `predicate` is a
/// catch-all.
#[derive(Clone, Default)]
pub struct EventFilter {
    /// If non-empty, only these event types are delivered.
    pub types: SmallVec<[&'static str; 4]>,
    /// If non-empty, only events from these sources are delivered.
    pub sources: SmallVec<[EventSource; 2]>,
    /// Custom predicate evaluated last; `None` means "no custom filter".
    pub predicate: Option<DynPredicate>,
}

impl std::fmt::Debug for EventFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventFilter")
            .field("types", &self.types)
            .field("sources", &self.sources)
            .field("predicate", &self.predicate.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

impl EventFilter {
    /// Filter that matches every event.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::EventFilter;
    /// let f = EventFilter::any();
    /// assert!(f.types.is_empty() && f.sources.is_empty());
    /// ```
    #[must_use]
    pub fn any() -> Self {
        Self::default()
    }

    /// Filter matching exactly one event type.
    #[must_use]
    pub fn of_type(event_type: &'static str) -> Self {
        let mut f = Self::default();
        f.types.push(event_type);
        f
    }

    /// Filter matching exactly one source.
    #[must_use]
    pub fn from_source(source: EventSource) -> Self {
        let mut f = Self::default();
        f.sources.push(source);
        f
    }

    /// Filter driven by a custom predicate.
    #[must_use]
    pub fn with_predicate<F>(f: F) -> Self
    where
        F: Fn(&EventEnvelope) -> bool + Send + Sync + 'static,
    {
        Self {
            predicate: Some(Arc::new(f)),
            ..Self::default()
        }
    }

    /// Narrow an existing filter with an additional type.
    #[must_use]
    pub fn add_type(mut self, event_type: &'static str) -> Self {
        self.types.push(event_type);
        self
    }

    /// Narrow an existing filter with an additional source.
    #[must_use]
    pub fn add_source(mut self, source: EventSource) -> Self {
        self.sources.push(source);
        self
    }

    /// Returns `true` if `envelope` passes every active criterion of this
    /// filter.
    #[must_use]
    pub fn matches(&self, envelope: &EventEnvelope) -> bool {
        if !self.types.is_empty() && !self.types.contains(&envelope.event_type) {
            return false;
        }
        if !self.sources.is_empty() && !self.sources.iter().any(|s| s == &envelope.source) {
            return false;
        }
        if let Some(p) = &self.predicate {
            if !p(envelope) {
                return false;
            }
        }
        true
    }
}

/// Handle to a live subscription. Dropping the handle unsubscribes from the
/// bus (best effort) unless [`SubscriptionHandle::leak`] is called first.
#[must_use = "subscribing without retaining the handle has no effect"]
pub struct SubscriptionHandle {
    pub(crate) id: SubscriptionId,
    pub(crate) bus: Weak<EventBusInner>,
    pub(crate) leaked: bool,
}

impl SubscriptionHandle {
    /// Id of the underlying subscription.
    #[must_use]
    pub fn id(&self) -> SubscriptionId {
        self.id
    }

    /// Prevent automatic unsubscription on drop. The subscription lives for
    /// as long as the event bus does.
    pub fn leak(mut self) {
        self.leaked = true;
    }
}

impl std::fmt::Debug for SubscriptionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubscriptionHandle")
            .field("id", &self.id)
            .field("leaked", &self.leaked)
            .finish()
    }
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        if self.leaked {
            return;
        }
        if let Some(inner) = self.bus.upgrade() {
            inner.remove_subscription(self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::event::{AppStarted, EventEnvelope};

    #[test]
    fn any_matches_everything() {
        let env = EventEnvelope::new(EventSource::System, AppStarted { version: "x".into() });
        assert!(EventFilter::any().matches(&env));
    }

    #[test]
    fn of_type_filter() {
        let env = EventEnvelope::new(EventSource::System, AppStarted { version: "x".into() });
        assert!(EventFilter::of_type("app.started").matches(&env));
        assert!(!EventFilter::of_type("fs.moved").matches(&env));
    }

    #[test]
    fn from_source_filter() {
        let env = EventEnvelope::new(EventSource::System, AppStarted { version: "x".into() });
        assert!(EventFilter::from_source(EventSource::System).matches(&env));
        assert!(!EventFilter::from_source(EventSource::User).matches(&env));
    }

    #[test]
    fn predicate_filter() {
        let env = EventEnvelope::new(EventSource::System, AppStarted { version: "1.0".into() });
        let f = EventFilter::with_predicate(|e| {
            e.downcast::<AppStarted>().is_some_and(|a| a.version.starts_with('1'))
        });
        assert!(f.matches(&env));
    }

    #[test]
    fn combined_filter_requires_all_criteria() {
        let env = EventEnvelope::new(EventSource::User, AppStarted { version: "1.0".into() });
        let f = EventFilter::of_type("app.started").add_source(EventSource::User);
        assert!(f.matches(&env));

        let f_sys = EventFilter::of_type("app.started").add_source(EventSource::System);
        assert!(!f_sys.matches(&env));
    }
}
