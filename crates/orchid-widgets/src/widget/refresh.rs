//! Lifecycle-aware periodic refresh helper used by built-in widgets.
//!
//! Widgets call [`PeriodicRefresh::start`] from `on_activate` and
//! [`PeriodicRefresh::stop`] from `on_sleep` / `on_unload` / `on_close`. The
//! handle is abort-safe: stopping (or dropping) the refresh aborts the
//! spawned task.

use std::future::Future;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::task::JoinHandle;

/// Periodic-refresh driver wrapping a `tokio` interval + spawned task.
pub struct PeriodicRefresh {
    interval: Duration,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for PeriodicRefresh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeriodicRefresh")
            .field("interval", &self.interval)
            .field("running", &self.is_running())
            .finish()
    }
}

impl PeriodicRefresh {
    /// New refresh with the given tick period. No task is spawned until
    /// [`PeriodicRefresh::start`] is called.
    #[must_use]
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            handle: Mutex::new(None),
        }
    }

    /// Current tick period.
    #[must_use]
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Whether a task is currently running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.handle
            .lock()
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }

    /// Spawn a task that calls `tick()` once per [`Self::interval`]. Any
    /// existing task is aborted before a new one starts.
    ///
    /// The first tick fires immediately.
    pub fn start<F, Fut>(&self, mut tick: F)
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.stop();
        let period = self.interval;
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(period);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                tick().await;
            }
        });
        *self.handle.lock() = Some(handle);
    }

    /// Abort the running task, if any.
    pub fn stop(&self) {
        if let Some(h) = self.handle.lock().take() {
            h.abort();
        }
    }
}

impl Drop for PeriodicRefresh {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test(flavor = "multi_thread")]
    async fn tick_fires_and_stop_halts() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let refresh = PeriodicRefresh::new(Duration::from_millis(20));
        refresh.start(move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
            }
        });
        tokio::time::sleep(Duration::from_millis(120)).await;
        refresh.stop();
        let mid = counter.load(Ordering::SeqCst);
        assert!(mid >= 3, "expected at least 3 ticks, got {mid}");
        tokio::time::sleep(Duration::from_millis(60)).await;
        let after = counter.load(Ordering::SeqCst);
        assert!(
            after <= mid + 1,
            "stop did not halt ticks (mid={mid}, after={after})"
        );
    }
}
