//! Managed-folder engine: ingest files into the content-addressed store and
//! track manifests keyed by on-disk path.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use orchid_crypto::{ChunkStore, Deduplicator, FileManifest};
use redb::ReadableTable;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::error::{FsError, Result};
use crate::managed::config::{ManagedFolderConfig, ManagedFolderStats};
use crate::managed::index::{MANAGED_FOLDERS, MANAGED_MANIFESTS};
use crate::path::FsPath;
use crate::provider::FsProviderRegistry;
use crate::watcher::events::{FsCreatedEvent, FsModifiedEvent};
use crate::watcher::FileWatcher;

/// Emitted when ingestion of a managed file begins.
#[derive(Debug, Clone)]
pub struct ManagedFileIngestStartedEvent {
    /// Path being ingested.
    pub path: FsPath,
}
impl orchid_core::Event for ManagedFileIngestStartedEvent {
    fn event_type() -> &'static str {
        "fs.managed_ingest_started"
    }
}

/// Emitted when ingestion fails (after [`ManagedFileIngestStartedEvent`]).
#[derive(Debug, Clone)]
pub struct ManagedFileIngestFailedEvent {
    /// Path that failed to ingest.
    pub path: FsPath,
}
impl orchid_core::Event for ManagedFileIngestFailedEvent {
    fn event_type() -> &'static str {
        "fs.managed_ingest_failed"
    }
}

/// Emitted when a file inside a managed folder has been ingested.
#[derive(Debug, Clone)]
pub struct ManagedFileIngestedEvent {
    /// Path of the ingested file.
    pub path: FsPath,
    /// Manifest identifier.
    pub manifest_id: uuid::Uuid,
}
impl orchid_core::Event for ManagedFileIngestedEvent {
    fn event_type() -> &'static str {
        "fs.managed_ingested"
    }
}

/// Primary engine for managed folders.
///
/// **MVP trade-off:** for tool-compatibility reasons the engine does *not*
/// replace the user's on-disk file with a manifest reference. Files stay in
/// place; the chunk store holds an additional, deduplicated copy. Actual
/// on-disk savings kick in only when the same content recurs. A full
/// reflink-based strategy (avoiding the redundant copy) is planned for
/// v1.x; see [`crate::managed`] module docs.
pub struct ManagedFolderEngine {
    inner: Arc<ManagedEngineInner>,
}

struct ManagedEngineInner {
    storage: Arc<orchid_storage::StateStore>,
    #[allow(dead_code)]
    crypto_store: Arc<ChunkStore>,
    deduplicator: Arc<Deduplicator>,
    registry: Arc<FsProviderRegistry>,
    bus: Arc<orchid_core::EventBus>,
    watcher: Arc<FileWatcher>,
    running: AtomicBool,
    loop_task: parking_lot::Mutex<Option<JoinHandle<()>>>,
    watch_handles: parking_lot::Mutex<Vec<crate::watcher::WatchHandle>>,
}

impl std::fmt::Debug for ManagedFolderEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedFolderEngine").finish_non_exhaustive()
    }
}

impl Clone for ManagedFolderEngine {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl ManagedFolderEngine {
    /// Construct a new engine.
    #[must_use]
    pub fn new(
        storage: Arc<orchid_storage::StateStore>,
        crypto_store: Arc<ChunkStore>,
        deduplicator: Arc<Deduplicator>,
        registry: Arc<FsProviderRegistry>,
        bus: Arc<orchid_core::EventBus>,
        watcher: Arc<FileWatcher>,
    ) -> Self {
        Self {
            inner: Arc::new(ManagedEngineInner {
                storage,
                crypto_store,
                deduplicator,
                registry,
                bus,
                watcher,
                running: AtomicBool::new(false),
                loop_task: parking_lot::Mutex::new(None),
                watch_handles: parking_lot::Mutex::new(Vec::new()),
            }),
        }
    }

    /// Register a managed folder.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub async fn add_folder(&self, cfg: ManagedFolderConfig) -> Result<()> {
        let db = self.inner.storage.raw_database();
        let txn = db
            .begin_write()
            .map_err(|e| FsError::Storage(e.into()))?;
        {
            let mut table = txn
                .open_table(MANAGED_FOLDERS)
                .map_err(|e| FsError::Storage(e.into()))?;
            let key = cfg.path.as_str();
            table
                .insert(key, &cfg)
                .map_err(|e| FsError::Storage(e.into()))?;
        }
        txn.commit().map_err(|e| FsError::Storage(e.into()))?;
        Ok(())
    }

    /// Remove a managed-folder declaration (manifests are kept in place).
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub async fn remove_folder(&self, path: &FsPath) -> Result<()> {
        let db = self.inner.storage.raw_database();
        let txn = db
            .begin_write()
            .map_err(|e| FsError::Storage(e.into()))?;
        {
            let mut table = txn
                .open_table(MANAGED_FOLDERS)
                .map_err(|e| FsError::Storage(e.into()))?;
            let _ = table
                .remove(path.as_str())
                .map_err(|e| FsError::Storage(e.into()))?;
        }
        txn.commit().map_err(|e| FsError::Storage(e.into()))?;
        Ok(())
    }

    /// List every registered managed folder.
    ///
    /// # Errors
    ///
    /// Propagates storage errors.
    pub async fn list_folders(&self) -> Result<Vec<ManagedFolderConfig>> {
        let db = self.inner.storage.raw_database();
        let txn = db
            .begin_read()
            .map_err(|e| FsError::Storage(e.into()))?;
        let table = match txn.open_table(MANAGED_FOLDERS) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(FsError::Storage(e.into())),
        };
        let mut out = Vec::new();
        for item in table.iter().map_err(|e| FsError::Storage(e.into()))? {
            let (_, v) = item.map_err(|e| FsError::Storage(e.into()))?;
            out.push(v.value());
        }
        Ok(out)
    }

    /// Ingest a single file manually. Does not move / delete the original.
    ///
    /// # Errors
    ///
    /// Propagates crypto / storage errors. Skips ingest when policy excludes the
    /// path or the folder quota is exceeded.
    pub async fn ingest(&self, path: &FsPath) -> Result<FileManifest> {
        if let Some(cfg) = self.config_for_path(path).await? {
            if let Some(ref policy) = cfg.policy {
                if !policy.should_ingest(path.as_str()) {
                    debug!(%path, "managed ingest skipped: excluded by policy");
                    return Err(FsError::ManagedIngestExcluded(path.to_string()));
                }
                let stats = self.folder_stats(&cfg.path).await?;
                if let Err(e) = policy.check_quota(&stats) {
                    warn!(
                        error = %e,
                        %path,
                        folder = %cfg.path,
                        "managed ingest skipped: quota exceeded"
                    );
                    return Err(e);
                }
            }
        }

        self.inner.bus.publish(
            orchid_core::EventSource::Subsystem("fs.managed".into()),
            ManagedFileIngestStartedEvent {
                path: path.clone(),
            },
        );
        let os_path = match path.to_local() {
            Ok(p) => p,
            Err(e) => {
                self.inner.bus.publish(
                    orchid_core::EventSource::Subsystem("fs.managed".into()),
                    ManagedFileIngestFailedEvent {
                        path: path.clone(),
                    },
                );
                return Err(e);
            }
        };
        let manifest = match self.inner.deduplicator.ingest_file(&os_path).await {
            Ok(m) => m,
            Err(e) => {
                self.inner.bus.publish(
                    orchid_core::EventSource::Subsystem("fs.managed".into()),
                    ManagedFileIngestFailedEvent {
                        path: path.clone(),
                    },
                );
                return Err(e.into());
            }
        };
        let manifest_id = manifest.id;

        let db = self.inner.storage.raw_database();
        let txn = db
            .begin_write()
            .map_err(|e| FsError::Storage(e.into()))?;
        {
            let mut table = txn
                .open_table(MANAGED_MANIFESTS)
                .map_err(|e| FsError::Storage(e.into()))?;
            table
                .insert(path.as_str(), &manifest)
                .map_err(|e| FsError::Storage(e.into()))?;
        }
        if let Err(e) = txn.commit().map_err(|e| FsError::Storage(e.into())) {
            self.inner.bus.publish(
                orchid_core::EventSource::Subsystem("fs.managed".into()),
                ManagedFileIngestFailedEvent {
                    path: path.clone(),
                },
            );
            return Err(e);
        }

        self.inner.bus.publish(
            orchid_core::EventSource::Subsystem("fs.managed".into()),
            ManagedFileIngestedEvent {
                path: path.clone(),
                manifest_id,
            },
        );
        Ok(manifest)
    }

    /// Compute aggregate stats for a managed folder.
    ///
    /// # Errors
    ///
    /// Propagates storage errors. Returns [`FsError::NotManagedFolder`] if
    /// the folder is not registered.
    pub async fn folder_stats(&self, path: &FsPath) -> Result<ManagedFolderStats> {
        let folders = self.list_folders().await?;
        if !folders.iter().any(|f| f.path == *path) {
            return Err(FsError::NotManagedFolder(path.to_string()));
        }
        let prefix = path.as_str();
        let db = self.inner.storage.raw_database();
        let txn = db
            .begin_read()
            .map_err(|e| FsError::Storage(e.into()))?;
        let table = match txn.open_table(MANAGED_MANIFESTS) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(ManagedFolderStats::default()),
            Err(e) => return Err(FsError::Storage(e.into())),
        };

        let mut files_tracked: u64 = 0;
        let mut logical_bytes: u64 = 0;
        let mut unique: HashSet<[u8; 32]> = HashSet::new();
        let mut physical_bytes: u64 = 0;
        for item in table.iter().map_err(|e| FsError::Storage(e.into()))? {
            let (k, v) = item.map_err(|e| FsError::Storage(e.into()))?;
            let key_s = k.value();
            if !key_s.starts_with(prefix) {
                continue;
            }
            let manifest = v.value();
            files_tracked += 1;
            logical_bytes += manifest.total_size;
            for chunk in &manifest.chunks {
                if unique.insert(chunk.hash) {
                    physical_bytes += chunk.length as u64;
                }
            }
        }
        Ok(ManagedFolderStats {
            files_tracked,
            logical_bytes,
            unique_chunks: unique.len() as u64,
            physical_bytes,
        })
    }

    /// Start the auto-ingest loop. No-op if already running.
    ///
    /// # Errors
    ///
    /// Propagates subscription errors from the event bus.
    pub async fn start(&self) -> Result<()> {
        if self.inner.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        // Subscribe to created / modified events across all managed folders.
        let filter = orchid_core::EventFilter::any();
        let (handle, mut rx) = self
            .inner
            .bus
            .subscribe(filter, orchid_core::HandlerPriority::Normal)?;
        // We own the handle for the task's lifetime.
        handle.leak();

        // Also install filesystem watches per managed folder so events
        // actually reach the bus.
        let folders = self.list_folders().await?;
        for f in folders {
            if let Ok(wh) = self.inner.watcher.watch(f.path.clone()).await {
                self.inner.watch_handles.lock().push(wh);
            }
        }

        let engine = self.clone();
        let task = tokio::spawn(async move {
            while let Some(envelope) = rx.recv().await {
                if !engine.inner.running.load(Ordering::SeqCst) {
                    break;
                }
                let maybe_path = envelope
                    .downcast::<FsCreatedEvent>()
                    .map(|e| e.path.clone())
                    .or_else(|| envelope.downcast::<FsModifiedEvent>().map(|e| e.path.clone()));
                if let Some(path) = maybe_path {
                    if engine.path_is_under_managed(&path).await.unwrap_or(false) {
                        if let Err(e) = engine.ingest(&path).await {
                            warn!(error = %e, %path, "managed auto-ingest failed");
                        } else {
                            debug!(%path, "managed auto-ingest complete");
                        }
                    }
                }
            }
        });
        *self.inner.loop_task.lock() = Some(task);
        Ok(())
    }

    /// Stop the auto-ingest loop and drop watches.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation.
    pub async fn shutdown(&self) -> Result<()> {
        self.inner.running.store(false, Ordering::SeqCst);
        if let Some(task) = self.inner.loop_task.lock().take() {
            task.abort();
        }
        self.inner.watch_handles.lock().clear();
        Ok(())
    }

    // ----------------------------------------------------------------
    // Helpers
    // ----------------------------------------------------------------

    async fn path_is_under_managed(&self, path: &FsPath) -> Result<bool> {
        Ok(self.config_for_path(path).await?.is_some())
    }

    async fn config_for_path(&self, path: &FsPath) -> Result<Option<ManagedFolderConfig>> {
        let folders = self.list_folders().await?;
        let p = path.as_str();
        Ok(folders.into_iter().find(|f| {
            f.enabled && f.auto_ingest && p.starts_with(f.path.as_str())
        }))
    }
}

// For compile-time access to the registry field even if we don't use it in
// this minimal MVP.
#[allow(dead_code)]
fn _registry_field(inner: &ManagedEngineInner) -> &FsProviderRegistry {
    &inner.registry
}
