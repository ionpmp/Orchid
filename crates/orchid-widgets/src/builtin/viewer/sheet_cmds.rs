//! Spreadsheet cell edit.

use super::*;

/// Write one cell of the open workbook and refresh the sheet table.
///
/// A formula cell is left unchanged. The returned string is the path that
/// was written.
pub async fn sheet_edit(
    instance_id: Uuid,
    sheet: String,
    address: String,
    text: String,
) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let registry = Arc::clone(&inner.deps.registry);
    let saved = {
        let guard = inner.viewer.lock().await;
        let v = guard
            .as_ref()
            .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
        let Some(office) = v.as_any().downcast_ref::<orchid_viewers::OfficeViewer>() else {
            return Err(WidgetError::InvalidStateForOperation(
                "viewer-sheet-not-workbook".into(),
            ));
        };
        office
            .edit_cell(registry, &sheet, &address, &text)
            .await
            .map_err(map_viewer_err)?;
        match office.snapshot() {
            ViewerSnapshot::Sheet(book) => book.path_display,
            _ => String::new(),
        }
    };
    inner.refresh_snapshot().await;
    Ok(saved)
}
