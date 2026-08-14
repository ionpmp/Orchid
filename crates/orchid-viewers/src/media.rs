//! Audio / video viewer — metadata plus system-player handoff.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::error::Result;
use crate::snapshot::{MediaSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

/// Extensions treated as audio or video.
pub const MEDIA_FILE_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "webm", "avi", "mov", "wmv", "m4v", "mpeg", "mpg", "mp3", "wav", "flac", "ogg",
    "aac", "m4a", "wma", "opus", "aiff",
];

/// Whether `ext` (lowercase, no dot) is a media file.
#[must_use]
pub fn is_media_file_extension(ext: &str) -> bool {
    MEDIA_FILE_EXTENSIONS.contains(&ext)
}

fn kind_label_for(ext: &str) -> &'static str {
    match ext {
        "mp3" | "wav" | "flac" | "ogg" | "aac" | "m4a" | "wma" | "opus" | "aiff" => "audio",
        _ => "video",
    }
}

/// Lightweight media viewer (Play opens the system player).
#[derive(Debug, Default)]
pub struct MediaViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    kind_label: RwLock<String>,
}

impl MediaViewer {
    /// Empty media viewer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Viewer for MediaViewer {
    fn type_id(&self) -> &'static str {
        "media"
    }

    async fn open(
        &mut self,
        path: orchid_fs::FsPath,
        _registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()> {
        let ext = path
            .file_name()
            .and_then(|n| n.rsplit_once('.'))
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        *self.kind_label.write() = kind_label_for(&ext).into();
        *self.path.write() = Some(path);
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        *self.path.write() = None;
        Ok(())
    }

    fn snapshot(&self) -> ViewerSnapshot {
        let path_display = self
            .path
            .read()
            .as_ref()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();
        let kind_label = self.kind_label.read().clone();
        ViewerSnapshot::Media(MediaSnapshot {
            path_display,
            kind_label,
            info_text: String::new(),
        })
    }

    fn current_path(&self) -> Option<&orchid_fs::FsPath> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
