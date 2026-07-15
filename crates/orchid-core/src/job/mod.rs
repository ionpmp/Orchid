//! Always-on background job queue.
//!
//! Widgets pause UI work when not visible; work that must keep running
//! (feed fetches, weather, future agents) is scheduled here instead of on
//! per-widget timers tied to lifecycle.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::task::{AbortHandle, JoinHandle};
use tracing::debug;

/// Boxed async job body produced on each tick.
pub type BoxedJobFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Factory invoked on each interval tick (and once immediately on schedule).
pub type JobFactory = Arc<dyn Fn() -> BoxedJobFuture + Send + Sync + 'static>;

struct Inner {
    jobs: Mutex<HashMap<String, AbortHandle>>,
    /// Join handles kept so [`BackgroundJobQueue::shutdown`] can await them.
    handles: Mutex<HashMap<String, JoinHandle<()>>>,
    shut_down: AtomicBool,
}

/// Shared, process-wide queue for interval work that outlives widget visibility.
///
/// Re-scheduling the same key replaces the previous job. Keys should be stable
/// and unique per logical task (e.g. `"rss:{instance_id}"`).
#[derive(Clone)]
pub struct BackgroundJobQueue {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for BackgroundJobQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackgroundJobQueue")
            .field("jobs", &self.inner.jobs.lock().len())
            .finish_non_exhaustive()
    }
}

impl Default for BackgroundJobQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl BackgroundJobQueue {
    /// Create an empty queue (no worker until the first [`Self::schedule`]).
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                jobs: Mutex::new(HashMap::new()),
                handles: Mutex::new(HashMap::new()),
                shut_down: AtomicBool::new(false),
            }),
        }
    }

    /// Schedule `job` to run immediately, then every `interval`.
    ///
    /// If `key` is already scheduled, the previous task is aborted first.
    /// No-op after [`Self::shutdown`].
    pub fn schedule<F, Fut>(&self, key: impl Into<String>, interval: Duration, job: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        if self.inner.shut_down.load(Ordering::SeqCst) {
            return;
        }
        let key = key.into();
        let factory: JobFactory = Arc::new(move || Box::pin(job()));
        self.schedule_factory(key, interval, factory);
    }

    /// Same as [`Self::schedule`] but accepts a pre-boxed factory (handy when
    /// the caller already holds an `Arc` of shared state).
    pub fn schedule_factory(&self, key: String, interval: Duration, factory: JobFactory) {
        if self.inner.shut_down.load(Ordering::SeqCst) {
            return;
        }
        self.cancel(&key);

        let key_for_task = key.clone();
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // First `tick()` completes immediately — same as PeriodicRefresh.
            loop {
                ticker.tick().await;
                factory().await;
            }
        });
        let abort = handle.abort_handle();
        self.inner.jobs.lock().insert(key.clone(), abort);
        self.inner.handles.lock().insert(key.clone(), handle);
        debug!(%key_for_task, ?interval, "background job scheduled");
    }

    /// Abort and forget the job registered under `key`, if any.
    pub fn cancel(&self, key: &str) {
        if let Some(abort) = self.inner.jobs.lock().remove(key) {
            abort.abort();
            debug!(%key, "background job cancelled");
        }
        // Drop the join handle without awaiting — abort is enough for cancel.
        let _ = self.inner.handles.lock().remove(key);
    }

    /// Whether `key` currently has a running schedule.
    #[must_use]
    pub fn is_scheduled(&self, key: &str) -> bool {
        self.inner.jobs.lock().contains_key(key)
    }

    /// Number of currently scheduled keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.jobs.lock().len()
    }

    /// `true` when no jobs are scheduled.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Abort every job and refuse further schedules.
    pub async fn shutdown(&self) {
        self.inner.shut_down.store(true, Ordering::SeqCst);
        let aborts: Vec<_> = {
            let mut jobs = self.inner.jobs.lock();
            jobs.drain().map(|(_, a)| a).collect()
        };
        for a in aborts {
            a.abort();
        }
        let handles: Vec<_> = {
            let mut h = self.inner.handles.lock();
            h.drain().map(|(_, j)| j).collect()
        };
        for h in handles {
            let _ = h.await;
        }
        debug!("background job queue shut down");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn schedule_runs_immediately_and_on_interval() {
        let q = BackgroundJobQueue::new();
        let n = Arc::new(AtomicUsize::new(0));
        let n2 = Arc::clone(&n);
        q.schedule("t", Duration::from_millis(40), move || {
            let n2 = Arc::clone(&n2);
            async move {
                n2.fetch_add(1, Ordering::SeqCst);
            }
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(n.load(Ordering::SeqCst) >= 1);
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(n.load(Ordering::SeqCst) >= 2);
        q.cancel("t");
        let after = n.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(n.load(Ordering::SeqCst), after);
        q.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reschedule_replaces_previous() {
        let q = BackgroundJobQueue::new();
        let a = Arc::new(AtomicUsize::new(0));
        let b = Arc::new(AtomicUsize::new(0));
        let a1 = Arc::clone(&a);
        q.schedule("k", Duration::from_secs(60), move || {
            let a1 = Arc::clone(&a1);
            async move {
                a1.fetch_add(1, Ordering::SeqCst);
            }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(a.load(Ordering::SeqCst), 1);

        let b1 = Arc::clone(&b);
        q.schedule("k", Duration::from_secs(60), move || {
            let b1 = Arc::clone(&b1);
            async move {
                b1.fetch_add(1, Ordering::SeqCst);
            }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(b.load(Ordering::SeqCst), 1);
        // Old job must not keep ticking.
        let a_after = a.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(a.load(Ordering::SeqCst), a_after);
        q.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_rejects_new_schedules() {
        let q = BackgroundJobQueue::new();
        q.shutdown().await;
        let n = Arc::new(AtomicUsize::new(0));
        let n2 = Arc::clone(&n);
        q.schedule("x", Duration::from_millis(10), move || {
            let n2 = Arc::clone(&n2);
            async move {
                n2.fetch_add(1, Ordering::SeqCst);
            }
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(n.load(Ordering::SeqCst), 0);
        assert!(q.is_empty());
    }
}
