//! Document viewer commands.

use super::*;

/// Dispatch a document-editor toolbar / shortcut action.
pub async fn document_action(instance_id: Uuid, action: String) -> WidgetResult<()> {
    use orchid_viewers::{Alignment, ListKind};

    let inner = live_inner(instance_id)?;
    match action.as_str() {
        "print" => {
            document_print_locked(&inner).await?;
            return Ok(());
        }
        "save" => {
            let mut guard = inner.viewer.lock().await;
            let v = guard
                .as_mut()
                .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
            v.save()
                .await
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        }
        "save-as" => {
            let default_name = {
                let guard = inner.viewer.lock().await;
                let v = guard
                    .as_ref()
                    .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
                let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
                    return Ok(());
                };
                doc.path_clone()
                    .and_then(|p| {
                        std::path::Path::new(p.as_str())
                            .file_name()
                            .and_then(|s| s.to_str())
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| "Untitled.orchid".into())
            };
            let Some(os_path) = orchid_viewers::pick_document_save_path(&default_name) else {
                return Ok(());
            };
            let fs_path = orchid_fs::FsPath::from_local(&os_path)
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            let mut guard = inner.viewer.lock().await;
            let v = guard
                .as_mut()
                .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
            let Some(doc) = v.as_any_mut().downcast_mut::<DocumentViewer>() else {
                return Ok(());
            };
            doc.save_as(fs_path)
                .await
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        }
        other => {
            let guard = inner.viewer.lock().await;
            let v = guard
                .as_ref()
                .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
            let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
                return Ok(());
            };
            let result = match other {
                "undo" => doc.undo(),
                "redo" => doc.redo(),
                "bold" => doc.toggle_style_all('b'),
                "italic" => doc.toggle_style_all('i'),
                "underline" => doc.toggle_style_all('u'),
                "strikethrough" | "strike" => doc.toggle_style_all('s'),
                "double-strikethrough" | "dstrike" => doc.toggle_style_all('d'),
                "all-caps" | "caps" => doc.toggle_style_all('a'),
                "small-caps" => doc.toggle_style_all('m'),
                "vanish" | "hidden" => doc.toggle_style_all('v'),
                "shadow" => doc.toggle_style_all('w'),
                "emboss" => doc.toggle_style_all('e'),
                "imprint" => doc.toggle_style_all('n'),
                "highlight" => doc.toggle_style_all('h'),
                "shade" => doc.toggle_paragraph_shade_selection(),
                "border-bottom" => doc.toggle_paragraph_border_bottom_selection(),
                "keep-next" => doc.toggle_keep_next_selection(),
                "keep-lines" => doc.toggle_keep_lines_selection(),
                "widow-control" => doc.toggle_widow_control_selection(),
                "contextual-spacing" => doc.toggle_contextual_spacing_selection(),
                "bidi" => doc.toggle_bidi_selection(),
                "suppress-auto-hyphens" => doc.toggle_suppress_auto_hyphens_selection(),
                "outline-level-cycle" => doc.cycle_outline_level_selection(),
                "character-style-cycle" => doc.cycle_character_style_selection(),
                "insert-bookmark" => doc.insert_bookmark_at_selection().map(|_| ()),
                "insert-comment" => doc.insert_comment_at_selection().map(|_| ()),
                "delete-comment" => doc.delete_comment_at_selection().map(|_| ()),
                "comment-next" => doc.goto_comment(true).map(|_| ()),
                "comment-prev" => doc.goto_comment(false).map(|_| ()),
                "insert-page-field" => doc.insert_page_number_fields_in_footer(),
                "insert-date-field" => {
                    doc.insert_field_at_selection(orchid_viewers::document::model::DocField::Date)
                }
                "insert-filename-field" => doc
                    .insert_field_at_selection(orchid_viewers::document::model::DocField::FileName),
                "insert-section-break" => doc.preview_insert_section_break(),
                "superscript" => doc.toggle_style_all('^'),
                "subscript" => doc.toggle_style_all('_'),
                "clear-formatting" => doc.clear_formatting_selection(),
                "font-smaller" => doc.bump_font_size_selection(-1),
                "font-larger" => doc.bump_font_size_selection(1),
                "font-family-prev" => doc.bump_font_family_selection(-1),
                "font-family-next" => doc.bump_font_family_selection(1),
                "align-left" => doc.set_alignment_all(Alignment::Left),
                "align-center" => doc.set_alignment_all(Alignment::Center),
                "align-right" => doc.set_alignment_all(Alignment::Right),
                "align-justify" => doc.set_alignment_all(Alignment::Justify),
                "list-bullet" => doc.toggle_list_all(ListKind::Bullet),
                "list-numbered" => doc.toggle_list_all(ListKind::Numbered),
                "list-indent" => doc.bump_list_level_selection(1),
                "list-outdent" => doc.bump_list_level_selection(-1),
                "space-after-more" => doc.bump_paragraph_spacing_selection(0, 120),
                "space-after-less" => doc.bump_paragraph_spacing_selection(0, -120),
                "space-before-more" => doc.bump_paragraph_spacing_selection(120, 0),
                "space-before-less" => doc.bump_paragraph_spacing_selection(-120, 0),
                "line-spacing-more" => doc.bump_line_spacing_selection(1),
                "line-spacing-less" => doc.bump_line_spacing_selection(-1),
                "margin-more" => doc.bump_page_margins(180),
                "margin-less" => doc.bump_page_margins(-180),
                "header-distance-more" => doc.bump_header_footer_distances(180, 0),
                "header-distance-less" => doc.bump_header_footer_distances(-180, 0),
                "footer-distance-more" => doc.bump_header_footer_distances(0, 180),
                "footer-distance-less" => doc.bump_header_footer_distances(0, -180),
                "indent-more" => doc.bump_indent_left_selection(720),
                "indent-less" => doc.bump_indent_left_selection(-720),
                "indent-right-more" => doc.bump_indent_right_selection(720),
                "indent-right-less" => doc.bump_indent_right_selection(-720),
                "first-line-more" => doc.bump_indent_first_line_selection(360),
                "first-line-less" => doc.bump_indent_first_line_selection(-360),
                "page-size-cycle" => doc.cycle_page_size(),
                "page-orientation-toggle" => doc.toggle_page_orientation(),
                "title-page" => doc.toggle_title_page(),
                "even-and-odd-headers" => doc.toggle_even_and_odd_headers(),
                "zoom-in" => doc.bump_preview_zoom(1),
                "zoom-out" => doc.bump_preview_zoom(-1),
                "zoom-reset" => doc.reset_preview_zoom(),
                "table-insert" => doc.preview_insert_table(2, 2),
                // "image-insert" is handled in the UI layer (clipboard bytes).
                "table-row-insert" => doc.preview_insert_table_row(),
                "table-row-delete" => doc.preview_delete_table_row(),
                "table-col-insert" => doc.preview_insert_table_column(),
                "table-col-delete" => doc.preview_delete_table_column(),
                "table-merge" => doc.preview_merge_table_cells(),
                "table-unmerge" => doc.preview_unmerge_table_cells(),
                "toggle-source" => {
                    doc.set_source_mode(!doc.source_mode());
                    Ok(())
                }
                color if color.starts_with("color-") => {
                    if let Some(rgb) = parse_toolbar_color(&color["color-".len()..]) {
                        doc.set_color_selection(rgb)
                    } else {
                        Ok(())
                    }
                }
                family if family.starts_with("font-family-") => {
                    let slug = &family["font-family-".len()..];
                    if let Some(name) = orchid_viewers::document::resolve_font_family_slug(slug) {
                        doc.set_font_family_selection(name)
                    } else {
                        Ok(())
                    }
                }
                _ => Ok(()),
            };
            result.map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            inner.schedule_doc_autosave();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: replace body from the Slint `TextInput` draft (no caret-breaking rebuild).
pub async fn document_push_edit(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                doc.replace_plain_text(&text)
                    .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            }
        }
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: find next/previous match in plain text.
pub async fn document_find(
    instance_id: Uuid,
    query: String,
    forward: bool,
    match_case: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                let _ = doc.preview_find(&query, forward, match_case);
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: replace current find match, or all matches when `all` is true.
pub async fn document_replace(
    instance_id: Uuid,
    query: String,
    replacement: String,
    all: bool,
    match_case: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                if all {
                    doc.preview_replace_all(&query, &replacement, match_case)
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                } else {
                    let _ = doc
                        .preview_replace_current(&query, &replacement, match_case)
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                }
            }
        }
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: update selection from Source `TextInput` UTF-8 byte offsets.
pub async fn document_set_selection(instance_id: Uuid, anchor: i32, head: i32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                doc.set_selection_plain_offsets(anchor.max(0) as usize, head.max(0) as usize);
            }
        }
    }
    // Selection changes do not need a full snapshot rebuild for caret-only moves;
    // toolbar accents update on the next format/edit refresh.
    Ok(())
}

/// Document: set preview layout width from the Slint viewport (CSS pixels).
pub async fn document_set_viewport_width(instance_id: Uuid, width_px: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                doc.set_preview_viewport_width(width_px.max(200.0));
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: update the body text of the comment at the caret / selection.
///
/// Empty / whitespace-only `text` is a no-op.
pub async fn document_comment(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        doc.set_comment_text_at_selection(&text)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: apply or remove an external hyperlink on the selection.
///
/// Empty `url` removes the link under the caret / selection.
pub async fn document_link(instance_id: Uuid, url: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        let result = if url.trim().is_empty() {
            doc.remove_hyperlink_selection()
        } else {
            doc.set_hyperlink_selection(&url)
        };
        result.map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: set or clear the default header story plain text.
pub async fn document_header(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        doc.set_header_plain_text(&text)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: set or clear the default footer story plain text.
pub async fn document_footer(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        doc.set_footer_plain_text(&text)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: pointer on the preview canvas
/// (`phase`: 0=down, 1=move, 2=up, 3=double-click word select,
/// 4=triple-click paragraph, 5=hover; `ctrl` opens hyperlinks on down).

/// Document: set or clear the first-page header story plain text.
pub async fn document_header_first(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        doc.set_header_first_plain_text(&text)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: set or clear the first-page footer story plain text.
pub async fn document_footer_first(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        doc.set_footer_first_plain_text(&text)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: set or clear the even-page header story plain text.
pub async fn document_header_even(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        doc.set_header_even_plain_text(&text)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: set or clear the even-page footer story plain text.
pub async fn document_footer_even(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        doc.set_footer_even_plain_text(&text)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Handle a pointer event on the document preview canvas.
pub async fn document_preview_pointer(
    instance_id: Uuid,
    phase: i32,
    x: f32,
    y: f32,
    ctrl: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let mut outcome = orchid_viewers::document::PreviewPointerOutcome::default();
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                outcome = doc.preview_pointer(phase.clamp(0, 5) as u8, x, y, ctrl);
            }
        }
    }
    if let Some(url) = outcome.open_url.as_deref() {
        if let Err(e) = opener::open(url) {
            warn!(error = %e, %url, "failed to open document hyperlink");
        }
    }
    // Refresh so caret / selection / link cursor tracks the pointer.
    if outcome.refresh {
        inner.refresh_snapshot().await;
    }
    Ok(())
}

/// Document: keyboard input while the preview canvas has focus.
///
/// Special keys are sent as tokens (`Backspace`, `Delete`, `Return`, `Left`,
/// `Right`, `Up`, `Down`); otherwise `key` is inserted as literal text.
///
/// Clipboard shortcuts (`c`/`x`/`v`) are handled in the UI layer (arboard).
pub async fn document_preview_key(
    instance_id: Uuid,
    key: String,
    ctrl: bool,
    shift: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    if ctrl && matches!(key.as_str(), "s" | "S") {
        if shift {
            return document_action(instance_id, "save-as".into()).await;
        }
        return document_action(instance_id, "save".into()).await;
    }
    if ctrl && matches!(key.as_str(), "p" | "P") {
        return document_action(instance_id, "print".into()).await;
    }
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        if doc.source_mode() {
            return Ok(());
        }
        let result = if ctrl {
            match key.as_str() {
                "a" | "A" => {
                    doc.preview_select_all();
                    Ok(())
                }
                "Home" => {
                    doc.preview_move_document_boundary(false, shift);
                    Ok(())
                }
                "End" => {
                    doc.preview_move_document_boundary(true, shift);
                    Ok(())
                }
                "Left" => {
                    doc.preview_move_by_words(-1, shift);
                    Ok(())
                }
                "Right" => {
                    doc.preview_move_by_words(1, shift);
                    Ok(())
                }
                "Backspace" => doc.preview_delete_word_backward(),
                "Delete" => doc.preview_delete_word_forward(),
                "b" | "B" => doc.toggle_style_all('b'),
                "i" | "I" => doc.toggle_style_all('i'),
                "u" | "U" => doc.toggle_style_all('u'),
                "x" | "X" if shift => doc.toggle_style_all('s'),
                "h" | "H" if shift => doc.toggle_style_all('h'),
                "=" | "+" if shift => doc.toggle_style_all('^'),
                "=" => doc.toggle_style_all('_'),
                " " => doc.clear_formatting_selection(),
                "z" | "Z" if shift => doc.redo(),
                "z" | "Z" => doc.undo(),
                "y" | "Y" => doc.redo(),
                "0" => doc.reset_preview_zoom(),
                "Return" => doc.preview_insert_page_break(),
                _ => Ok(()),
            }
        } else {
            match key.as_str() {
                "Backspace" => doc.preview_delete_backward(),
                "Delete" => doc.preview_delete_forward(),
                "Return" if shift => doc.preview_insert_soft_break(),
                "Return" => doc.preview_insert_paragraph_break(),
                "Tab" if shift => {
                    if doc.selection().head.cell.is_some() {
                        let _ = doc.preview_move_table_cell(false);
                        Ok(())
                    } else {
                        doc.bump_list_level_selection(-1)
                    }
                }
                "Tab" => {
                    if doc.selection().head.cell.is_some() {
                        let _ = doc.preview_move_table_cell(true);
                        Ok(())
                    } else {
                        doc.bump_list_level_selection(1)
                    }
                }
                "Home" => {
                    doc.preview_move_line_boundary(false, shift);
                    Ok(())
                }
                "End" => {
                    doc.preview_move_line_boundary(true, shift);
                    Ok(())
                }
                "Left" => {
                    doc.preview_move_by_chars(-1, shift);
                    Ok(())
                }
                "Right" => {
                    doc.preview_move_by_chars(1, shift);
                    Ok(())
                }
                "Up" => {
                    doc.preview_move_vertical(-1, shift);
                    Ok(())
                }
                "Down" => {
                    doc.preview_move_vertical(1, shift);
                    Ok(())
                }
                other if is_printable_preview_text(other) => doc.preview_insert_text(other),
                _ => Ok(()),
            }
        };
        result.map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: plain text of the current preview selection (for clipboard copy).
pub async fn document_preview_selection_text(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let guard = inner.viewer.lock().await;
    let Some(v) = guard.as_ref() else {
        return Ok(String::new());
    };
    let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
        return Ok(String::new());
    };
    Ok(doc.selected_plain_text())
}

/// Document: cut selection; returns the removed text for the clipboard.
pub async fn document_preview_cut(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let text = {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(String::new());
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(String::new());
        };
        doc.preview_cut_selection()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
    };
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(text)
}

/// Document: paste plain text at the preview caret.
pub async fn document_preview_paste(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        doc.preview_paste_plain(&text)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: insert a PNG/JPEG/… image block after the caret (from clipboard or toolbar).
pub async fn document_preview_insert_image(
    instance_id: Uuid,
    bytes: Vec<u8>,
    width_px: u32,
    height_px: u32,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        doc.preview_insert_image(bytes, width_px, height_px)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.schedule_doc_autosave();
    inner.refresh_snapshot().await;
    Ok(())
}

fn is_printable_preview_text(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    !key.chars()
        .any(|c| c.is_control() || ('\u{f700}'..='\u{f7ff}').contains(&c) || c == '\u{7f}')
}

/// Parse a toolbar colour token (`RRGGBB` hex) into RGB bytes.
fn parse_toolbar_color(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim();
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r, g, b])
}
