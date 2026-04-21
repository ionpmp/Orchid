//! Cross-provider file watcher that routes events onto the [`EventBus`].
//!
//! [`EventBus`]: orchid_core::EventBus

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::error::{FsError, Result};
use crate::path::FsPath;
use crate::provider::{FsChangeKind, FsProviderRegistry};
use crate::watcher::events::{
    FsCreatedEvent, FsDeletedEvent, FsModifiedEvent, FsRenamedEvent,
};

/// Watcher handle — unregisters the underlying watch on drop.
pub struct WatchHandle {
    id: Uuid,
    owner: std::sync::Weak<FileWatcherInner>,
}

impl WatchHandle {
    /// Identifier for this watch.
    #[must_use]
    pub fn id(&self) -> Uuid {
        self.id
    }
}

impl std::fmt::Debug for WatchHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchHandle").field("id", &self.id).finish()
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        if let Some(inner) = self.owner.upgrade() {
            if let Some((_, entry)) = inner.watches.remove(&self.id) {
                entry.task.abort();
            }
        }
    }
}

struct ActiveWatch {
    task: JoinHandle<()>,
}

pub(crate) struct FileWatcherInner {
    bus: Arc<orchid_core::EventBus>,
    registry: Arc<FsProviderRegistry>,
    watches: DashMap<Uuid, ActiveWatch>,
    shutdown: AtomicBool,
}

/// Aggregates per-provider watches into a single stream of bus events.
#[derive(Clone)]
pub struct FileWatcher {
    inner: Arc<FileWatcherInner>,
}

impl std::fmt::Debug for FileWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileWatcher")
            .field("active", &self.inner.watches.len())
            .finish_non_exhaustive()
    }
}

impl FileWatcher {
    /// Construct a new watcher tied to an [`EventBus`](orchid_core::EventBus)
    /// and a provider registry.
    #[must_use]
    pub fn new(
        bus: Arc<orchid_core::EventBus>,
        registry: Arc<FsProviderRegistry>,
    ) -> Self {
        Self {
            inner: Arc::new(FileWatcherInner {
                bus,
                registry,
                watches: DashMap::new(),
                shutdown: AtomicBool::new(false),
            }),
        }
    }

    /// Start watching `path`. Drop the returned handle (or let the
    /// `FileWatcher` shut down) to stop.
    ///
    /// # Errors
    ///
    /// * [`FsError::ProviderNotMounted`] if no provider serves the scheme.
    /// * Underlying notify errors.
    pub async fn watch(&self, path: FsPath) -> Result<WatchHandle> {
        if self.inner.shutdown.load(Ordering::SeqCst) {
            return Err(FsError::Cancelled);
        }
        let provider = self
            .inner
            .registry
            .for_path(&path)
            .ok_or_else(|| FsError::ProviderNotMounted(path.to_string()))?;
        let mut handle = provider
            .watch(&path)
            .await?
            .ok_or_else(|| {
                FsError::InvalidPath {
                    reason: format!(
                        "provider for `{}` does not implement a native watcher",
                        path
                    ),
                }
            })?;

        let id = Uuid::new_v4();
        let inner_weak = Arc::downgrade(&self.inner);
        let task = tokio::spawn(async move {
            loop {
                let batch = match handle.recv().await {
                    Some(b) => b,
                    None => break,
                };
                let Some(inner) = inner_weak.upgrade() else {
                    break;
                };
                for change in batch {
                    dispatch_change(&inner.bus, change).await;
                }
            }
            // Best-effort shutdown of the backend once the loop exits.
            handle.shutdown().await;
        });
        self.inner.watches.insert(id, ActiveWatch { task });
        Ok(WatchHandle {
            id,
            owner: Arc::downgrade(&self.inner),
        })
    }

    /// Cancel every active watch and stop accepting new ones.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation.
    pub async fn shutdown(&self) -> Result<()> {
        self.inner.shutdown.store(true, Ordering::SeqCst);
        let ids: Vec<Uuid> = self.inner.watches.iter().map(|e| *e.key()).collect();
        for id in ids {
            if let Some((_, entry)) = self.inner.watches.remove(&id) {
                entry.task.abort();
            }
        }
        Ok(())
    }
}

async fn dispatch_change(bus: &orchid_core::EventBus, change: crate::provider::FsChange) {
    let source = orchid_core::EventSource::Subsystem("fs.watcher".into());
    match change.kind {
        FsChangeKind::Created => {
            bus.publish(
                source,
                FsCreatedEvent {
                    path: change.path,
                    at: change.timestamp,
                },
            );
        }
        FsChangeKind::Modified => {
            bus.publish(
                source,
                FsModifiedEvent {
                    path: change.path,
                    at: change.timestamp,
                },
            );
        }
        FsChangeKind::Deleted => {
            bus.publish(
                source,
                FsDeletedEvent {
                    path: change.path,
                    at: change.timestamp,
                },
            );
        }
        FsChangeKind::Renamed { from } => {
            bus.publish(
                source,
                FsRenamedEvent {
                    from,
                    to: change.path,
                    at: change.timestamp,
                },
            );
        }
    }
}
