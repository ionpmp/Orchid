//! Open / viewport / passphrase / path helpers.

use super::*;

/// Update image/PDF/text viewport size for fit/zoom/window math.
pub async fn set_viewport(instance_id: Uuid, width: f32, height: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let mut should_refresh = false;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
                img.set_viewport(width, height);
                should_refresh = true;
            } else if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.apply_viewport(width, height)
                    .await
                    .map_err(map_viewer_err)?;
                should_refresh = true;
            } else if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                let count = (height / TEXT_LINE_HEIGHT_PX).floor().max(1.0) as u32;
                // Keep the current first line; only resize the window.
                tv.set_visible_range(tv.first_visible_line(), count);
                should_refresh = true;
            } else if let Some(media) = v.as_any().downcast_ref::<MediaViewer>() {
                *inner.media_viewport.write() = (width, height);
                let panel = inner.playlist_panel_open.load(Ordering::Relaxed);
                media.set_viewport(width, height, panel);
                // No immediate refresh — next frame blit uses the new target size.
            }
        }
    }
    if should_refresh {
        inner.refresh_snapshot().await;
    }
    Ok(())
}

/// Open `path` on the viewer instance `instance_id`.
pub async fn open_path(instance_id: Uuid, path: orchid_fs::FsPath) -> WidgetResult<()> {
    let inner = VIEWER_LIVE
        .get(&instance_id)
        .map(|e| Arc::clone(e.value()))
        .ok_or_else(|| WidgetError::InvalidStateForOperation("viewer widget not live".into()))?;
    inner.open_path(path).await
}

/// Unlock a pending encrypted `.orchid` on `instance_id`.
pub async fn commit_orchid_passphrase(
    instance_id: Uuid,
    passphrase: impl AsRef<str>,
) -> WidgetResult<()> {
    let inner = VIEWER_LIVE
        .get(&instance_id)
        .map(|e| Arc::clone(e.value()))
        .ok_or_else(|| WidgetError::InvalidStateForOperation("viewer widget not live".into()))?;
    inner.commit_orchid_passphrase(passphrase.as_ref()).await
}

/// Cancel the encrypted `.orchid` unlock dialog on `instance_id`.
pub fn cancel_orchid_passphrase(instance_id: Uuid) -> WidgetResult<()> {
    let inner = VIEWER_LIVE
        .get(&instance_id)
        .map(|e| Arc::clone(e.value()))
        .ok_or_else(|| WidgetError::InvalidStateForOperation("viewer widget not live".into()))?;
    inner.cancel_orchid_passphrase();
    Ok(())
}

/// Open `path` and enter text-edit mode when the file is a text document.
pub async fn open_path_for_edit(instance_id: Uuid, path: orchid_fs::FsPath) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.pending_edit.store(true, Ordering::Relaxed);
    inner.open_path(path).await
}
/// Pause every live media viewer that is currently playing.
pub async fn pause_all_media() {
    let ids: Vec<Uuid> = VIEWER_LIVE.iter().map(|e| *e.key()).collect();
    for id in ids {
        let Some(inner) = VIEWER_LIVE.get(&id).map(|e| Arc::clone(e.value())) else {
            continue;
        };
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            continue;
        };
        let Some(media) = v.as_any().downcast_ref::<MediaViewer>() else {
            continue;
        };
        if media.is_playing() {
            media.pause();
        }
    }
}

/// Current open path for a live viewer instance, if any.
#[must_use]
pub fn current_path(instance_id: Uuid) -> Option<orchid_fs::FsPath> {
    VIEWER_LIVE
        .get(&instance_id)
        .and_then(|e| e.value().path.read().clone())
}

/// Floating overlay bounds for a live viewer, if undocked.
#[must_use]
pub fn floating_bounds(instance_id: Uuid) -> Option<crate::layout::PixelBounds> {
    VIEWER_LIVE
        .get(&instance_id)
        .and_then(|e| *e.value().floating.read())
}

/// Set or clear floating overlay bounds on a live viewer.
pub fn set_floating_bounds(
    instance_id: Uuid,
    bounds: Option<crate::layout::PixelBounds>,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    *inner.floating.write() = bounds;
    Ok(())
}

/// Find a live viewer in `instance_ids` that already has `path` open.
#[must_use]
pub fn find_instance_for_path(instance_ids: &[Uuid], path: &orchid_fs::FsPath) -> Option<Uuid> {
    for id in instance_ids {
        if current_path(*id).as_ref() == Some(path) {
            return Some(*id);
        }
    }
    None
}
