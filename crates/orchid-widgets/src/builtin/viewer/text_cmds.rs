//! Text viewer commands.

use super::*;

/// Scroll the text viewer by `delta` lines.
pub async fn text_scroll(instance_id: Uuid, delta: i32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                tv.scroll_lines(delta);
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: switch read / edit mode (`edit == true` → edit).
pub async fn text_set_mode(instance_id: Uuid, edit: bool) -> WidgetResult<()> {
    use orchid_viewers::TextViewerMode;
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                tv.set_mode(if edit {
                    TextViewerMode::Edit
                } else {
                    TextViewerMode::Read
                });
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: flip read ↔ edit. Returns `true` when the resulting mode is edit.
///
/// Leaving edit mode with unsaved changes is allowed for MVP — the dirty ●
/// indicator remains until save.
pub async fn text_toggle_edit(instance_id: Uuid) -> WidgetResult<bool> {
    use orchid_viewers::TextViewerMode;
    let inner = live_inner(instance_id)?;
    let edit = {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(false);
        };
        let Some(tv) = v.as_any().downcast_ref::<TextViewer>() else {
            return Ok(false);
        };
        let edit = tv.mode() == TextViewerMode::Read;
        tv.set_mode(if edit {
            TextViewerMode::Edit
        } else {
            TextViewerMode::Read
        });
        edit
    };
    inner.refresh_snapshot().await;
    Ok(edit)
}

/// Text: push the full document contents from the plain editor.
pub async fn text_push_edit(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                tv.replace_content(&text)
                    .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: save buffer to disk (clears dirty).
pub async fn text_save(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut guard = inner.viewer.lock().await;
        let v = guard
            .as_mut()
            .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
        v.save()
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: toolbar action (`text` / `hex` / `bin` / `undo` / `redo` / `print`).
pub async fn text_action(instance_id: Uuid, action: String) -> WidgetResult<()> {
    use orchid_viewers::TextDisplayMode;
    let inner = live_inner(instance_id)?;
    match action.as_str() {
        "text" | "hex" | "bin" => {
            let mode = match action.as_str() {
                "hex" => TextDisplayMode::Hex,
                "bin" => TextDisplayMode::Binary,
                _ => TextDisplayMode::Text,
            };
            let guard = inner.viewer.lock().await;
            if let Some(v) = guard.as_ref() {
                if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                    tv.set_display_mode(mode);
                }
            }
        }
        "undo" => {
            let guard = inner.viewer.lock().await;
            if let Some(v) = guard.as_ref() {
                if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                    tv.undo()
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                }
            }
        }
        "redo" => {
            let guard = inner.viewer.lock().await;
            if let Some(v) = guard.as_ref() {
                if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                    tv.redo()
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                }
            }
        }
        "print" => {
            text_print_locked(&inner).await?;
            return Ok(());
        }
        _ => {}
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: re-decode with an explicit encoding label.
pub async fn text_set_encoding(instance_id: Uuid, label: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                tv.set_encoding(&label)
                    .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: find next (`forward`) / previous match.
pub async fn text_find(
    instance_id: Uuid,
    query: String,
    forward: bool,
    regex: bool,
    multiline: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                tv.find(
                    &query,
                    forward,
                    orchid_viewers::FindOptions { regex, multiline },
                )
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: replace current match or all matches.
pub async fn text_replace(
    instance_id: Uuid,
    query: String,
    replacement: String,
    all: bool,
    regex: bool,
    multiline: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                let opts = orchid_viewers::FindOptions { regex, multiline };
                if all {
                    tv.replace_all(&query, &replacement, opts)
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                } else {
                    tv.replace_current(&query, &replacement, opts)
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                }
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Open the current viewer file in the system default app (player / browser).
pub fn open_current_externally(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let path = inner
        .path
        .read()
        .clone()
        .ok_or_else(|| WidgetError::InvalidStateForOperation("no file open".into()))?;
    let os = path
        .to_local()
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    opener::open(&os).map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))
}
