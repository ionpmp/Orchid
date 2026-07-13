//! Batched, async index scheduler that drains tasks into a [`SearchEngine`].
//!
//! A single worker owns the receive side of the channel. Extra concurrency
//! would not help: [`SearchEngine`]'s Tantivy `IndexWriter` is already
//! serialised behind a mutex.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::engine::{IndexDocument, SearchEngine};
use crate::error::{Result, SearchError};

/// Work units consumed by the scheduler.
#[derive(Debug, Clone)]
pub enum IndexTask {
    /// Add or replace a document.
    Upsert(IndexDocument),
    /// Remove a document by path.
    Remove(String),
    /// Flush pending work to disk.
    Flush,
}

/// Background scheduler that batches upserts.
#[derive(Clone)]
pub struct IndexScheduler {
    tx: mpsc::UnboundedSender<IndexTask>,
    worker: Arc<parking_lot::Mutex<Option<JoinHandle<()>>>>,
}

impl std::fmt::Debug for IndexScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexScheduler").finish_non_exhaustive()
    }
}

const BATCH_MAX: usize = 64;

impl IndexScheduler {
    /// Spawn a single background worker that drains the task queue.
    ///
    /// `concurrency` is retained for API compatibility but ignored: indexing
    /// is single-threaded because the Tantivy writer is serialised.
    #[must_use]
    pub fn new(engine: Arc<SearchEngine>, _concurrency: usize) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let worker = tokio::spawn(async move { worker_loop(engine, rx).await });
        Self {
            tx,
            worker: Arc::new(parking_lot::Mutex::new(Some(worker))),
        }
    }

    /// Enqueue an upsert.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError::IndexClosed`] if the scheduler has been shut
    /// down.
    pub async fn enqueue_upsert(&self, doc: IndexDocument) -> Result<()> {
        self.tx
            .send(IndexTask::Upsert(doc))
            .map_err(|_| SearchError::IndexClosed)
    }

    /// Enqueue a removal.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError::IndexClosed`] if the scheduler is shut down.
    pub async fn enqueue_remove(&self, path: String) -> Result<()> {
        self.tx
            .send(IndexTask::Remove(path))
            .map_err(|_| SearchError::IndexClosed)
    }

    /// Ask the scheduler to commit after draining pending work.
    ///
    /// Upsert batches and removes already commit automatically so live FS
    /// indexing is searchable without an explicit flush; this remains useful
    /// for crawl completion and tests that want a deterministic barrier.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError::IndexClosed`] if the scheduler is shut down.
    pub async fn flush(&self) -> Result<()> {
        self.tx
            .send(IndexTask::Flush)
            .map_err(|_| SearchError::IndexClosed)
    }

    /// Shut the scheduler down and join the worker.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation.
    pub async fn shutdown(self) -> Result<()> {
        drop(self.tx);
        if let Some(handle) = self.worker.lock().take() {
            let _ = handle.await;
        }
        Ok(())
    }
}

async fn worker_loop(
    engine: Arc<SearchEngine>,
    mut rx: mpsc::UnboundedReceiver<IndexTask>,
) {
    loop {
        let Some(task) = rx.recv().await else {
            break;
        };
        let mut batch: Vec<IndexDocument> = Vec::new();
        let mut pending_commit = false;
        match task {
            IndexTask::Upsert(doc) => batch.push(doc),
            IndexTask::Remove(path) => {
                if let Err(e) = engine.remove(&path).await {
                    warn!(error = %e, %path, "index remove failed");
                }
                // Deletes must become searchable without an explicit flush —
                // the FS watcher never calls flush on its own.
                if let Err(e) = engine.commit().await {
                    warn!(error = %e, "index commit failed");
                }
                continue;
            }
            IndexTask::Flush => {
                if let Err(e) = engine.commit().await {
                    warn!(error = %e, "index commit failed");
                }
                continue;
            }
        }
        // Drain any additional upserts already queued.
        while batch.len() < BATCH_MAX {
            match rx.try_recv() {
                Ok(IndexTask::Upsert(d)) => batch.push(d),
                Ok(IndexTask::Remove(path)) => {
                    // Apply the pending upsert batch first so remove-after-upsert
                    // ordering stays correct, then remove + commit below.
                    if !batch.is_empty() {
                        debug!(count = batch.len(), "flushing batch");
                        if let Err(e) = engine.upsert_batch(std::mem::take(&mut batch)).await {
                            warn!(error = %e, "index upsert_batch failed");
                        }
                    }
                    if let Err(e) = engine.remove(&path).await {
                        warn!(error = %e, %path, "index remove failed");
                    }
                    pending_commit = true;
                    break;
                }
                Ok(IndexTask::Flush) => {
                    // Defer commit until after the upsert batch is written.
                    pending_commit = true;
                    break;
                }
                Err(_) => break,
            }
        }
        if !batch.is_empty() {
            debug!(count = batch.len(), "flushing batch");
            if let Err(e) = engine.upsert_batch(batch).await {
                warn!(error = %e, "index upsert_batch failed");
            }
            // Live FS events never send Flush; commit after each batch so
            // incremental indexing is visible to search without a crawl.
            pending_commit = true;
        }
        if pending_commit {
            if let Err(e) = engine.commit().await {
                warn!(error = %e, "index commit failed");
            }
        }
    }
}
