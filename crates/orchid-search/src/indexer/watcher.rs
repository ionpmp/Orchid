//! FS event subscriber that feeds the index scheduler.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::engine::{DocumentKind, IndexDocument};
use crate::error::Result;
use crate::extractors::Extractor;
use crate::indexer::scheduler::IndexScheduler;

/// Index-coverage policy.
#[derive(Debug, Clone, Default)]
pub struct IndexScope {
    /// Only paths under one of these roots are indexed.
    pub included_roots: Vec<orchid_fs::FsPath>,
    /// Glob-style exclusion patterns matched against the relative path
    /// under each root.
    pub excluded_patterns: Vec<String>,
    /// Files larger than this are not content-extracted.
    pub max_file_size: u64,
    /// Text content extraction toggle.
    pub extract_text_content: bool,
    /// PDF content extraction toggle.
    pub extract_pdf_content: bool,
}

impl IndexScope {
    /// Reasonable defaults: 50 MiB content cap, text + PDF on.
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            included_roots: Vec::new(),
            excluded_patterns: Vec::new(),
            max_file_size: 50 * 1024 * 1024,
            extract_text_content: true,
            extract_pdf_content: true,
        }
    }
}

/// Subscribes to `fs.*` and `fs.tags_changed` events and reindexes affected
/// documents.
pub struct IndexFsSubscriber {
    inner: Arc<SubscriberInner>,
}

struct SubscriberInner {
    bus: Arc<orchid_core::EventBus>,
    scheduler: Arc<IndexScheduler>,
    extractor: Arc<Extractor>,
    registry: Arc<orchid_fs::FsProviderRegistry>,
    tags: Arc<orchid_fs::TagManager>,
    scope: RwLock<IndexScope>,
    running: AtomicBool,
    tasks: parking_lot::Mutex<Vec<JoinHandle<()>>>,
}

impl std::fmt::Debug for IndexFsSubscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexFsSubscriber").finish_non_exhaustive()
    }
}

impl Clone for IndexFsSubscriber {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl IndexFsSubscriber {
    /// Build a subscriber.
    #[must_use]
    pub fn new(
        bus: Arc<orchid_core::EventBus>,
        scheduler: Arc<IndexScheduler>,
        extractor: Arc<Extractor>,
        registry: Arc<orchid_fs::FsProviderRegistry>,
        tags: Arc<orchid_fs::TagManager>,
    ) -> Self {
        Self {
            inner: Arc::new(SubscriberInner {
                bus,
                scheduler,
                extractor,
                registry,
                tags,
                scope: RwLock::new(IndexScope::defaults()),
                running: AtomicBool::new(false),
                tasks: parking_lot::Mutex::new(Vec::new()),
            }),
        }
    }

    /// Replace the scope atomically.
    pub fn set_scope(&self, scope: IndexScope) {
        *self.inner.scope.write() = scope;
    }

    /// Subscribe to the bus and start feeding the scheduler.
    ///
    /// # Errors
    ///
    /// Propagates subscription errors from the event bus.
    pub async fn start(&self) -> Result<()> {
        if self.inner.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let sub = self.clone();
        let (handle, mut rx) = self.inner.bus.subscribe(
            orchid_core::EventFilter::any(),
            orchid_core::HandlerPriority::Normal,
        )?;
        handle.leak();

        let task = tokio::spawn(async move {
            while let Some(envelope) = rx.recv().await {
                if !sub.inner.running.load(Ordering::SeqCst) {
                    break;
                }
                if let Err(e) = sub.handle_envelope(&envelope).await {
                    warn!(error = %e, "indexer: failed to handle bus envelope");
                }
            }
        });
        self.inner.tasks.lock().push(task);
        Ok(())
    }

    /// Stop the subscriber.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation.
    pub async fn shutdown(&self) -> Result<()> {
        self.inner.running.store(false, Ordering::SeqCst);
        let tasks = std::mem::take(&mut *self.inner.tasks.lock());
        for h in tasks {
            h.abort();
        }
        Ok(())
    }

    async fn handle_envelope(&self, envelope: &orchid_core::EventEnvelope) -> Result<()> {
        if let Some(ev) = envelope.downcast::<orchid_fs::FsCreatedEvent>() {
            self.reindex(&ev.path).await?;
        } else if let Some(ev) = envelope.downcast::<orchid_fs::FsModifiedEvent>() {
            self.reindex(&ev.path).await?;
        } else if let Some(ev) = envelope.downcast::<orchid_fs::FsDeletedEvent>() {
            self.inner
                .scheduler
                .enqueue_remove(ev.path.as_str().to_string())
                .await?;
        } else if let Some(ev) = envelope.downcast::<orchid_fs::FsRenamedEvent>() {
            self.inner
                .scheduler
                .enqueue_remove(ev.from.as_str().to_string())
                .await?;
            self.reindex(&ev.to).await?;
        } else if let Some(ev) = envelope.downcast::<orchid_fs::TagsChangedEvent>() {
            self.reindex(&ev.path).await?;
        }
        Ok(())
    }

    /// Build an `IndexDocument` for `path` (respecting scope) and enqueue.
    async fn reindex(&self, path: &orchid_fs::FsPath) -> Result<()> {
        if !self.path_in_scope(path) {
            return Ok(());
        }
        let Some(provider) = self.inner.registry.for_path(path) else {
            return Ok(());
        };
        let meta = match provider.metadata(path).await {
            Ok(m) => m,
            Err(e) => {
                debug!(error = %e, %path, "indexer: metadata fetch failed");
                return Ok(());
            }
        };
        let scope = self.inner.scope.read().clone();
        let name = path
            .file_name()
            .unwrap_or("")
            .to_string();
        let extension = path.extension().map(|s| s.to_ascii_lowercase());
        let modified = meta.modified.map(|t| t.timestamp()).unwrap_or(0);

        let tags = self
            .inner
            .tags
            .get(path)
            .unwrap_or_default()
            .map(|t| t.tags)
            .unwrap_or_default();
        let color = self
            .inner
            .tags
            .get(path)
            .unwrap_or_default()
            .and_then(|t| t.color_label)
            .map(|c| format!("{c:?}"));

        let content = if matches!(meta.kind, orchid_fs::FsEntryKind::File)
            && meta.size <= scope.max_file_size
            && (scope.extract_text_content || scope.extract_pdf_content)
        {
            match self
                .inner
                .extractor
                .extract(provider.as_ref(), path, meta.mime.as_deref())
                .await
            {
                Ok(t) => t,
                Err(e) => {
                    debug!(error = %e, %path, "indexer: extraction failed");
                    None
                }
            }
        } else {
            None
        };

        let kind = if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
            DocumentKind::Directory
        } else {
            DocumentKind::File
        };

        let doc = IndexDocument {
            path: path.as_str().to_string(),
            name,
            extension,
            content,
            tags,
            color_label: color,
            size: meta.size,
            modified,
            mime: meta.mime,
            kind,
            in_archive: None,
        };
        self.inner.scheduler.enqueue_upsert(doc).await?;
        Ok(())
    }

    fn path_in_scope(&self, path: &orchid_fs::FsPath) -> bool {
        let scope = self.inner.scope.read();
        if scope.included_roots.is_empty() {
            return false;
        }
        let path_s = path.as_str();
        let Some(root) = scope
            .included_roots
            .iter()
            .find(|r| path_s.starts_with(r.as_str()))
        else {
            return false;
        };
        let rel = path_s
            .strip_prefix(root.as_str())
            .unwrap_or(path_s)
            .trim_start_matches('/');
        for pat in &scope.excluded_patterns {
            if glob_match(pat, rel) {
                return false;
            }
        }
        true
    }
}

/// Minimal glob matcher supporting `*` (any segment), `**` (any subtree),
/// `?` (any single char). Sufficient for exclusion patterns like
/// `"*.tmp"` and `"node_modules/**"`.
pub(crate) fn glob_match(pattern: &str, target: &str) -> bool {
    glob_match_inner(pattern.as_bytes(), target.as_bytes())
}

fn glob_match_inner(p: &[u8], t: &[u8]) -> bool {
    match (p.first(), t.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            if p.len() >= 2 && p[1] == b'*' {
                // `**` matches any subtree including slashes.
                for i in 0..=t.len() {
                    if glob_match_inner(&p[2..], &t[i..]) {
                        return true;
                    }
                }
                return false;
            }
            // `*` matches anything except a path separator.
            for i in 0..=t.len() {
                if t[..i].contains(&b'/') {
                    break;
                }
                if glob_match_inner(&p[1..], &t[i..]) {
                    return true;
                }
            }
            false
        }
        (Some(b'?'), Some(_)) => glob_match_inner(&p[1..], &t[1..]),
        (Some(a), Some(b)) if a == b => glob_match_inner(&p[1..], &t[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_star_extension() {
        assert!(glob_match("*.tmp", "foo.tmp"));
        assert!(!glob_match("*.tmp", "foo/bar.tmp")); // `*` doesn't cross slashes
        assert!(glob_match("**/*.tmp", "foo/bar.tmp"));
    }

    #[test]
    fn glob_subtree() {
        assert!(glob_match("node_modules/**", "node_modules/x/y/z.js"));
        assert!(!glob_match("node_modules/**", "src/node_modules_fake/x"));
    }
}
