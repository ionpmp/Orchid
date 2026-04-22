//! Atomic save helper for the text viewer.
//!
//! The MVP writes a sibling `*.tmp` file via the underlying provider, then
//! renames over the target. The provider's `rename` is expected to be
//! atomic on same-volume paths.

use std::sync::Arc;

use crate::error::{Result, ViewerError};
use crate::text::buffer::TextBuffer;

/// Save `buffer` back to `path`.
///
/// # Errors
///
/// Propagates [`orchid_fs::FsError`] on any provider failure, and
/// [`ViewerError::TextDecode`] when encoding the buffer fails.
pub async fn save_text(
    path: &orchid_fs::FsPath,
    registry: Arc<orchid_fs::FsProviderRegistry>,
    buffer: &TextBuffer,
) -> Result<()> {
    let provider = registry
        .for_path(path)
        .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;
    let bytes = buffer.to_bytes()?;
    // Staged write.
    let tmp = tmp_path_for(path)?;
    provider.write(&tmp, &bytes).await?;
    provider.rename(&tmp, path).await?;
    Ok(())
}

fn tmp_path_for(path: &orchid_fs::FsPath) -> Result<orchid_fs::FsPath> {
    let raw = path.as_str();
    let with_suffix = format!("{raw}.orchid-save");
    Ok(orchid_fs::FsPath::new(with_suffix)?)
}

// Keep the unused import quiet on builds that elide the tracing calls.
#[allow(dead_code)]
fn _touch() -> Option<ViewerError> {
    None
}
