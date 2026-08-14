//! HTML viewer — source preview plus system-browser handoff.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::error::{Result, ViewerError};
use crate::snapshot::{HtmlSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

const SOURCE_LIMIT: usize = 256 * 1024;

/// Whether `ext` (lowercase, no dot) is an HTML file.
#[must_use]
pub fn is_html_file_extension(ext: &str) -> bool {
    matches!(ext, "html" | "htm" | "xhtml")
}

/// HTML source viewer.
#[derive(Debug, Default)]
pub struct HtmlViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    source: RwLock<Arc<str>>,
}

impl HtmlViewer {
    /// Empty HTML viewer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Viewer for HtmlViewer {
    fn type_id(&self) -> &'static str {
        "html"
    }

    async fn open(
        &mut self,
        path: orchid_fs::FsPath,
        registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()> {
        let provider = registry
            .for_path(&path)
            .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;
        let bytes = provider.read(&path).await.map_err(ViewerError::Fs)?;
        let slice = &bytes[..bytes.len().min(SOURCE_LIMIT)];
        let text = String::from_utf8_lossy(slice);
        *self.source.write() = Arc::from(text.as_ref());
        *self.path.write() = Some(path);
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        *self.path.write() = None;
        *self.source.write() = Arc::from("");
        Ok(())
    }

    fn snapshot(&self) -> ViewerSnapshot {
        let path_display = self
            .path
            .read()
            .as_ref()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();
        ViewerSnapshot::Html(HtmlSnapshot {
            path_display,
            source_preview: Arc::clone(&*self.source.read()),
            info_text: String::new(),
        })
    }

    fn current_path(&self) -> Option<&orchid_fs::FsPath> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
