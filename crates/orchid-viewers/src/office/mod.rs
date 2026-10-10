//! Spreadsheet and presentation preview.
//!
//! Spreadsheets become a sheet and cell table. One stored cell can be
//! written back into the package. A formula cell is left unchanged. After a
//! value edit, the functions named in the spreadsheet section of
//! `docs/user/viewers.md` are recalculated on that sheet. Other formulas
//! keep their stored value. Presentations become one HTML card per slide.

use std::any::Any;
use std::io::{Cursor, Read, Seek, Write};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use zip::ZipArchive;

use crate::error::{Result, ViewerError};
use crate::snapshot::{HtmlSnapshot, SheetCell, SheetPage, SheetSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

const MAX_ROWS: usize = 400;
const MAX_COLS: usize = 32;

/// Spreadsheet and presentation preview.
#[derive(Debug, Default)]
pub struct OfficeViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    html: RwLock<Arc<str>>,
    info: RwLock<String>,
    sheets: RwLock<Arc<Vec<SheetPage>>>,
    /// `true` for a slide deck. Workbooks use [`Self::sheets`].
    slides: RwLock<bool>,
}

impl OfficeViewer {
    /// Empty preview.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace one stored cell and write the package back to `path`.
    ///
    /// A formula cell is refused. Drawings and the other zip parts are
    /// copied through. Nothing is recalculated. The in-memory table is
    /// refreshed only when this viewer still has the same path open.
    ///
    /// # Errors
    ///
    /// A slide deck, a missing sheet or cell, a formula cell, or a
    /// provider write failure.
    pub async fn edit_cell(
        &self,
        registry: Arc<orchid_fs::FsProviderRegistry>,
        sheet: &str,
        address: &str,
        text: &str,
    ) -> Result<()> {
        if *self.slides.read() {
            return Err(ViewerError::SheetEdit("viewer-sheet-not-workbook".into()));
        }
        let path = self
            .path
            .read()
            .clone()
            .ok_or_else(|| ViewerError::SheetEdit("viewer-sheet-not-workbook".into()))?;
        let provider = registry
            .for_path(&path)
            .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;
        let bytes = provider.read(&path).await?;
        let sheet = sheet.to_string();
        let address = address.to_string();
        let text = text.to_string();
        let saved =
            tokio::task::spawn_blocking(move || set_sheet_cell(&bytes, &sheet, &address, &text))
                .await
                .map_err(|err| ViewerError::SheetEdit(err.to_string()))?
                .map_err(ViewerError::SheetEdit)?;
        let tmp = orchid_fs::FsPath::new(format!("{}.orchid-save", path.as_str()))?;
        provider.write(&tmp, &saved).await?;
        provider.rename(&tmp, &path).await?;
        let preview = tokio::task::spawn_blocking(move || render_office(&saved, false))
            .await
            .map_err(|err| ViewerError::SheetEdit(err.to_string()))?
            .map_err(ViewerError::DocumentParse)?;
        if self.path.read().as_ref().map(orchid_fs::FsPath::as_str) != Some(path.as_str()) {
            return Ok(());
        }
        if let OfficePreview::Sheets(book) = preview {
            *self.html.write() = Arc::from("");
            *self.info.write() = book.info;
            *self.sheets.write() = Arc::new(book.sheets);
            *self.slides.write() = false;
        }
        Ok(())
    }
}

#[async_trait]
impl Viewer for OfficeViewer {
    fn type_id(&self) -> &'static str {
        "office"
    }

    async fn open(
        &mut self,
        path: orchid_fs::FsPath,
        registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()> {
        let provider = registry
            .for_path(&path)
            .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;
        let bytes = provider.read(&path).await?;
        let slides = is_slide_path(&path);
        let preview = tokio::task::spawn_blocking(move || render_office(&bytes, slides))
            .await
            .map_err(|err| ViewerError::DocumentParse(err.to_string()))?
            .map_err(ViewerError::DocumentParse)?;
        match preview {
            OfficePreview::Slides(preview) => {
                *self.html.write() = Arc::from(preview.html);
                *self.info.write() = preview.info;
                *self.sheets.write() = Arc::new(Vec::new());
                *self.slides.write() = true;
            }
            OfficePreview::Sheets(book) => {
                *self.html.write() = Arc::from("");
                *self.info.write() = book.info;
                *self.sheets.write() = Arc::new(book.sheets);
                *self.slides.write() = false;
            }
        }
        *self.path.write() = Some(path);
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        *self.path.write() = None;
        *self.html.write() = Arc::from("");
        *self.info.write() = String::new();
        *self.sheets.write() = Arc::new(Vec::new());
        *self.slides.write() = false;
        Ok(())
    }

    fn snapshot(&self) -> ViewerSnapshot {
        let path_display = self
            .path
            .read()
            .as_ref()
            .map(|path| path.as_str().to_string())
            .unwrap_or_default();
        if *self.slides.read() {
            ViewerSnapshot::Html(HtmlSnapshot {
                path_display,
                source_preview: Arc::clone(&self.html.read()),
                local_path: None,
                info_text: self.info.read().clone(),
            })
        } else {
            ViewerSnapshot::Sheet(SheetSnapshot {
                path_display,
                info_text: self.info.read().clone(),
                sheets: Arc::clone(&self.sheets.read()),
            })
        }
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

include!("calc.rs");
include!("preview.rs");
include!("xml.rs");

#[cfg(test)]
include!("tests.rs");
