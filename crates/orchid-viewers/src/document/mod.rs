//! DOCX-compatible document viewer/editor (Tier 1 rich text).

pub mod cursor;
pub mod layout;
pub mod model;
pub mod ooxml;
pub mod undo;

use std::any::Any;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};

use crate::error::{Result, ViewerError};
use crate::snapshot::{DocumentSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

pub use cursor::{Cursor, Selection};
pub use model::{
    Alignment, Block, Document, ImageFormat, InlineImage, ListKind, OpaqueXmlNode, PageSetup,
    Paragraph, Run, RunStyle, Table, TableCell, TableRow,
};
pub use undo::{EditCommand, RunStylePatch, UndoStack};

/// Soft ceiling for DOCX payloads accepted by the viewer (128 MiB).
pub const DEFAULT_SIZE_LIMIT: u64 = 128 * 1024 * 1024;

/// Document viewer / editor for `.docx` (Office Open XML).
pub struct DocumentViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    document: RwLock<Option<Document>>,
    undo: Mutex<UndoStack>,
    warnings: RwLock<Vec<String>>,
    registry: RwLock<Option<Arc<orchid_fs::FsProviderRegistry>>>,
    size_limit: u64,
}

impl std::fmt::Debug for DocumentViewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentViewer")
            .field(
                "path",
                &self.path.read().as_ref().map(|p| p.as_str().to_string()),
            )
            .finish_non_exhaustive()
    }
}

impl Default for DocumentViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentViewer {
    /// Build an empty document viewer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: RwLock::new(None),
            document: RwLock::new(None),
            undo: Mutex::new(UndoStack::new()),
            warnings: RwLock::new(Vec::new()),
            registry: RwLock::new(None),
            size_limit: DEFAULT_SIZE_LIMIT,
        }
    }

    /// Borrow the loaded document model (for tests / UI commands).
    #[must_use]
    pub fn document(&self) -> parking_lot::RwLockReadGuard<'_, Option<Document>> {
        self.document.read()
    }

    /// Mutable borrow of the loaded document.
    pub fn document_mut(&self) -> parking_lot::RwLockWriteGuard<'_, Option<Document>> {
        self.document.write()
    }

    /// Apply an edit command and push it onto the undo stack.
    ///
    /// # Errors
    ///
    /// Returns [`ViewerError::EditOutOfBounds`] when the command targets an
    /// invalid range, or [`ViewerError::DocumentNotOpen`] when nothing is loaded.
    pub fn apply(&self, cmd: EditCommand) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        self.undo.lock().push(doc, cmd)?;
        Ok(())
    }

    /// Undo the last edit.
    pub fn undo(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        self.undo.lock().undo(doc)?;
        Ok(())
    }

    /// Redo the last undone edit.
    pub fn redo(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        self.undo.lock().redo(doc)?;
        Ok(())
    }
}

#[async_trait]
impl Viewer for DocumentViewer {
    fn type_id(&self) -> &'static str {
        "document"
    }

    async fn open(
        &mut self,
        path: orchid_fs::FsPath,
        registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()> {
        let provider = registry
            .for_path(&path)
            .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;

        let os_path = if path.is_local() {
            path.to_local()?
        } else {
            // Remote: materialise into a temp file for zip/xml parsing.
            let bytes = provider.read(&path).await?;
            if bytes.len() as u64 > self.size_limit {
                return Err(ViewerError::FileTooLarge {
                    size: bytes.len() as u64,
                    limit: self.size_limit,
                });
            }
            let tmp = std::env::temp_dir().join(format!(
                "orchid-docx-{}.docx",
                uuid::Uuid::new_v4()
            ));
            tokio::fs::write(&tmp, &bytes).await?;
            let doc = Document::from_docx(&tmp).await?;
            let _ = tokio::fs::remove_file(&tmp).await;
            *self.document.write() = Some(doc);
            *self.path.write() = Some(path);
            *self.registry.write() = Some(registry);
            *self.undo.lock() = UndoStack::new();
            *self.warnings.write() = Vec::new();
            return Ok(());
        };

        let meta = tokio::fs::metadata(&os_path).await?;
        if meta.len() > self.size_limit {
            return Err(ViewerError::FileTooLarge {
                size: meta.len(),
                limit: self.size_limit,
            });
        }

        let doc = Document::from_docx(Path::new(&os_path)).await?;
        *self.document.write() = Some(doc);
        *self.path.write() = Some(path);
        *self.registry.write() = Some(registry);
        *self.undo.lock() = UndoStack::new();
        *self.warnings.write() = Vec::new();
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        *self.document.write() = None;
        *self.path.write() = None;
        *self.registry.write() = None;
        *self.undo.lock() = UndoStack::new();
        *self.warnings.write() = Vec::new();
        Ok(())
    }

    fn snapshot(&self) -> ViewerSnapshot {
        let path_display = self
            .path
            .read()
            .as_ref()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return ViewerSnapshot::Loading { path_display };
        };
        let dirty = self.undo.lock().is_dirty();
        let warnings = self.warnings.read().clone();
        let plain_text = doc.plain_text();
        let block_count = doc.blocks.len() as u32;
        drop(doc_guard);
        ViewerSnapshot::Document(DocumentSnapshot {
            path_display,
            dirty,
            block_count,
            plain_text: Arc::from(plain_text.as_str()),
            warnings,
            info_text: String::new(),
        })
    }

    fn is_dirty(&self) -> bool {
        self.undo.lock().is_dirty()
    }

    async fn save(&mut self) -> Result<()> {
        let path = self
            .path
            .read()
            .clone()
            .ok_or(ViewerError::DocumentNotOpen)?;
        if !path.is_local() {
            return Err(ViewerError::DocumentSave(String::from(
                "saving remote documents is not supported yet",
            )));
        }
        let os_path = path.to_local()?;
        let doc = self
            .document
            .read()
            .clone()
            .ok_or(ViewerError::DocumentNotOpen)?;
        ooxml::container::save_document(&doc, Path::new(&os_path)).await?;
        self.undo.lock().mark_clean();
        Ok(())
    }

    fn current_path(&self) -> Option<&orchid_fs::FsPath> {
        // Safety: path is only replaced under exclusive access via open/close;
        // callers hold `&self` so the Option stays stable for the call duration.
        // We expose via a leaked-lifetime pattern used elsewhere — return None
        // and let UI use snapshot path_display when needed. Prefer owned clone
        // via snapshot; trait requires Option<&FsPath>.
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// Implement current_path properly with a stored path we can return.
// The RwLock prevents returning a reference — mirror TextViewer pattern.
impl DocumentViewer {
    /// Path of the open document, if any.
    #[must_use]
    pub fn path_clone(&self) -> Option<orchid_fs::FsPath> {
        self.path.read().clone()
    }
}
