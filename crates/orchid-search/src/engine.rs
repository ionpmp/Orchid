//! Tantivy-backed [`SearchEngine`].

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query as TantivyQuery, RangeQuery, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

use crate::error::{Result, SearchError};
use crate::query::builder::Query;
use crate::query::snippet::{SearchHit, SearchResults, Snippet};
use crate::schema::Schema;

const INDEX_WRITER_BUDGET_BYTES: usize = 128 * 1024 * 1024;

/// Whether a document represents a file or a directory.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    File,
    Directory,
}

impl DocumentKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
        }
    }
}

/// Serialisable form of an indexed document.
#[derive(Debug, Clone)]
pub struct IndexDocument {
    /// Canonical path (used as primary key).
    pub path: String,
    /// Display name.
    pub name: String,
    /// Lowercased extension.
    pub extension: Option<String>,
    /// Extracted text content.
    pub content: Option<String>,
    /// Tag tokens.
    pub tags: Vec<String>,
    /// Colour label string.
    pub color_label: Option<String>,
    /// Size in bytes.
    pub size: u64,
    /// Last-modified Unix seconds.
    pub modified: i64,
    /// Sniffed MIME type.
    pub mime: Option<String>,
    /// File or directory.
    pub kind: DocumentKind,
    /// Outer archive path, if any.
    pub in_archive: Option<String>,
}

/// Handle to a live search index.
#[derive(Clone)]
pub struct SearchEngine {
    inner: Arc<SearchEngineInner>,
}

struct SearchEngineInner {
    schema: Schema,
    index: Index,
    reader: IndexReader,
    writer: Mutex<Option<IndexWriter>>,
}

impl std::fmt::Debug for SearchEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchEngine").finish_non_exhaustive()
    }
}

impl SearchEngine {
    /// Open (or create) an index at `index_dir`.
    ///
    /// # Errors
    ///
    /// Propagates Tantivy errors.
    pub fn open(index_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(index_dir)?;
        let schema = Schema::new();
        let index = if index_dir.read_dir()?.next().is_some() {
            Index::open_in_dir(index_dir)?
        } else {
            Index::create_in_dir(index_dir, schema.tantivy.clone())?
        };
        // The `en_stem` tokenizer ships with Tantivy — nothing extra to
        // register.

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        let writer = index.writer(INDEX_WRITER_BUDGET_BYTES)?;
        Ok(Self {
            inner: Arc::new(SearchEngineInner {
                schema,
                index,
                reader,
                writer: Mutex::new(Some(writer)),
            }),
        })
    }

    /// Schema handle for direct Tantivy-level use.
    #[must_use]
    pub fn schema(&self) -> &Schema {
        &self.inner.schema
    }

    /// Add or replace a single document (keyed by `doc.path`).
    ///
    /// # Errors
    ///
    /// Propagates Tantivy errors.
    pub async fn upsert(&self, doc: IndexDocument) -> Result<()> {
        self.upsert_batch(vec![doc]).await
    }

    /// Delete a document by its path.
    ///
    /// # Errors
    ///
    /// Propagates Tantivy errors.
    pub async fn remove(&self, path: &str) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let path = path.to_string();
        tokio::task::spawn_blocking(move || remove_sync(&inner, &path))
            .await
            .map_err(|e| SearchError::Extraction {
                path: String::new(),
                reason: format!("join: {e}"),
            })?
    }

    /// Bulk index. All documents commit in a single transaction.
    ///
    /// # Errors
    ///
    /// Propagates Tantivy errors.
    pub async fn upsert_batch(&self, docs: Vec<IndexDocument>) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || upsert_batch_sync(&inner, &docs))
            .await
            .map_err(|e| SearchError::Extraction {
                path: String::new(),
                reason: format!("join: {e}"),
            })?
    }

    /// Commit any pending changes to disk.
    ///
    /// # Errors
    ///
    /// Propagates Tantivy errors.
    pub async fn commit(&self) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let mut guard = inner.writer.lock();
            if let Some(w) = guard.as_mut() {
                w.commit().map(|_| ())?;
            }
            Ok::<_, SearchError>(())
        })
        .await
        .map_err(|e| SearchError::Extraction {
            path: String::new(),
            reason: format!("join: {e}"),
        })?
    }

    /// Current number of live documents in the index.
    ///
    /// # Errors
    ///
    /// Propagates Tantivy errors.
    pub fn doc_count(&self) -> Result<u64> {
        self.inner.reader.reload()?;
        let searcher = self.inner.reader.searcher();
        Ok(searcher.num_docs())
    }

    /// Run a query. Returns up to `query.limit` ranked hits.
    ///
    /// # Errors
    ///
    /// Propagates Tantivy errors.
    pub async fn search(&self, query: Query) -> Result<SearchResults> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || run_query(&inner, &query))
            .await
            .map_err(|e| SearchError::Extraction {
                path: String::new(),
                reason: format!("join: {e}"),
            })?
    }

    /// Merge segments.
    ///
    /// # Errors
    ///
    /// Propagates Tantivy errors.
    pub async fn optimize(&self) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut guard = inner.writer.lock();
            if let Some(w) = guard.as_mut() {
                let meta = inner.index.load_metas()?;
                let segment_ids: Vec<_> = meta.segments.iter().map(|s| s.id()).collect();
                if segment_ids.len() > 1 {
                    let _ = futures_executor::block_on(w.merge(&segment_ids));
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| SearchError::Extraction {
            path: String::new(),
            reason: format!("join: {e}"),
        })?
    }

    /// Drop the writer (idempotent).
    ///
    /// # Errors
    ///
    /// Propagates Tantivy errors.
    pub async fn shutdown(self) -> Result<()> {
        let inner = self.inner;
        tokio::task::spawn_blocking(move || {
            let writer = {
                let mut guard = inner.writer.lock();
                guard.take()
            };
            if let Some(mut w) = writer {
                let _ = w.commit();
            }
        })
        .await
        .map_err(|e| SearchError::Extraction {
            path: String::new(),
            reason: format!("join: {e}"),
        })
    }
}

// `tantivy::IndexWriter::merge` returns a future; we need a synchronous
// driver for the `spawn_blocking` context. Use `futures_executor`, which is
// already pulled in transitively by Tantivy. If it isn't, we fall back to
// a tiny hand-rolled block_on.
mod futures_executor {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, Waker};

    pub fn block_on<F: Future>(fut: F) -> F::Output {
        // Minimal executor: park the current thread, poll once per wake-up.
        let mut fut = Box::pin(fut);
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(&waker);
        loop {
            match Pin::as_mut(&mut fut).poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

}

// -------------------------------------------------------------------------
// Sync helpers run inside spawn_blocking.
// -------------------------------------------------------------------------

fn upsert_batch_sync(inner: &SearchEngineInner, docs: &[IndexDocument]) -> Result<()> {
    let mut guard = inner.writer.lock();
    let writer = guard.as_mut().ok_or(SearchError::IndexClosed)?;
    for d in docs {
        // Delete any existing doc with the same path first.
        let term = Term::from_field_text(inner.schema.field_path, &d.path);
        writer.delete_term(term);

        let mut tantivy_doc = TantivyDocument::default();
        tantivy_doc.add_text(inner.schema.field_path, &d.path);
        tantivy_doc.add_text(inner.schema.field_name, &d.name);
        if let Some(ext) = &d.extension {
            tantivy_doc.add_text(inner.schema.field_extension, ext);
        }
        if let Some(content) = &d.content {
            tantivy_doc.add_text(inner.schema.field_content, content);
        }
        for t in &d.tags {
            tantivy_doc.add_text(inner.schema.field_tags, t);
        }
        if let Some(c) = &d.color_label {
            tantivy_doc.add_text(inner.schema.field_color_label, c);
        }
        tantivy_doc.add_u64(inner.schema.field_size, d.size);
        tantivy_doc.add_i64(inner.schema.field_modified, d.modified);
        if let Some(m) = &d.mime {
            tantivy_doc.add_text(inner.schema.field_mime, m);
        }
        tantivy_doc.add_text(inner.schema.field_kind, d.kind.as_str());
        if let Some(arch) = &d.in_archive {
            tantivy_doc.add_text(inner.schema.field_in_archive, arch);
        }
        writer.add_document(tantivy_doc)?;
    }
    // NOTE: we do not commit here. Callers commit explicitly via
    // `SearchEngine::commit` (or `IndexScheduler::flush`). Double-committing
    // causes file-handle races on Windows.
    Ok(())
}

fn remove_sync(inner: &SearchEngineInner, path: &str) -> Result<()> {
    let mut guard = inner.writer.lock();
    let writer = guard.as_mut().ok_or(SearchError::IndexClosed)?;
    let term = Term::from_field_text(inner.schema.field_path, path);
    writer.delete_term(term);
    // Commit is the caller's responsibility — see `upsert_batch_sync` note.
    Ok(())
}

fn run_query(inner: &SearchEngineInner, q: &Query) -> Result<SearchResults> {
    use tantivy::query::QueryParser;

    inner.reader.reload()?;
    let searcher = inner.reader.searcher();

    let mut clauses: Vec<(Occur, Box<dyn TantivyQuery>)> = Vec::new();

    if let Some(text) = q.text.as_ref().filter(|s| !s.trim().is_empty()) {
        let parser = QueryParser::for_index(
            &inner.index,
            vec![inner.schema.field_name, inner.schema.field_content],
        );
        match parser.parse_query(text) {
            Ok(p) => clauses.push((Occur::Must, p)),
            Err(e) => return Err(SearchError::QueryParse(e.to_string())),
        }
    }
    for ext in &q.extensions {
        let term = Term::from_field_text(inner.schema.field_extension, &ext.to_lowercase());
        clauses.push((
            Occur::Must,
            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
        ));
    }
    for mime in &q.mimes {
        let term = Term::from_field_text(inner.schema.field_mime, mime);
        clauses.push((
            Occur::Must,
            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
        ));
    }
    for tag in &q.tags {
        let term = Term::from_field_text(inner.schema.field_tags, tag);
        clauses.push((
            Occur::Must,
            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
        ));
    }
    if let Some(c) = &q.color_label {
        let term = Term::from_field_text(inner.schema.field_color_label, c);
        clauses.push((
            Occur::Must,
            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
        ));
    }
    if let Some(prefix) = q.path_prefix.as_ref().filter(|s| !s.is_empty()) {
        let parser =
            QueryParser::for_index(&inner.index, vec![inner.schema.field_path]);
        let wildcard = format!("{prefix}*");
        if let Ok(p) = parser.parse_query(&wildcard) {
            clauses.push((Occur::Must, p));
        }
    }
    if q.min_size.is_some() || q.max_size.is_some() {
        let lo = q.min_size.unwrap_or(0);
        let hi = q.max_size.unwrap_or(u64::MAX);
        clauses.push((
            Occur::Must,
            Box::new(RangeQuery::new_u64_bounds(
                "size".to_string(),
                std::ops::Bound::Included(lo),
                std::ops::Bound::Included(hi),
            )),
        ));
    }
    if q.modified_after.is_some() || q.modified_before.is_some() {
        let lo = q.modified_after.unwrap_or(i64::MIN);
        let hi = q.modified_before.unwrap_or(i64::MAX);
        clauses.push((
            Occur::Must,
            Box::new(RangeQuery::new_i64_bounds(
                "modified".to_string(),
                std::ops::Bound::Included(lo),
                std::ops::Bound::Included(hi),
            )),
        ));
    }
    if q.only_files {
        let term = Term::from_field_text(inner.schema.field_kind, "file");
        clauses.push((
            Occur::Must,
            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
        ));
    }
    if q.only_directories {
        let term = Term::from_field_text(inner.schema.field_kind, "directory");
        clauses.push((
            Occur::Must,
            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
        ));
    }

    // Empty query → match everything.
    let combined: Box<dyn TantivyQuery> = if clauses.is_empty() {
        Box::new(tantivy::query::AllQuery)
    } else {
        Box::new(BooleanQuery::new(clauses))
    };

    let start = std::time::Instant::now();
    let limit = q.limit.max(1);
    let collector = TopDocs::with_limit(limit).and_offset(q.offset);
    let top = searcher.search(&combined, &collector)?;
    let elapsed_ms = start.elapsed().as_millis() as u64;

    // Snippets only make sense for free-text queries against stored content.
    let snippet_generator = q
        .text
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|_| {
            tantivy::snippet::SnippetGenerator::create(
                &searcher,
                &*combined,
                inner.schema.field_content,
            )
            .ok()
            .map(|mut gen| {
                gen.set_max_num_chars(160);
                gen
            })
        });

    let mut hits = Vec::with_capacity(top.len());
    for (score, addr) in top {
        let retrieved: TantivyDocument = searcher.doc(addr)?;
        let path = get_str(&retrieved, inner.schema.field_path).unwrap_or_default();
        let name = get_str(&retrieved, inner.schema.field_name).unwrap_or_default();
        let extension = get_str(&retrieved, inner.schema.field_extension);
        let size = get_u64(&retrieved, inner.schema.field_size).unwrap_or(0);
        let modified = get_i64(&retrieved, inner.schema.field_modified).unwrap_or(0);
        let mime = get_str(&retrieved, inner.schema.field_mime);
        let kind_str = get_str(&retrieved, inner.schema.field_kind);
        let kind = if kind_str.as_deref() == Some("directory") {
            DocumentKind::Directory
        } else {
            DocumentKind::File
        };

        let snippet = snippet_generator
            .as_ref()
            .and_then(|gen| snippet_from_doc(gen, &retrieved));

        hits.push(SearchHit {
            path,
            name,
            extension,
            size,
            modified,
            mime,
            kind,
            score,
            snippet,
        });
    }

    // Total estimate: we can only give what TopDocs collected; if the caller
    // needs a precise count they should use `doc_count` separately.
    let total = hits.len() as u64;
    Ok(SearchResults {
        hits,
        total_estimated: total,
        query_time_ms: elapsed_ms,
    })
}

fn snippet_from_doc(
    generator: &tantivy::snippet::SnippetGenerator,
    doc: &TantivyDocument,
) -> Option<Snippet> {
    let tantivy_snippet = generator.snippet_from_doc(doc);
    let text = tantivy_snippet.fragment().to_string();
    if text.trim().is_empty() {
        return None;
    }
    let highlights = tantivy_snippet
        .highlighted()
        .iter()
        .map(|range| (range.start as u32, range.end as u32))
        .collect();
    Some(Snippet { text, highlights })
}

fn get_str(d: &TantivyDocument, field: tantivy::schema::Field) -> Option<String> {
    use tantivy::schema::Value;
    d.get_first(field).and_then(|v| v.as_str().map(ToOwned::to_owned))
}
fn get_u64(d: &TantivyDocument, field: tantivy::schema::Field) -> Option<u64> {
    use tantivy::schema::Value;
    d.get_first(field).and_then(|v| v.as_u64())
}
fn get_i64(d: &TantivyDocument, field: tantivy::schema::Field) -> Option<i64> {
    use tantivy::schema::Value;
    d.get_first(field).and_then(|v| v.as_i64())
}

