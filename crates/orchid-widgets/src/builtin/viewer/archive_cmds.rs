//! Archive viewer commands.

use super::*;

/// Open an archive entry (folder or nested archive).
pub async fn archive_navigate_into(instance_id: Uuid, path: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_mut() {
            if let Some(ar) = v.as_any_mut().downcast_mut::<ArchiveViewer>() {
                ar.navigate_into(&path).await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Archive: go up.
pub async fn archive_navigate_up(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_mut() {
            if let Some(ar) = v.as_any_mut().downcast_mut::<ArchiveViewer>() {
                ar.navigate_up().await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Archive: select file for preview.
pub async fn archive_select(instance_id: Uuid, path: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_mut() {
            if let Some(ar) = v.as_any_mut().downcast_mut::<ArchiveViewer>() {
                ar.select(&path).await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Archive: extract the selected file beside the archive.
pub async fn archive_extract_selected(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let dest = {
        let mut guard = inner.viewer.lock().await;
        let v = guard
            .as_mut()
            .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
        let ar = v
            .as_any_mut()
            .downcast_mut::<ArchiveViewer>()
            .ok_or_else(|| WidgetError::InvalidStateForOperation("not an archive".into()))?;
        ar.extract_selected_to_sibling()
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
    };
    inner.refresh_snapshot().await;
    Ok(dest.to_string_lossy().into_owned())
}

/// Archive: extract all entries into a sibling folder.
pub async fn archive_extract_all(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let dest = {
        let mut guard = inner.viewer.lock().await;
        let v = guard
            .as_mut()
            .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
        let ar = v
            .as_any_mut()
            .downcast_mut::<ArchiveViewer>()
            .ok_or_else(|| WidgetError::InvalidStateForOperation("not an archive".into()))?;
        ar.extract_all_to_sibling()
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
            .0
    };
    inner.refresh_snapshot().await;
    Ok(dest.to_string_lossy().into_owned())
}
