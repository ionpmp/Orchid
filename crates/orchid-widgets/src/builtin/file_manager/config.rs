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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode)]
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

/// One tab saved in workspace state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct PersistedTab {
    pub id: String,
    pub path: String,
    pub history_back: Vec<String>,
    pub history_forward: Vec<String>,
    pub view_mode: ViewMode,
    pub sort_by: SortBy,
    pub sort_descending: bool,
}

/// Saved pane with tabs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct PersistedPane {
    pub tabs: Vec<PersistedTab>,
    pub active_tab: usize,
}

/// Which pane had focus when state was saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum PersistedActivePane {
    Left,
    Right,
}

/// Saved navigation session (tabs, paths, history).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct FileManagerSession {
    pub left_pane: PersistedPane,
    pub right_pane: Option<PersistedPane>,
    pub active_pane: PersistedActivePane,
}

/// Config plus optional live session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct FileManagerPersisted {
    pub config: FileManagerConfig,
    pub session: Option<FileManagerSession>,
}

/// Decode widget bytes, falling back to legacy config-only blobs.
///
/// # Errors
///
/// Returns [`crate::error::WidgetError::CreationFailed`] when neither format parses.
pub fn decode_persisted(bytes: &[u8]) -> crate::error::Result<FileManagerPersisted> {
    if let Ok(persisted) = crate::widget::config::restore_state::<FileManagerPersisted>(bytes) {
        return Ok(persisted);
    }
    let config = crate::widget::config::restore_state::<FileManagerConfig>(bytes)?;
    Ok(FileManagerPersisted {
        config,
        session: None,
    })
}

#[cfg(test)]
mod persist_tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn decode_legacy_config_only() {
        let bytes = crate::widget::config::save_state(&FileManagerConfig::default()).unwrap();
        let persisted = decode_persisted(&bytes).unwrap();
        assert!(persisted.session.is_none());
        assert_eq!(persisted.config, FileManagerConfig::default());
    }

    #[test]
    fn session_roundtrip() {
        let session = FileManagerSession {
            left_pane: PersistedPane {
                tabs: vec![PersistedTab {
                    id: Uuid::new_v4().to_string(),
                    path: "local:/tmp".into(),
                    history_back: vec!["local:/".into()],
                    history_forward: vec![],
                    view_mode: ViewMode::List,
                    sort_by: SortBy::Size,
                    sort_descending: true,
                }],
                active_tab: 0,
            },
            right_pane: None,
            active_pane: PersistedActivePane::Left,
        };
        let persisted = FileManagerPersisted {
            config: FileManagerConfig::default(),
            session: Some(session.clone()),
        };
        let bytes = crate::widget::config::save_state(&persisted).unwrap();
        let decoded = decode_persisted(&bytes).unwrap();
        assert_eq!(decoded.session, Some(session));
    }
}
