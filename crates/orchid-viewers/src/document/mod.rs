//! DOCX-compatible document viewer/editor (Tier 1 rich text).

pub mod cursor;
mod edit_helpers;
mod format;
pub mod layout;
pub mod model;
pub mod ooxml;
pub mod orchid_io;
mod preview;
pub mod sample;
pub mod table_edit;
pub mod undo;
mod viewer_impl;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};

pub use cursor::{
    adjacent_cell_cursor, adjacent_in_cell, cursor_from_plain_offset,
    expand_selection_to_hyperlink_span, hyperlink_at_cursor, is_image_cursor, is_safe_external_url,
    normalize_external_link_url, normalize_internal_bookmark, paragraph_cursors_in_selection,
    paragraph_indices_in_selection, paragraph_mut, paragraph_mut_in_blocks, paragraph_ref,
    plain_offset_from_cursor, selection_from_plain_offsets, CellPath, Cursor, Selection,
};
pub use edit_helpers::resolve_font_family_slug;
pub use layout::{DocumentLayout, PreviewInsets, DEFAULT_PREVIEW_WIDTH};
pub use model::{
    Alignment, Block, Bookmark, CellImage, CommentRange, DocComment, DocField, Document, Hyperlink,
    ImageFormat, InlineImage, LineSpacingRule, ListKind, NamedCharacterStyle, NamedParagraphStyle,
    OpaqueXmlNode, PageSetup, Paragraph, Run, RunStyle, SectionBreakType, Table, TableCell,
    TableRow, VMerge,
};
pub use orchid_io::{
    is_docx_raw_meta, is_orchid_identity_error, is_orchid_path, looks_like_orchid,
    materialize_orchid_raw_temp, open_document_from_orchid, open_document_from_orchid_with_store,
    peek_orchid_raw_meta, pick_document_save_path, save_document_as_linked_orchid,
    save_document_as_orchid,
};
pub use sample::{
    create_sample_docx, create_sample_orchid, create_sample_orchid_with_store, sample_document,
};
pub use undo::{EditCommand, RunStylePatch, UndoStack};

/// Soft ceiling for DOCX payloads accepted by the viewer (128 MiB).
pub const DEFAULT_SIZE_LIMIT: u64 = 128 * 1024 * 1024;

pub(super) struct PreviewState {
    /// Content column width (CSS px), excluding page margins.
    width: f32,
    /// Last Slint viewport width used to derive [`Self::width`] (`0` = unknown).
    viewport_px: f32,
    bytes: Arc<Vec<u8>>,
    width_px: u32,
    height_px: u32,
    valid: bool,
    /// Plain-text selection baked into `bytes` (`start == end` → caret only).
    sel_start: usize,
    sel_end: usize,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            width: DEFAULT_PREVIEW_WIDTH,
            viewport_px: 0.0,
            bytes: Arc::new(Vec::new()),
            width_px: 0,
            height_px: 0,
            valid: false,
            sel_start: 0,
            sel_end: 0,
        }
    }
}

/// Document viewer / editor for `.docx` (Office Open XML).
pub struct DocumentViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    document: RwLock<Option<Document>>,
    undo: Mutex<UndoStack>,
    warnings: RwLock<Vec<String>>,
    registry: RwLock<Option<Arc<orchid_fs::FsProviderRegistry>>>,
    size_limit: u64,
    layout: Mutex<DocumentLayout>,
    preview: Mutex<PreviewState>,
    source_mode: RwLock<bool>,
    selection: Mutex<Selection>,
    /// Plain-text offset captured on preview pointer-down (drag selection).
    preview_drag_anchor: Mutex<Option<usize>>,
    /// Multi-click tracking for word (2×) / paragraph (3×) select.
    preview_click: Mutex<PreviewClickState>,
    /// Bumped when [`Self::preview_find`] selects a match (or reports no match).
    find_gen: Mutex<i32>,
    find_anchor: Mutex<i32>,
    find_cursor: Mutex<i32>,
    /// 1-based index of the current find match (`0` when none).
    find_match_index: Mutex<i32>,
    /// Total non-overlapping matches for the last query (`0` when none).
    find_match_count: Mutex<i32>,
    /// Preview image Y (CSS px) to scroll to for the current find match (`-1` = none).
    find_scroll_y_px: Mutex<i32>,
    /// Preview display zoom factor (`1.0` = 100%; layout width unchanged).
    preview_zoom: Mutex<f32>,
    /// Preview pointer is over an external hyperlink.
    link_hover: Mutex<bool>,
    /// App `ChunkStore` for linked `.orchid` open/save (None → sealed only).
    chunk_store: Option<Arc<orchid_crypto::ChunkStore>>,
    /// Last known `.orchid` file UUID (preserved across linked generation bumps).
    orchid_file_uuid: Mutex<Option<[u8; 16]>>,
    /// Last known TOC generation for the open `.orchid`.
    orchid_generation: Mutex<u64>,
    /// Whether the open `.orchid` is linked (ChunkStore payloads).
    orchid_linked: Mutex<bool>,
    /// C2PA verify when Provenance is present (`None` = no C2PA region).
    orchid_c2pa_ok: Mutex<Option<bool>>,
    /// Age identity used to decrypt (and re-encrypt on save) a private `.orchid`.
    orchid_decrypt: Mutex<Option<orchid_crypto::Identity>>,
    /// When true, next `.orchid` sealed save names Raw `original.docx` (DOCX import).
    prefer_original_docx_name: Mutex<bool>,
}

/// Result of [`DocumentViewer::preview_pointer`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreviewPointerOutcome {
    /// Safe external URL to open (`http`/`https`/`mailto`); `None` if none.
    pub open_url: Option<String>,
    /// Whether the UI should refresh the document snapshot.
    pub refresh: bool,
}

#[derive(Default)]
pub(super) struct PreviewClickState {
    count: u8,
    last_at: Option<Instant>,
    last_offset: usize,
}

pub(super) const MULTI_CLICK_GAP: Duration = Duration::from_millis(500);

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
            layout: Mutex::new(DocumentLayout::new()),
            preview: Mutex::new(PreviewState::default()),
            source_mode: RwLock::new(false),
            selection: Mutex::new(Selection {
                anchor: Cursor::default(),
                head: Cursor::default(),
            }),
            preview_drag_anchor: Mutex::new(None),
            preview_click: Mutex::new(PreviewClickState::default()),
            find_gen: Mutex::new(0),
            find_anchor: Mutex::new(0),
            find_cursor: Mutex::new(0),
            find_match_index: Mutex::new(0),
            find_match_count: Mutex::new(0),
            find_scroll_y_px: Mutex::new(-1),
            preview_zoom: Mutex::new(1.0),
            link_hover: Mutex::new(false),
            chunk_store: None,
            orchid_file_uuid: Mutex::new(None),
            orchid_generation: Mutex::new(0),
            orchid_linked: Mutex::new(false),
            orchid_c2pa_ok: Mutex::new(None),
            orchid_decrypt: Mutex::new(None),
            prefer_original_docx_name: Mutex::new(false),
        }
    }

    /// Inject the content-addressed store used for linked `.orchid` I/O.
    pub fn set_chunk_store(&mut self, store: Arc<orchid_crypto::ChunkStore>) {
        self.chunk_store = Some(store);
    }

    /// Set the age identity for opening/saving an encrypted `.orchid`.
    pub fn set_decrypt_identity(&self, identity: Option<orchid_crypto::Identity>) {
        *self.orchid_decrypt.lock() = identity;
    }

    fn decrypt_identity(&self) -> Option<orchid_crypto::Identity> {
        self.orchid_decrypt.lock().clone()
    }

    fn remember_orchid_identity(&self, path: &Path) {
        if let Ok(meta) = orchid_io::orchid_open_meta(path) {
            *self.orchid_file_uuid.lock() = Some(meta.file_uuid);
            *self.orchid_generation.lock() = meta.generation;
            *self.orchid_linked.lock() = meta.linked;
            *self.orchid_c2pa_ok.lock() = meta.c2pa_ok;
        }
    }

    fn clear_orchid_identity(&self) {
        *self.orchid_file_uuid.lock() = None;
        *self.orchid_generation.lock() = 0;
        *self.orchid_linked.lock() = false;
        *self.orchid_c2pa_ok.lock() = None;
        *self.orchid_decrypt.lock() = None;
    }

    fn set_prefer_original_docx_name(&self, prefer: bool) {
        *self.prefer_original_docx_name.lock() = prefer;
    }

    fn take_orchid_raw_name(&self) -> &'static str {
        if *self.prefer_original_docx_name.lock() {
            "original.docx"
        } else {
            "document.docx"
        }
    }
}
