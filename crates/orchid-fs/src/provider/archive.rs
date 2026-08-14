//! Browse an archive as a folder via `archive:<file>#<inner>`.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;

use crate::archive::ops::{add_named_file_sync, add_to_archive, delete_from_archive};
use crate::archive::reader::open_archive;
use crate::archive::types::ArchiveEntry;
use crate::entry::{ExtendedAttributes, FsEntry, FsEntryKind, FsMetadata};
use crate::error::{FsError, Result};
use crate::path::{FsPath, SCHEME_ARCHIVE};
use crate::provider::{FsCapabilities, FsProvider, FsWatcherHandle, ProviderId};

/// Pseudo-provider that lists and mutates archive contents.
pub struct ArchiveProvider {
    id: ProviderId,
}

impl Default for ArchiveProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveProvider {
    /// Construct with id `"archive"`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: ProviderId::new("archive"),
        }
    }
}

/// Register the archive provider on `registry`.
///
/// # Errors
///
/// Duplicate provider id.
pub fn register_archive_provider(registry: &crate::provider::FsProviderRegistry) -> Result<()> {
    registry.register(Arc::new(ArchiveProvider::new()) as Arc<dyn FsProvider>)
}

fn container_and_inner(path: &FsPath) -> Result<(FsPath, String)> {
    let container = path
        .archive_container()
        .ok_or_else(|| FsError::InvalidPath {
            reason: format!("not an archive path: {}", path.as_str()),
        })?;
    let inner = path
        .archive_inner()
        .unwrap_or("")
        .trim_matches('/')
        .to_string();
    Ok((container, inner))
}

fn empty_meta(kind: FsEntryKind, size: u64, modified: Option<chrono::DateTime<Utc>>) -> FsMetadata {
    FsMetadata {
        kind,
        size,
        created: None,
        modified,
        accessed: None,
        readonly: kind == FsEntryKind::File,
        hidden: false,
        system: false,
        mime: None,
        extended: ExtendedAttributes::default(),
    }
}

fn child_entries(
    all: &[ArchiveEntry],
    prefix: &str,
) -> Vec<(String, bool, u64, Option<chrono::DateTime<Utc>>)> {
    let prefix = prefix.trim_matches('/');
    let mut kids: BTreeMap<String, (bool, u64, Option<chrono::DateTime<Utc>>)> = BTreeMap::new();
    for e in all {
        let path = e.path.trim_matches('/');
        let rest = if prefix.is_empty() {
            path
        } else if path == prefix {
            continue;
        } else if let Some(r) = path.strip_prefix(prefix).and_then(|r| r.strip_prefix('/')) {
            r
        } else {
            continue;
        };
        let name = rest.split('/').next().unwrap_or(rest);
        if name.is_empty() {
            continue;
        }
        let is_dir = e.is_dir || rest.contains('/');
        let rec = kids.entry(name.to_string()).or_insert((false, 0, None));
        rec.0 |= is_dir;
        if !is_dir {
            rec.1 = e.size;
            rec.2 = e.modified;
        }
    }
    kids.into_iter()
        .map(|(n, (d, s, m))| (n, d, s, m))
        .collect()
}

#[async_trait]
impl FsProvider for ArchiveProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn scheme(&self) -> &'static str {
        SCHEME_ARCHIVE
    }

    async fn list(&self, path: &FsPath) -> Result<Vec<FsEntry>> {
        let (container, inner) = container_and_inner(path)?;
        let os = container.to_local()?;
        let reader = open_archive(&os)?;
        let all = reader.list().await?;
        let kids = child_entries(&all, &inner);
        let mut out = Vec::with_capacity(kids.len());
        for (name, is_dir, size, modified) in kids {
            let child = if inner.is_empty() {
                FsPath::archive_from_file(&container, &name)?
            } else {
                FsPath::archive_from_file(&container, &format!("{inner}/{name}"))?
            };
            let kind = if is_dir {
                FsEntryKind::Directory
            } else {
                FsEntryKind::File
            };
            out.push(FsEntry {
                path: child,
                name,
                metadata: empty_meta(kind, size, modified),
            });
        }
        Ok(out)
    }

    async fn metadata(&self, path: &FsPath) -> Result<FsMetadata> {
        let (container, inner) = container_and_inner(path)?;
        if inner.is_empty() {
            return Ok(empty_meta(FsEntryKind::Directory, 0, None));
        }
        let os = container.to_local()?;
        let reader = open_archive(&os)?;
        let all = reader.list().await?;
        if let Some(e) = all.iter().find(|e| e.path.trim_matches('/') == inner) {
            let kind = if e.is_dir {
                FsEntryKind::Directory
            } else {
                FsEntryKind::File
            };
            return Ok(empty_meta(kind, e.size, e.modified));
        }
        if all.iter().any(|e| {
            e.path
                .trim_matches('/')
                .strip_prefix(&inner)
                .is_some_and(|r| r.starts_with('/'))
        }) {
            return Ok(empty_meta(FsEntryKind::Directory, 0, None));
        }
        Err(FsError::NotFound(path.as_str().into()))
    }

    async fn exists(&self, path: &FsPath) -> Result<bool> {
        Ok(self.metadata(path).await.is_ok())
    }

    async fn read(&self, path: &FsPath) -> Result<Vec<u8>> {
        let (container, inner) = container_and_inner(path)?;
        if inner.is_empty() {
            return Err(FsError::InvalidPath {
                reason: "cannot read archive root".into(),
            });
        }
        let os = container.to_local()?;
        let reader = open_archive(&os)?;
        reader.read_entry(&inner).await
    }

    async fn read_stream(
        &self,
        path: &FsPath,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
        let bytes = self.read(path).await?;
        Ok(Box::new(Cursor::new(bytes)))
    }

    async fn write(&self, path: &FsPath, bytes: &[u8]) -> Result<()> {
        let (container, inner) = container_and_inner(path)?;
        if inner.is_empty() {
            return Err(FsError::InvalidPath {
                reason: "cannot write archive root".into(),
            });
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("orchid-arc-w-{stamp}"));
        tokio::fs::write(&tmp, bytes).await?;
        let tmp_clone = tmp.clone();
        let container_clone = container.clone();
        let inner_clone = inner.clone();
        let result = tokio::task::spawn_blocking(move || {
            add_named_file_sync(&container_clone, &tmp_clone, &inner_clone)
        })
        .await
        .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?;
        let _ = tokio::fs::remove_file(&tmp).await;
        result
    }

    async fn write_stream(
        &self,
        path: &FsPath,
    ) -> Result<Box<dyn tokio::io::AsyncWrite + Unpin + Send>> {
        let (container, inner) = container_and_inner(path)?;
        if inner.is_empty() {
            return Err(FsError::InvalidPath {
                reason: "cannot write archive root".into(),
            });
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("orchid-arc-ws-{stamp}"));
        let file = tokio::fs::File::create(&tmp).await?;
        Ok(Box::new(ArchiveWriteFinish {
            file: Some(file),
            tmp,
            container,
            inner,
            finished: false,
        }))
    }

    async fn create_dir(&self, path: &FsPath, _recursive: bool) -> Result<()> {
        let (container, inner) = container_and_inner(path)?;
        if inner.is_empty() {
            return Ok(());
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("orchid-arc-d-{stamp}"));
        let dest = root.join(inner.replace('/', std::path::MAIN_SEPARATOR_STR));
        tokio::fs::create_dir_all(&dest).await?;
        let first = inner.split('/').next().unwrap_or(inner.as_str());
        let add = FsPath::from_local(&root.join(first))?;
        let result = add_to_archive(&container, &[add]).await;
        let _ = tokio::fs::remove_dir_all(&root).await;
        result
    }

    async fn rename(&self, _from: &FsPath, _to: &FsPath) -> Result<()> {
        Err(FsError::InvalidPath {
            reason: "rename inside archives is not supported".into(),
        })
    }

    async fn remove(&self, path: &FsPath, _recursive: bool) -> Result<()> {
        let (container, inner) = container_and_inner(path)?;
        if inner.is_empty() {
            return Err(FsError::InvalidPath {
                reason: "cannot delete archive root from inside".into(),
            });
        }
        delete_from_archive(&container, &[inner]).await
    }

    async fn watch(
        &self,
        _path: &FsPath,
        _recursive: bool,
    ) -> Result<Option<Box<dyn FsWatcherHandle>>> {
        Ok(None)
    }

    fn capabilities(&self) -> FsCapabilities {
        FsCapabilities {
            supports_rename: false,
            supports_symlinks: false,
            supports_permissions: false,
            supports_extended_attrs: false,
            supports_native_watch: false,
            case_sensitive: true,
            supports_random_write: false,
        }
    }
}

struct ArchiveWriteFinish {
    file: Option<tokio::fs::File>,
    tmp: std::path::PathBuf,
    container: FsPath,
    inner: String,
    finished: bool,
}

impl ArchiveWriteFinish {
    fn finish_add(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.file = None;
        if self.tmp.is_file() {
            let _ = add_named_file_sync(&self.container, &self.tmp, &self.inner);
        }
        let _ = std::fs::remove_file(&self.tmp);
    }
}

impl tokio::io::AsyncWrite for ArchiveWriteFinish {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.file.as_mut() {
            Some(f) => std::pin::Pin::new(f).poll_write(cx, buf),
            None => std::task::Poll::Ready(Err(std::io::Error::other("archive write closed"))),
        }
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.file.as_mut() {
            Some(f) => std::pin::Pin::new(f).poll_flush(cx),
            None => std::task::Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if let Some(f) = self.file.as_mut() {
            match std::pin::Pin::new(f).poll_shutdown(cx) {
                std::task::Poll::Pending => return std::task::Poll::Pending,
                std::task::Poll::Ready(Err(e)) => return std::task::Poll::Ready(Err(e)),
                std::task::Poll::Ready(Ok(())) => {}
            }
        }
        self.finish_add();
        std::task::Poll::Ready(Ok(()))
    }
}

impl Drop for ArchiveWriteFinish {
    fn drop(&mut self) {
        self.finish_add();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::ops::create_archive;
    use crate::archive::types::CreateArchiveOptions;

    #[tokio::test]
    async fn lists_zip_children() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("hello.txt");
        std::fs::write(&src, b"hi").unwrap();
        let zip = dir.path().join("pack.zip");
        create_archive(
            &FsPath::from_local(&zip).unwrap(),
            &[FsPath::from_local(&src).unwrap()],
            CreateArchiveOptions::default(),
        )
        .await
        .unwrap();
        let root = FsPath::archive_from_file(&FsPath::from_local(&zip).unwrap(), "").unwrap();
        let kids = ArchiveProvider::new().list(&root).await.unwrap();
        assert!(kids.iter().any(|e| e.name == "hello.txt"));
    }
}
