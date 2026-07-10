//! Bounded event queue with true drop-oldest / drop-newest policies.
//!
//! Tokio's `mpsc` cannot evict the oldest item from the sender side, so channel
//! subscribers use this queue instead.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tokio::sync::Notify;

use crate::event::event::EventEnvelope;

/// Shared state for one channel subscription.
pub(crate) struct ChannelInner {
    queue: Mutex<VecDeque<EventEnvelope>>,
    capacity: usize,
    closed: AtomicBool,
    notify: Notify,
}

impl ChannelInner {
    pub(crate) fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity.max(1))),
            capacity: capacity.max(1),
            closed: AtomicBool::new(false),
            notify: Notify::new(),
        })
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
}

/// Producer half of a channel subscription. Closing (drop) wakes receivers.
pub(crate) struct ChannelSender {
    inner: Arc<ChannelInner>,
}

impl ChannelSender {
    pub(crate) fn new(inner: Arc<ChannelInner>) -> Self {
        Self { inner }
    }

    /// Push with drop-oldest semantics. Returns `Ok(evicted)` when the new
    /// event is queued (`evicted` is true if an older event was discarded),
    /// or `Err(())` if the channel is closed.
    pub(crate) fn push_drop_oldest(&self, envelope: EventEnvelope) -> Result<bool, ()> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err(());
        }
        let mut q = self.inner.queue.lock();
        let mut evicted = false;
        if q.len() >= self.inner.capacity {
            let _ = q.pop_front();
            evicted = true;
        }
        q.push_back(envelope);
        drop(q);
        self.inner.notify.notify_one();
        Ok(evicted)
    }

    /// Push with drop-newest semantics. Returns `Ok(())` on queue, `Err(Full)`
    /// when the buffer is full (new event discarded), `Err(Closed)` when closed.
    pub(crate) fn try_push_drop_newest(&self, envelope: EventEnvelope) -> Result<(), PushError> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err(PushError::Closed);
        }
        let mut q = self.inner.queue.lock();
        if q.len() >= self.inner.capacity {
            return Err(PushError::Full);
        }
        q.push_back(envelope);
        drop(q);
        self.inner.notify.notify_one();
        Ok(())
    }

    /// Block until space is available or `timeout` elapses.
    pub(crate) fn push_block(
        &self,
        envelope: EventEnvelope,
        timeout: Duration,
    ) -> Result<(), PushError> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.try_push_drop_newest(envelope.clone()) {
                Ok(()) => return Ok(()),
                Err(PushError::Closed) => return Err(PushError::Closed),
                Err(PushError::Full) => {
                    if Instant::now() >= deadline {
                        return Err(PushError::Full);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }
}

impl Drop for ChannelSender {
    fn drop(&mut self) {
        self.inner.close();
    }
}

/// Failure modes for non-drop-oldest pushes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PushError {
    /// Buffer is at capacity.
    Full,
    /// Receiver side is gone / subscription removed.
    Closed,
}

/// Consumer half returned by [`crate::EventBus::subscribe`].
pub struct EventReceiver {
    inner: Arc<ChannelInner>,
}

impl std::fmt::Debug for EventReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventReceiver")
            .field("queued", &self.inner.queue.lock().len())
            .field("closed", &self.inner.closed.load(Ordering::Relaxed))
            .finish()
    }
}

impl EventReceiver {
    pub(crate) fn new(inner: Arc<ChannelInner>) -> Self {
        Self { inner }
    }

    /// Receive the next envelope, or `None` when the subscription is closed
    /// and the queue is empty.
    pub async fn recv(&mut self) -> Option<EventEnvelope> {
        loop {
            {
                let mut q = self.inner.queue.lock();
                if let Some(env) = q.pop_front() {
                    return Some(env);
                }
                if self.inner.closed.load(Ordering::SeqCst) {
                    return None;
                }
            }
            self.inner.notify.notified().await;
        }
    }
}
