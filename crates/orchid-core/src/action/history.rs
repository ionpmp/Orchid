//! [`ActionMiddleware`] that records every dispatched action to the
//! [`orchid_storage`] history table.
//!
//! Recording can be toggled at runtime via
//! [`HistoryRecorder::set_enabled`] so the `privacy.record_action_history`
//! config flag can flip it without re-wiring the dispatcher.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use bincode::{Decode, Encode};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use crate::action::context::{ActionContext, ActionOutcome};
use crate::action::dispatcher::ActionMiddleware;
use crate::action::reversible::REVERSIBLE_WINDOW_SECONDS;
use crate::action::Action;
use crate::error::Result;
use orchid_storage::{HistoryEntry, StateStore};

/// Metadata serialized into [`HistoryEntry::metadata`] by this middleware.
///
/// Kept as a private internal format; consumers of the history table should
/// not rely on its layout. If you need structured access add proper columns
/// to the storage schema instead.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
struct HistoryMetadata {
    success: bool,
    error_message: Option<String>,
    #[bincode(with_serde)]
    correlation_id: Option<Uuid>,
    source_label: String,
}

/// Middleware that writes every dispatched action to the state database.
pub struct HistoryRecorder {
    storage: Arc<StateStore>,
    enabled: Arc<AtomicBool>,
}

impl std::fmt::Debug for HistoryRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryRecorder")
            .field("enabled", &self.enabled.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl HistoryRecorder {
    /// Build a recorder.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use orchid_core::HistoryRecorder;
    /// use orchid_storage::StateStore;
    ///
    /// let storage = Arc::new(StateStore::open_in_memory("0").unwrap());
    /// let _rec = HistoryRecorder::new(storage, true);
    /// ```
    #[must_use]
    pub fn new(storage: Arc<StateStore>, enabled: bool) -> Self {
        Self {
            storage,
            enabled: Arc::new(AtomicBool::new(enabled)),
        }
    }

    /// Enable / disable recording at runtime. Cheap (single atomic store).
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Is recording currently active?
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl ActionMiddleware for HistoryRecorder {
    async fn before(&self, _: &dyn Action, _: &ActionContext) -> Result<()> {
        Ok(())
    }

    async fn after(
        &self,
        action: &dyn Action,
        ctx: &ActionContext,
        outcome: &Result<ActionOutcome>,
    ) {
        if !self.is_enabled() {
            return;
        }

        let now = Utc::now();
        let reversible_until = if action.is_reversible() {
            Some(now + chrono::Duration::seconds(REVERSIBLE_WINDOW_SECONDS))
        } else {
            None
        };
        let reverse_command = outcome
            .as_ref()
            .ok()
            .and_then(|o| o.reverse_command_text.clone());

        let metadata = HistoryMetadata {
            success: outcome.is_ok() && outcome.as_ref().map(|o| o.success).unwrap_or(false),
            error_message: outcome.as_ref().err().map(|e| e.to_string()),
            correlation_id: ctx.correlation_id,
            source_label: ctx.source.label(),
        };
        let metadata_bytes =
            match bincode::encode_to_vec(&metadata, bincode::config::standard()) {
                Ok(b) => b,
                Err(e) => {
                    warn!(error = %e, "failed to encode history metadata; skipping entry");
                    return;
                }
            };

        let entry = HistoryEntry {
            id: Uuid::new_v4(),
            timestamp: now,
            action_id: action.id().to_string(),
            command_text: action.command_text(),
            target: action.target(),
            reversible_until,
            reverse_command,
            metadata: metadata_bytes,
        };

        let storage = Arc::clone(&self.storage);
        let write_result = (|| {
            let mut w = storage.write()?;
            w.put_history(&entry)?;
            w.commit()?;
            Ok::<_, crate::CoreError>(())
        })();

        if let Err(e) = write_result {
            warn!(error = %e, action.id = action.id(), "history recording failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::event::{EventBus, EventBusConfig};
    use crate::ActionDispatcher;

    fn make_ctx(storage: Arc<StateStore>) -> ActionContext {
        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let config =
            Arc::new(parking_lot::RwLock::new(orchid_storage::OrchidConfig::default()));
        ActionContext::new(bus, storage, config)
    }

    struct Echo;
    #[async_trait]
    impl Action for Echo {
        fn id(&self) -> &'static str {
            "test.echo"
        }
        fn display_name_key(&self) -> &'static str {
            "test.echo.name"
        }
        fn command_text(&self) -> String {
            "orc test echo".into()
        }
        async fn execute(&self, _ctx: &ActionContext) -> Result<ActionOutcome> {
            Ok(ActionOutcome::ok_with_message("hi"))
        }
    }

    struct Failer;
    #[async_trait]
    impl Action for Failer {
        fn id(&self) -> &'static str {
            "test.fail"
        }
        fn display_name_key(&self) -> &'static str {
            "test.fail.name"
        }
        fn command_text(&self) -> String {
            "orc test fail".into()
        }
        async fn execute(&self, _ctx: &ActionContext) -> Result<ActionOutcome> {
            Ok(ActionOutcome::failed("nope"))
        }
    }

    #[tokio::test]
    async fn records_successful_action() {
        let storage = Arc::new(StateStore::open_in_memory("0").unwrap());
        let rec = Arc::new(HistoryRecorder::new(Arc::clone(&storage), true));
        let d = ActionDispatcher::new().with_middleware(rec.clone() as _);
        d.dispatch(Box::new(Echo), &make_ctx(Arc::clone(&storage)))
            .await
            .unwrap();

        let r = storage.read().unwrap();
        let recent = r.iter_history_recent(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].action_id, "test.echo");
        assert_eq!(recent[0].command_text, "orc test echo");

        let meta: HistoryMetadata =
            bincode::decode_from_slice(&recent[0].metadata, bincode::config::standard())
                .unwrap()
                .0;
        assert!(meta.success);
    }

    #[tokio::test]
    async fn records_failed_action_with_success_false() {
        let storage = Arc::new(StateStore::open_in_memory("0").unwrap());
        let rec = Arc::new(HistoryRecorder::new(Arc::clone(&storage), true));
        let d = ActionDispatcher::new().with_middleware(rec.clone() as _);
        d.dispatch(Box::new(Failer), &make_ctx(Arc::clone(&storage)))
            .await
            .unwrap();

        let r = storage.read().unwrap();
        let recent = r.iter_history_recent(10).unwrap();
        assert_eq!(recent.len(), 1);
        let meta: HistoryMetadata =
            bincode::decode_from_slice(&recent[0].metadata, bincode::config::standard())
                .unwrap()
                .0;
        assert!(!meta.success);
    }

    #[tokio::test]
    async fn disabled_recorder_does_not_write() {
        let storage = Arc::new(StateStore::open_in_memory("0").unwrap());
        let rec = Arc::new(HistoryRecorder::new(Arc::clone(&storage), false));
        let d = ActionDispatcher::new().with_middleware(rec.clone() as _);
        d.dispatch(Box::new(Echo), &make_ctx(Arc::clone(&storage)))
            .await
            .unwrap();

        let r = storage.read().unwrap();
        assert!(r.iter_history_recent(10).unwrap().is_empty());

        rec.set_enabled(true);
        d.dispatch(Box::new(Echo), &make_ctx(Arc::clone(&storage)))
            .await
            .unwrap();
        let r = storage.read().unwrap();
        assert_eq!(r.iter_history_recent(10).unwrap().len(), 1);
    }
}
