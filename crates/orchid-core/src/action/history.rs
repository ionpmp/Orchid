//! [`ActionMiddleware`] that records every dispatched action to the
//! [`orchid_storage`] history table.
//!
//! Recording can be toggled at runtime via
//! [`HistoryRecorder::set_enabled`] so the `privacy.record_action_history`
//! config flag can flip it without re-wiring the dispatcher.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bincode_reloaded::{Decode, Encode};
use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

/// Flush once this many entries are buffered.
const FLUSH_MAX_ENTRIES: usize = 32;
/// Flush when the oldest buffered entry is at least this old.
const FLUSH_MAX_AGE: Duration = Duration::from_secs(2);

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
    #[bincode_reloaded(with_serde)]
    correlation_id: Option<Uuid>,
    source_label: String,
}

/// Entries waiting to be written, plus when the oldest one was buffered.
#[derive(Default)]
struct PendingHistory {
    entries: Vec<HistoryEntry>,
    oldest_at: Option<Instant>,
}

/// Middleware that writes every dispatched action to the state database.
///
/// Writes are batched: one redb transaction (and therefore one fsync) covers
/// up to [`FLUSH_MAX_ENTRIES`] actions or [`FLUSH_MAX_AGE`] of activity,
/// instead of one per dispatched action. Call [`HistoryRecorder::flush`]
/// before shutting down so the tail of the buffer reaches disk.
pub struct HistoryRecorder {
    storage: Arc<StateStore>,
    enabled: Arc<AtomicBool>,
    pending: Arc<Mutex<PendingHistory>>,
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
            pending: Arc::new(Mutex::new(PendingHistory::default())),
        }
    }

    /// Enable / disable recording at runtime. Cheap (single atomic store).
    ///
    /// Turning recording off leaves already-buffered entries queued; they are
    /// written by the next [`Self::flush`].
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Is recording currently active?
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Write every buffered entry in one transaction.
    ///
    /// Safe to call when nothing is pending. Call this on shutdown, otherwise
    /// the last few actions of a session are lost.
    pub async fn flush(&self) {
        let entries = self.take_pending();
        if entries.is_empty() {
            return;
        }
        let storage = Arc::clone(&self.storage);
        let count = entries.len();
        let join = tokio::task::spawn_blocking(move || Self::write_batch(&storage, &entries));
        match join.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => warn!(error = %e, count, "history flush failed"),
            Err(e) => warn!(error = %e, count, "history flush join failed"),
        }
    }

    fn take_pending(&self) -> Vec<HistoryEntry> {
        let mut pending = self.pending.lock();
        pending.oldest_at = None;
        std::mem::take(&mut pending.entries)
    }

    /// Queue `entry`, returning `true` when the batch is due to be written.
    fn push_pending(&self, entry: HistoryEntry) -> bool {
        let mut pending = self.pending.lock();
        pending.entries.push(entry);
        let oldest = *pending.oldest_at.get_or_insert_with(Instant::now);
        pending.entries.len() >= FLUSH_MAX_ENTRIES || oldest.elapsed() >= FLUSH_MAX_AGE
    }

    fn write_batch(storage: &StateStore, entries: &[HistoryEntry]) -> Result<()> {
        let mut w = storage.write()?;
        for entry in entries {
            w.put_history(entry)?;
        }
        w.commit()?;
        Ok(())
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
        let metadata_bytes = match bincode_reloaded::encode_to_vec(
            &metadata,
            bincode_reloaded::config::standard(),
        ) {
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

        // Buffer instead of committing per action: a redb commit is an fsync,
        // and dispatch latency used to include it.
        if self.push_pending(entry) {
            self.flush().await;
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
        let config = Arc::new(parking_lot::RwLock::new(
            orchid_storage::OrchidConfig::default(),
        ));
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
        rec.flush().await;

        let r = storage.read().unwrap();
        let recent = r.iter_history_recent(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].action_id, "test.echo");
        assert_eq!(recent[0].command_text, "orc test echo");

        let meta: HistoryMetadata = bincode_reloaded::decode_from_slice(
            &recent[0].metadata,
            bincode_reloaded::config::standard(),
        )
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
        rec.flush().await;

        let r = storage.read().unwrap();
        let recent = r.iter_history_recent(10).unwrap();
        assert_eq!(recent.len(), 1);
        let meta: HistoryMetadata = bincode_reloaded::decode_from_slice(
            &recent[0].metadata,
            bincode_reloaded::config::standard(),
        )
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

        rec.flush().await;
        let r = storage.read().unwrap();
        assert!(r.iter_history_recent(10).unwrap().is_empty());

        rec.set_enabled(true);
        d.dispatch(Box::new(Echo), &make_ctx(Arc::clone(&storage)))
            .await
            .unwrap();
        rec.flush().await;
        let r = storage.read().unwrap();
        assert_eq!(r.iter_history_recent(10).unwrap().len(), 1);
    }

    /// Asserts *whether* rows exist, not how many: `history_by_timestamp` is
    /// keyed by unix millis, so entries recorded inside the same millisecond
    /// share one index row and `iter_history_recent` under-reports.
    #[tokio::test]
    async fn entries_are_batched_until_threshold() {
        let storage = Arc::new(StateStore::open_in_memory("0").unwrap());
        let rec = Arc::new(HistoryRecorder::new(Arc::clone(&storage), true));
        let d = ActionDispatcher::new().with_middleware(rec.clone() as _);
        let ctx = make_ctx(Arc::clone(&storage));

        // Well under FLUSH_MAX_ENTRIES and inside FLUSH_MAX_AGE: nothing written.
        for _ in 0..FLUSH_MAX_ENTRIES - 1 {
            d.dispatch(Box::new(Echo), &ctx).await.unwrap();
        }
        assert!(
            storage
                .read()
                .unwrap()
                .iter_history_recent(64)
                .unwrap()
                .is_empty(),
            "entries below the threshold must stay buffered"
        );

        d.dispatch(Box::new(Echo), &ctx).await.unwrap();
        assert!(
            !storage
                .read()
                .unwrap()
                .iter_history_recent(64)
                .unwrap()
                .is_empty(),
            "reaching the entry threshold must write the batch"
        );
    }
}
