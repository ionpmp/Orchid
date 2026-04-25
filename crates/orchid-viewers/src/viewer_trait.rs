//! The [`Viewer`] trait — every viewer implementation conforms to this.

use std::sync::Arc;

use async_trait::async_trait;
use std::any::Any;

use crate::error::Result;
use crate::snapshot::ViewerSnapshot;

/// Viewer trait. Implementations own the concrete file state and produce
/// [`ViewerSnapshot`]s for the UI.
#[async_trait]
pub trait Viewer: Send + Sync {
    /// Stable type id (`"image"`, `"pdf"`, `"text"`, `"archive"`).
    fn type_id(&self) -> &'static str;

    /// Open a file and initialise internal state.
    async fn open(
        &mut self,
        path: orchid_fs::FsPath,
        registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()>;

    /// Close and release any held resources.
    async fn close(&mut self) -> Result<()>;

    /// Produce a snapshot for the UI layer.
    fn snapshot(&self) -> ViewerSnapshot;

    /// Whether the viewer has unsaved edits (text editor only).
    fn is_dirty(&self) -> bool {
        false
    }

    /// Save pending edits. Default: no-op.
    async fn save(&mut self) -> Result<()> {
        Ok(())
    }

    /// Current path, if a file is open.
    fn current_path(&self) -> Option<&orchid_fs::FsPath>;

    /// Downcast for viewer-specific UI commands (zoom, page nav, etc.).
    fn as_any(&self) -> &dyn Any;

    /// Mutable downcast for async operations that need `&mut` concrete viewers.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
