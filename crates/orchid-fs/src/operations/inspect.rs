//! Local file-property snapshot for the file-manager report.

use crate::entry::{FsEntryKind, FsMetadata};
use crate::error::Result;
use crate::path::FsPath;
use crate::provider::FsProviderRegistry;

/// OS-level properties used by the Properties report.
#[derive(Debug, Clone)]
pub struct FileProperties {
    /// Display name.
    pub name: String,
    /// Canonical Orchid path.
    pub path: String,
    /// File / folder / symlink.
    pub kind: FsEntryKind,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Creation time.
    pub created: Option<chrono::DateTime<chrono::Utc>>,
    /// Last write time.
    pub modified: Option<chrono::DateTime<chrono::Utc>>,
    /// Last access time.
    pub accessed: Option<chrono::DateTime<chrono::Utc>>,
    /// Read-only bit.
    pub readonly: bool,
    /// Hidden bit.
    pub hidden: bool,
    /// System bit (Windows).
    pub system: bool,
    /// Guessed MIME type.
    pub mime: Option<String>,
}

/// Read properties through the provider registry.
///
/// # Errors
///
/// Missing provider or metadata failure.
pub async fn inspect_path(registry: &FsProviderRegistry, path: &FsPath) -> Result<FileProperties> {
    let provider = registry
        .for_path(path)
        .ok_or_else(|| crate::error::FsError::ProviderNotMounted(path.to_string()))?;
    let meta: FsMetadata = provider.metadata(path).await?;
    Ok(FileProperties {
        name: path
            .file_name()
            .map(String::from)
            .unwrap_or_else(|| path.as_str().to_string()),
        path: path.as_str().to_string(),
        kind: meta.kind,
        size: meta.size,
        created: meta.created,
        modified: meta.modified,
        accessed: meta.accessed,
        readonly: meta.readonly,
        hidden: meta.hidden,
        system: meta.system,
        mime: meta.mime,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{FsProvider, LocalProvider};
    use std::sync::Arc;

    #[tokio::test]
    async fn inspects_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.txt");
        std::fs::write(&file, b"hi").unwrap();
        let registry = FsProviderRegistry::new();
        registry
            .register(Arc::new(LocalProvider::new()) as Arc<dyn FsProvider>)
            .unwrap();
        let path = FsPath::from_local(&file).unwrap();
        let props = inspect_path(&registry, &path).await.unwrap();
        assert_eq!(props.name, "note.txt");
        assert_eq!(props.size, 2);
        assert_eq!(props.kind, FsEntryKind::File);
    }
}
