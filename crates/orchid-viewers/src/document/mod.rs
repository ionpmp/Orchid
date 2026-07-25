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

    /// Whether undo is available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.undo.lock().can_undo()
    }

    /// Whether redo is available.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.undo.lock().can_redo()
    }

    /// Replace body content from plain text (paragraphs separated by blank lines).
    ///
    /// Preserves [`PageSetup`] and retained package parts. Rich formatting of
    /// previous runs is not preserved across a full text push.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`] when nothing is loaded.
    pub fn replace_plain_text(&self, text: &str) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        // Skip no-op pushes so typing churn does not stack identical bodies.
        let next = plain_text_to_blocks_preserving(doc, text);
        if doc.blocks == next {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        Ok(())
    }

    /// Toggle a boolean character style on every run in the document.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_style_all(&self, which: char) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        let currently_on = first_run_style(doc)
            .map(|s| match which {
                'b' => s.bold,
                'i' => s.italic,
                'u' => s.underline,
                _ => false,
            })
            .unwrap_or(false);
        let patch = match which {
            'b' => RunStylePatch {
                bold: Some(!currently_on),
                ..Default::default()
            },
            'i' => RunStylePatch {
                italic: Some(!currently_on),
                ..Default::default()
            },
            'u' => RunStylePatch {
                underline: Some(!currently_on),
                ..Default::default()
            },
            _ => return Ok(()),
        };
        let mut next = doc.blocks.clone();
        apply_style_patch_all_blocks(&mut next, &patch);
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        Ok(())
    }

    /// Set alignment on every paragraph block.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_alignment_all(&self, alignment: Alignment) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        let mut next = doc.blocks.clone();
        for block in &mut next {
            if let Block::Paragraph(p) = block {
                p.alignment = alignment;
            }
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        Ok(())
    }

    /// Set list kind on every paragraph block.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_list_all(&self, kind: ListKind) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        let mut next = doc.blocks.clone();
        for block in &mut next {
            if let Block::Paragraph(p) = block {
                p.list = kind;
            }
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        Ok(())
    }

    /// Toggle bullet or numbered list on every paragraph (off when already that kind).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_list_all(&self, kind: ListKind) -> Result<()> {
        let current = self
            .document
            .read()
            .as_ref()
            .and_then(first_paragraph)
            .map(|p| p.list)
            .unwrap_or(ListKind::None);
        let next = if current == kind {
            ListKind::None
        } else {
            kind
        };
        self.set_list_all(next)
    }
}

fn plain_text_to_blocks_preserving(doc: &Document, text: &str) -> Vec<Block> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = if normalized.is_empty() {
        vec![""]
    } else {
        normalized.split('\n').collect()
    };
    lines
        .into_iter()
        .enumerate()
        .map(|(idx, line)| {
            if let Some(Block::Paragraph(prev)) = doc.blocks.get(idx) {
                let style = prev
                    .runs
                    .first()
                    .map(|r| r.style.clone())
                    .unwrap_or_default();
                Block::Paragraph(Paragraph {
                    runs: vec![Run {
                        text: line.to_string(),
                        style,
                    }],
                    alignment: prev.alignment,
                    list: prev.list,
                    list_level: prev.list_level,
                    num_id: prev.num_id,
                    unsupported: prev.unsupported.clone(),
                })
            } else {
                Block::Paragraph(Paragraph {
                    runs: vec![Run {
                        text: line.to_string(),
                        style: RunStyle::default(),
                    }],
                    ..Default::default()
                })
            }
        })
        .collect()
}

fn first_paragraph(doc: &Document) -> Option<&Paragraph> {
    doc.blocks.iter().find_map(|b| match b {
        Block::Paragraph(p) => Some(p),
        _ => None,
    })
}

fn first_run_style(doc: &Document) -> Option<&RunStyle> {
    first_paragraph(doc).and_then(|p| p.runs.first().map(|r| &r.style))
}

fn apply_style_patch_all_blocks(blocks: &mut [Block], patch: &RunStylePatch) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                for run in &mut p.runs {
                    patch.apply_to(&mut run.style);
                }
            }
            Block::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        for p in &mut cell.paragraphs {
                            for run in &mut p.runs {
                                patch.apply_to(&mut run.style);
                            }
                        }
                    }
                }
            }
            Block::Image(_) => {}
        }
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
        let undo = self.undo.lock();
        let dirty = undo.is_dirty();
        let can_undo = undo.can_undo();
        let can_redo = undo.can_redo();
        drop(undo);
        let warnings = self.warnings.read().clone();
        let plain_text = doc.plain_text();
        let block_count = doc.blocks.len() as u32;
        let (bold, italic, underline, alignment, list_kind) =
            if let Some(p) = first_paragraph(doc) {
                let style = p.runs.first().map(|r| &r.style);
                (
                    style.is_some_and(|s| s.bold),
                    style.is_some_and(|s| s.italic),
                    style.is_some_and(|s| s.underline),
                    match p.alignment {
                        Alignment::Left => 0,
                        Alignment::Center => 1,
                        Alignment::Right => 2,
                        Alignment::Justify => 3,
                    },
                    match p.list {
                        ListKind::None => 0,
                        ListKind::Bullet => 1,
                        ListKind::Numbered => 2,
                    },
                )
            } else {
                (false, false, false, 0, 0)
            };
        drop(doc_guard);
        ViewerSnapshot::Document(DocumentSnapshot {
            path_display,
            dirty,
            block_count,
            plain_text: Arc::from(plain_text.as_str()),
            warnings,
            info_text: String::new(),
            bold,
            italic,
            underline,
            alignment,
            list_kind,
            can_undo,
            can_redo,
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
