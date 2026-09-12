//! [`Viewer`] trait impl and save-as.

use std::any::Any;
use std::path::Path;

use super::edit_helpers::*;
use super::*;
use crate::error::{Result, ViewerError};
use crate::snapshot::{DocumentSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;
use async_trait::async_trait;

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
            let tmp =
                std::env::temp_dir().join(format!("orchid-docx-{}.docx", uuid::Uuid::new_v4()));
            tokio::fs::write(&tmp, &bytes).await?;
            let doc = if orchid_io::looks_like_orchid(&bytes) {
                let orchid_tmp = std::env::temp_dir()
                    .join(format!("orchid-open-{}.orchid", uuid::Uuid::new_v4()));
                tokio::fs::write(&orchid_tmp, &bytes).await?;
                let id = self.decrypt_identity();
                let opened = orchid_io::open_document_from_orchid_with_store(
                    &orchid_tmp,
                    self.chunk_store.as_deref(),
                    id.as_ref(),
                )
                .await?;
                self.remember_orchid_identity(&orchid_tmp);
                self.set_prefer_original_docx_name(false);
                let _ = tokio::fs::remove_file(&orchid_tmp).await;
                opened
            } else {
                self.clear_orchid_identity();
                self.set_prefer_original_docx_name(true);
                Document::from_docx(&tmp).await?
            };
            let _ = tokio::fs::remove_file(&tmp).await;
            *self.document.write() = Some(doc);
            *self.path.write() = Some(path);
            *self.registry.write() = Some(registry);
            *self.undo.lock() = UndoStack::new();
            *self.warnings.write() = Vec::new();
            *self.preview.lock() = PreviewState::default();
            *self.source_mode.write() = false;
            *self.selection.lock() = Selection {
                anchor: Cursor::default(),
                head: Cursor::default(),
            };
            return Ok(());
        };

        let meta = tokio::fs::metadata(&os_path).await?;
        if meta.len() > self.size_limit {
            return Err(ViewerError::FileTooLarge {
                size: meta.len(),
                limit: self.size_limit,
            });
        }

        let os = Path::new(&os_path);
        let doc = if orchid_io::is_orchid_path(os) {
            let id = self.decrypt_identity();
            let opened = orchid_io::open_document_from_orchid_with_store(
                os,
                self.chunk_store.as_deref(),
                id.as_ref(),
            )
            .await?;
            self.remember_orchid_identity(os);
            self.set_prefer_original_docx_name(false);
            opened
        } else {
            self.clear_orchid_identity();
            let is_docx = os
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("docx"));
            self.set_prefer_original_docx_name(is_docx);
            Document::from_docx(os).await?
        };
        *self.document.write() = Some(doc);
        *self.path.write() = Some(path);
        *self.registry.write() = Some(registry);
        *self.undo.lock() = UndoStack::new();
        *self.warnings.write() = Vec::new();
        *self.preview.lock() = PreviewState::default();
        *self.source_mode.write() = false;
        *self.selection.lock() = Selection {
            anchor: Cursor::default(),
            head: Cursor::default(),
        };
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        self.clear_orchid_identity();
        *self.document.write() = None;
        *self.path.write() = None;
        *self.registry.write() = None;
        *self.undo.lock() = UndoStack::new();
        *self.warnings.write() = Vec::new();
        *self.preview.lock() = PreviewState::default();
        *self.source_mode.write() = false;
        *self.selection.lock() = Selection {
            anchor: Cursor::default(),
            head: Cursor::default(),
        };
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
        let (word_count, char_count) = crate::document::model::text_stats(&plain_text);
        let comment_count = doc.comments.len() as u32;
        let sel = *self.selection.lock();
        let caret = sel.normalized().0;
        let style = style_at_cursor(doc, caret);
        let link_url = hyperlink_at_cursor(doc, sel.head)
            .map(|hl| hl.display_target())
            .unwrap_or_default();
        let para = paragraph_ref(doc, caret).or_else(|| first_paragraph(doc));
        let (
            bold,
            italic,
            underline,
            strikethrough,
            double_strikethrough,
            highlight,
            all_caps,
            small_caps,
            vanish,
            shadow,
            emboss,
            imprint,
            superscript,
            subscript,
            font_size_pt,
            font_family,
            color_rgb,
            alignment,
            list_kind,
        ) = (
            style.as_ref().is_some_and(|s| s.bold),
            style.as_ref().is_some_and(|s| s.italic),
            style.as_ref().is_some_and(|s| s.underline),
            style.as_ref().is_some_and(|s| s.strikethrough),
            style.as_ref().is_some_and(|s| s.double_strikethrough),
            style.as_ref().is_some_and(|s| s.highlight),
            style.as_ref().is_some_and(|s| s.all_caps),
            style.as_ref().is_some_and(|s| s.small_caps),
            style.as_ref().is_some_and(|s| s.vanish),
            style.as_ref().is_some_and(|s| s.shadow),
            style.as_ref().is_some_and(|s| s.emboss),
            style.as_ref().is_some_and(|s| s.imprint),
            style.as_ref().is_some_and(|s| s.superscript),
            style.as_ref().is_some_and(|s| s.subscript),
            style.as_ref().and_then(|s| s.font_size_pt).unwrap_or(0.0),
            style
                .as_ref()
                .and_then(|s| s.font_family.clone())
                .unwrap_or_default(),
            style
                .as_ref()
                .and_then(|s| s.color)
                .map(|[r, g, b]| (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
                .unwrap_or(0),
            match para.map(|p| p.alignment).unwrap_or_default() {
                Alignment::Left => 0,
                Alignment::Center => 1,
                Alignment::Right => 2,
                Alignment::Justify => 3,
            },
            match para.map(|p| p.list).unwrap_or_default() {
                ListKind::None => 0,
                ListKind::Bullet => 1,
                ListKind::Numbered => 2,
            },
        );
        let shade = if let Some(path) = caret.cell {
            matches!(
                doc.blocks.get(caret.block_idx),
                Some(Block::Table(t))
                    if t.rows
                        .get(path.row)
                        .and_then(|r| r.cells.get(path.col))
                        .is_some_and(|c| c.shade_fill.is_some())
            )
        } else {
            para.is_some_and(|p| p.shade_fill.is_some())
        };
        let border_bottom = if let Some(path) = caret.cell {
            matches!(
                doc.blocks.get(caret.block_idx),
                Some(Block::Table(t))
                    if t.rows
                        .get(path.row)
                        .and_then(|r| r.cells.get(path.col))
                        .is_some_and(|c| c.border_sides != 0)
            )
        } else {
            para.is_some_and(|p| p.border_sides != 0)
        };
        let keep_next = para.is_some_and(|p| p.keep_next);
        let keep_lines = para.is_some_and(|p| p.keep_lines);
        let widow_control = para.is_some_and(|p| p.widow_control);
        let contextual_spacing = para.is_some_and(|p| p.contextual_spacing);
        let bidi = para.is_some_and(|p| p.bidi);
        let suppress_auto_hyphens = para.is_some_and(|p| p.suppress_auto_hyphens);
        let character_style_id = run_style_id_at_cursor(doc, sel.head).unwrap_or_default();
        let section_break_continuous = paragraph_ref(doc, sel.head)
            .and_then(|p| p.section_properties.as_ref())
            .is_some_and(|ps| ps.section_break == SectionBreakType::Continuous);
        let outline_level = para
            .and_then(|p| p.outline_level)
            .map(|lvl| i32::from(lvl))
            .unwrap_or(-1);

        let source_mode = *self.source_mode.read();
        let (sel_start, sel_end) = {
            let (a, b) = sel.normalized();
            (
                plain_offset_from_cursor(doc, a),
                plain_offset_from_cursor(doc, b),
            )
        };
        let (preview_rgba, preview_width_px, preview_height_px) = {
            let mut prev = self.preview.lock();
            let sel_changed = prev.sel_start != sel_start || prev.sel_end != sel_end;
            if !prev.valid || sel_changed {
                let mut layout = self.layout.lock();
                if !prev.valid {
                    layout.drop_render_scene();
                }
                let file_name = self.path.read().as_ref().and_then(|p| {
                    std::path::Path::new(p.as_str())
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(str::to_owned)
                });
                layout.set_field_file_name(file_name);
                let (bytes, w, h) = layout.render_document_with_selection(
                    doc,
                    prev.width,
                    Some((sel_start, sel_end)),
                );
                prev.bytes = bytes;
                prev.width_px = w;
                prev.height_px = h;
                prev.sel_start = sel_start;
                prev.sel_end = sel_end;
                prev.valid = true;
            }
            (Arc::clone(&prev.bytes), prev.width_px, prev.height_px)
        };
        let page_setup_caret = page_setup_for_cursor(doc, sel.head);
        let page_is_a4 = is_a4_page(page_setup_caret);
        let page_landscape = is_landscape_page(page_setup_caret);
        let header_text = story_plain_text(&doc.header);
        let footer_text = story_plain_text(&doc.footer);
        let header_first_text = story_plain_text(&doc.header_first);
        let footer_first_text = story_plain_text(&doc.footer_first);
        let header_even_text = story_plain_text(&doc.header_even);
        let footer_even_text = story_plain_text(&doc.footer_even);
        let title_page = page_setup_caret.title_page;
        let even_and_odd_headers = page_setup_caret.even_and_odd_headers;
        let caret_off = plain_offset_from_cursor(doc, sel.head);
        let comment_hit = comment_id_overlapping(doc, caret_off, caret_off)
            .and_then(|id| doc.comments.iter().find(|c| c.id == id));
        let comment_edit_text = comment_hit.map(|c| c.text.clone()).unwrap_or_default();
        let comment_at_caret = comment_hit
            .map(|c| {
                let body: String = c.text.chars().take(80).collect();
                if c.author.is_empty() {
                    body
                } else {
                    format!("{}: {body}", c.author)
                }
            })
            .unwrap_or_default();
        drop(doc_guard);
        ViewerSnapshot::Document(DocumentSnapshot {
            path_display,
            dirty,
            block_count,
            word_count,
            char_count,
            comment_count,
            comment_at_caret,
            comment_edit_text,
            plain_text: Arc::from(plain_text.as_str()),
            warnings,
            info_text: String::new(),
            bold,
            italic,
            underline,
            strikethrough,
            double_strikethrough,
            highlight,
            all_caps,
            small_caps,
            vanish,
            shadow,
            emboss,
            imprint,
            shade,
            border_bottom,
            keep_next,
            keep_lines,
            widow_control,
            contextual_spacing,
            bidi,
            suppress_auto_hyphens,
            outline_level,
            character_style_id,
            section_break_continuous,
            superscript,
            subscript,
            font_size_pt,
            font_family,
            color_rgb,
            alignment,
            list_kind,
            can_undo,
            can_redo,
            preview_rgba,
            preview_width_px,
            preview_height_px,
            preview_render_scale: crate::document::layout::PREVIEW_RENDER_SCALE.round() as i32,
            source_mode,
            find_gen: *self.find_gen.lock(),
            find_anchor: *self.find_anchor.lock(),
            find_cursor: *self.find_cursor.lock(),
            find_match_index: *self.find_match_index.lock(),
            find_match_count: *self.find_match_count.lock(),
            find_scroll_y_px: *self.find_scroll_y_px.lock(),
            preview_zoom_percent: ((*self.preview_zoom.lock() * 100.0).round() as i32)
                .clamp(50, 300),
            link_hover: *self.link_hover.lock(),
            link_url,
            page_is_a4,
            page_landscape,
            header_text,
            footer_text,
            header_first_text,
            footer_first_text,
            header_even_text,
            footer_even_text,
            title_page,
            even_and_odd_headers,
            orchid_generation: *self.orchid_generation.lock(),
            orchid_linked: *self.orchid_linked.lock(),
            orchid_c2pa_ok: *self.orchid_c2pa_ok.lock(),
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
        let os = Path::new(&os_path);
        if orchid_io::is_orchid_path(os) {
            let encrypt = self.decrypt_identity();
            if let Some(store) = self.chunk_store.as_ref() {
                let parent = *self.orchid_generation.lock();
                let next = parent.saturating_add(1).max(1);
                let uuid = *self.orchid_file_uuid.lock();
                orchid_io::save_document_as_linked_orchid(
                    &doc,
                    os,
                    store.as_ref(),
                    uuid,
                    next,
                    parent,
                    encrypt.as_ref(),
                )
                .await?;
                self.remember_orchid_identity(os);
                self.set_prefer_original_docx_name(false);
            } else {
                let raw_name = self.take_orchid_raw_name();
                orchid_io::save_document_as_orchid_named(&doc, os, raw_name, encrypt.as_ref())
                    .await?;
                self.remember_orchid_identity(os);
                self.set_prefer_original_docx_name(false);
            }
        } else {
            self.clear_orchid_identity();
            ooxml::container::save_document(&doc, os).await?;
        }
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

    /// Change the save target and write (`.orchid` or `.docx` by extension).
    pub async fn save_as(&mut self, new_path: orchid_fs::FsPath) -> Result<()> {
        if !new_path.is_local() {
            return Err(ViewerError::DocumentSave(String::from(
                "saving remote documents is not supported yet",
            )));
        }
        if let Ok(os) = new_path.to_local() {
            if orchid_io::is_orchid_path(Path::new(&os)) {
                // Keep original.docx when Save As from a DOCX into `.orchid`.
                let from_docx = self
                    .path
                    .read()
                    .as_ref()
                    .and_then(|p| p.to_local().ok())
                    .is_some_and(|p| {
                        Path::new(&p)
                            .extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
                    });
                if from_docx {
                    self.set_prefer_original_docx_name(true);
                }
            }
        }
        *self.path.write() = Some(new_path);
        <Self as Viewer>::save(self).await
    }
}
