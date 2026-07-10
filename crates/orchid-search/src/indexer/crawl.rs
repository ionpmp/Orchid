//! Initial directory crawl that seeds the index scheduler.

use tracing::{debug, warn};

use orchid_fs::{FsEntryKind, FsPath, FsProviderRegistry};

use crate::engine::{DocumentKind, IndexDocument};
use crate::error::Result;
use crate::extractors::Extractor;
use crate::indexer::scheduler::IndexScheduler;
use crate::indexer::watcher::{glob_match, IndexScope};

/// Maximum directories visited during a single bootstrap crawl (breadth-first).
const MAX_DIRS: usize = 2_000;
/// Maximum files enqueued during a single bootstrap crawl.
const MAX_FILES: usize = 10_000;

/// Walk `roots` (breadth-first) and enqueue upserts for files in scope.
///
/// Best-effort: listing / metadata / extraction failures are logged and
/// skipped so a single bad path cannot abort indexing.
pub async fn crawl_roots(
    registry: &FsProviderRegistry,
    scheduler: &IndexScheduler,
    extractor: &Extractor,
    tags: &orchid_fs::TagManager,
    scope: &IndexScope,
    roots: &[FsPath],
) -> Result<()> {
    let mut dirs: Vec<FsPath> = roots.to_vec();
    let mut dirs_seen = 0usize;
    let mut files_enqueued = 0usize;

    while let Some(dir) = dirs.pop() {
        if dirs_seen >= MAX_DIRS || files_enqueued >= MAX_FILES {
            warn!(
                dirs_seen,
                files_enqueued,
                "search crawl hit safety cap; remaining paths will be indexed on change"
            );
            break;
        }
        dirs_seen += 1;

        let Some(provider) = registry.for_path(&dir) else {
            continue;
        };
        let entries = match provider.list(&dir).await {
            Ok(e) => e,
            Err(e) => {
                debug!(error = %e, %dir, "search crawl: list failed");
                continue;
            }
        };

        for entry in entries {
            if !path_in_scope(scope, &entry.path) {
                continue;
            }
            match entry.metadata.kind {
                FsEntryKind::Directory => {
                    dirs.push(entry.path);
                }
                FsEntryKind::File => {
                    if files_enqueued >= MAX_FILES {
                        break;
                    }
                    match build_document(
                        provider.as_ref(),
                        extractor,
                        tags,
                        scope,
                        &entry.path,
                        &entry.metadata,
                    )
                    .await
                    {
                        Ok(doc) => {
                            scheduler.enqueue_upsert(doc).await?;
                            files_enqueued += 1;
                        }
                        Err(e) => {
                            debug!(
                                error = %e,
                                path = %entry.path,
                                "search crawl: document build failed"
                            );
                        }
                    }
                }
                FsEntryKind::Symlink | FsEntryKind::Other => {}
            }
        }
    }

    if files_enqueued > 0 {
        let _ = scheduler.flush().await;
    }
    Ok(())
}

async fn build_document(
    provider: &dyn orchid_fs::FsProvider,
    extractor: &Extractor,
    tags: &orchid_fs::TagManager,
    scope: &IndexScope,
    path: &FsPath,
    meta: &orchid_fs::FsMetadata,
) -> Result<IndexDocument> {
    let name = path.file_name().unwrap_or("").to_string();
    let extension = path.extension().map(|s| s.to_ascii_lowercase());
    let modified = meta.modified.map(|t| t.timestamp()).unwrap_or(0);

    let tag_row = tags.get(path).unwrap_or_default();
    let tags_vec = tag_row
        .as_ref()
        .map(|t| t.tags.clone())
        .unwrap_or_default();
    let color = tag_row
        .and_then(|t| t.color_label)
        .map(|c| format!("{c:?}"));

    let content = if meta.size <= scope.max_file_size
        && (scope.extract_text_content || scope.extract_pdf_content)
    {
        match extractor
            .extract(provider, path, meta.mime.as_deref())
            .await
        {
            Ok(t) => t,
            Err(e) => {
                debug!(error = %e, %path, "search crawl: extraction failed");
                None
            }
        }
    } else {
        None
    };

    Ok(IndexDocument {
        path: path.as_str().to_string(),
        name,
        extension,
        content,
        tags: tags_vec,
        color_label: color,
        size: meta.size,
        modified,
        mime: meta.mime.clone(),
        kind: DocumentKind::File,
        in_archive: None,
    })
}

fn path_in_scope(scope: &IndexScope, path: &FsPath) -> bool {
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
