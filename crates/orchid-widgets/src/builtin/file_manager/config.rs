//! File-manager persistent configuration.

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// Sort key used by the file manager.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
#[serde(rename_all = "kebab-case")]
pub enum SortBy {
    #[default]
    Name,
    Size,
    Modified,
    Type,
}

/// Single-click vs double-click behaviour.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
#[serde(rename_all = "kebab-case")]
pub enum ClickBehavior {
    SingleToOpen,
    #[default]
    DoubleToOpen,
}

/// View mode.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
#[serde(rename_all = "kebab-case")]
pub enum ViewMode {
    Icons,
    List,
    #[default]
    Details,
    Gallery,
}

/// Thumbnail-size preset.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
#[serde(rename_all = "kebab-case")]
pub enum ThumbnailSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// Persistent file-manager config.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct FileManagerConfig {
    pub dual_pane: bool,
    pub default_view_mode: ViewMode,
    pub show_hidden: bool,
    pub show_extensions: bool,
    pub sort_by: SortBy,
    pub sort_descending: bool,
    pub thumbnail_size: ThumbnailSize,
    pub confirm_delete: bool,
    pub delete_to_recycle: bool,
    pub click_behavior: ClickBehavior,
}

impl Default for FileManagerConfig {
    fn default() -> Self {
        Self {
            dual_pane: false,
            default_view_mode: ViewMode::Details,
            show_hidden: false,
            show_extensions: true,
            sort_by: SortBy::Name,
            sort_descending: false,
            thumbnail_size: ThumbnailSize::Medium,
            confirm_delete: true,
            delete_to_recycle: true,
            click_behavior: ClickBehavior::DoubleToOpen,
        }
    }
}
