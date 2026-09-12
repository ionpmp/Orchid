//! PDF viewer commands.

use super::*;

/// Go to the previous PDF page.
pub async fn pdf_prev_page(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.prev_page().await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: next page.
pub async fn pdf_next_page(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.next_page().await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: fit width.
pub async fn pdf_fit_width(instance_id: Uuid, viewport_w: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.fit_width(viewport_w).await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: fit page.
pub async fn pdf_fit_page(instance_id: Uuid, viewport_w: f32, viewport_h: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.fit_page(viewport_w, viewport_h)
                    .await
                    .map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: zoom in.
pub async fn pdf_zoom_in(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.zoom_in().await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: zoom out.
pub async fn pdf_zoom_out(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.zoom_out().await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: go to 1-based page index.
pub async fn pdf_go_to_page(instance_id: Uuid, page: i32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.go_to_page(page.max(1) as u32)
                    .await
                    .map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: extract Unicode text for the current page (caller copies to clipboard).
pub async fn pdf_current_page_text(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let guard = inner.viewer.lock().await;
    let Some(v) = guard.as_ref() else {
        return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
    };
    let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() else {
        return Err(WidgetError::InvalidStateForOperation(
            "not a pdf viewer".into(),
        ));
    };
    pdf.copy_text().await.map_err(map_viewer_err)
}

/// PDF: write the current page as a sibling PNG.
pub async fn pdf_extract_page(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let dest = {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() else {
            return Err(WidgetError::InvalidStateForOperation(
                "not a pdf viewer".into(),
            ));
        };
        pdf.extract_current_page().map_err(map_viewer_err)?
    };
    Ok(dest.to_string_lossy().into_owned())
}

/// PDF: find (`dir` 0 = new, 1 = next, -1 = previous).
pub async fn pdf_find(
    instance_id: Uuid,
    query: String,
    match_case: bool,
    dir: i32,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.find(query, match_case, dir)
                    .await
                    .map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: pointer on the page image (`phase` 0 press, 1 drag, 2 release, 3 word, 4 page).
pub async fn pdf_pointer(instance_id: Uuid, phase: i32, x: f32, y: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.pointer(phase, x, y);
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: jump to an outline destination page.
pub async fn pdf_outline_goto(instance_id: Uuid, page: i32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.outline_goto(page.max(0) as u32)
                    .await
                    .map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: print the open file (or a temp copy of the payload).
pub async fn pdf_print(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let guard = inner.viewer.lock().await;
    let Some(v) = guard.as_ref() else {
        return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
    };
    let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() else {
        return Err(WidgetError::InvalidStateForOperation(
            "not a pdf viewer".into(),
        ));
    };
    match pdf.local_path() {
        Ok(path) => print_path(&path),
        Err(_) => {
            let bytes = pdf.payload_bytes().map_err(map_viewer_err)?;
            let tmp = std::env::temp_dir().join(format!("orchid-pdf-print-{instance_id}.pdf"));
            std::fs::write(&tmp, bytes.as_slice())
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            print_path(&tmp)
        }
    }
}

/// PDF: export the current selection as a sibling highlight PDF.
pub async fn pdf_highlight(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let dest = {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() else {
            return Err(WidgetError::InvalidStateForOperation(
                "not a pdf viewer".into(),
            ));
        };
        pdf.highlight_selection().await.map_err(map_viewer_err)?
    };
    inner.refresh_snapshot().await;
    Ok(dest.to_string_lossy().into_owned())
}

/// PDF: pin the current selection as a sticky text annotation.
pub async fn pdf_comment(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let dest = {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() else {
            return Err(WidgetError::InvalidStateForOperation(
                "not a pdf viewer".into(),
            ));
        };
        pdf.comment_selection().await.map_err(map_viewer_err)?
    };
    inner.refresh_snapshot().await;
    Ok(dest.to_string_lossy().into_owned())
}
