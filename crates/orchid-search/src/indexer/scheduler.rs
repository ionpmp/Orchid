//! Batched, async index scheduler that drains tasks into a [`SearchEngine`].

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
    workers: Arc<parking_lot::Mutex<Vec<JoinHandle<()>>>>,
}

impl std::fmt::Debug for IndexScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexScheduler").finish_non_exhaustive()
    }
}

const BATCH_MAX: usize = 64;

impl IndexScheduler {
    /// Spawn `concurrency` background workers.
    #[must_use]
    pub fn new(engine: Arc<SearchEngine>, concurrency: usize) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let rx = Arc::new(tokio::sync::Mutex::new(rx));
        let workers = (0..concurrency.max(1))
            .map(|_| {
                let engine = Arc::clone(&engine);
                let rx = Arc::clone(&rx);
                tokio::spawn(async move { worker_loop(engine, rx).await })
            })
            .collect();
        Self {
            tx,
            workers: Arc::new(parking_lot::Mutex::new(workers)),
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

    /// Ask the scheduler to flush after draining pending work.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError::IndexClosed`] if the scheduler is shut down.
    pub async fn flush(&self) -> Result<()> {
        self.tx
            .send(IndexTask::Flush)
            .map_err(|_| SearchError::IndexClosed)
    }

    /// Shut the scheduler down and join every worker.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation.
    pub async fn shutdown(self) -> Result<()> {
        drop(self.tx);
        let handles = std::mem::take(&mut *self.workers.lock());
        for h in handles {
            let _ = h.await;
        }
        Ok(())
    }
}

async fn worker_loop(
    engine: Arc<SearchEngine>,
    rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<IndexTask>>>,
) {
    loop {
        let task = {
            let mut guard = rx.lock().await;
            guard.recv().await
        };
        let Some(task) = task else { break };
        let mut batch: Vec<IndexDocument> = Vec::new();
        match task {
            IndexTask::Upsert(doc) => batch.push(doc),
            IndexTask::Remove(path) => {
                if let Err(e) = engine.remove(&path).await {
                    warn!(error = %e, %path, "index remove failed");
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
            let mut guard = rx.lock().await;
            match guard.try_recv() {
                Ok(IndexTask::Upsert(d)) => batch.push(d),
                Ok(IndexTask::Remove(path)) => {
                    drop(guard);
                    if let Err(e) = engine.remove(&path).await {
                        warn!(error = %e, %path, "index remove failed");
                    }
                    break;
                }
                Ok(IndexTask::Flush) => {
                    drop(guard);
                    if let Err(e) = engine.commit().await {
                        warn!(error = %e, "index commit failed");
                    }
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
        }
    }
}
