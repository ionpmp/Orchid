//! All value types persisted in the state database.
//!
//! Every type derives both `serde` traits (for interop, tests, and diagnostic
//! dumps) and `bincode` traits (for on-disk storage via [`super::codec`]).
//! Fields whose types do not natively implement the bincode traits — `Uuid`,
//! `DateTime<Utc>` — are routed through serde using
//! `#[bincode(with_serde)]`.

use bincode::{Decode, Encode};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Metadata stored once in the `meta` table under the key `"current"`.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SchemaMeta {
    /// On-disk schema version. Incremented whenever a migration is added.
    pub version: u32,
    /// When this database was first created.
    #[bincode(with_serde)]
    pub created_at: DateTime<Utc>,
    /// When the database was most recently opened by Orchid.
    #[bincode(with_serde)]
    pub last_opened_at: DateTime<Utc>,
    /// Version string of `orchid-app` that most recently opened this DB.
    pub orchid_version: String,
}

/// A single action recorded to the action history.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct HistoryEntry {
    /// Unique identifier for the entry.
    #[bincode(with_serde)]
    pub id: Uuid,
    /// When the action was recorded.
    #[bincode(with_serde)]
    pub timestamp: DateTime<Utc>,
    /// Logical action identifier, e.g. `"fs.move"` or `"widget.create"`.
    pub action_id: String,
    /// Textual representation of the action, e.g. `"orc fs move <src> <dst>"`.
    pub command_text: String,
    /// Optional target (file path, widget id, etc.).
    pub target: Option<String>,
    /// If set, the action can be reversed up to this deadline.
    #[bincode(with_serde)]
    pub reversible_until: Option<DateTime<Utc>>,
    /// If set, the textual command that would reverse this action.
    pub reverse_command: Option<String>,
    /// Bincode-encoded action-specific payload.
    pub metadata: Vec<u8>,
}

/// A saved widget instance on a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct WidgetInstance {
    /// Unique identifier for this widget instance.
    #[bincode(with_serde)]
    pub id: Uuid,
    /// Identifier of the widget type, e.g. `"weather"`, `"moon"`, `"terminal"`.
    pub widget_type: String,
    /// Which workspace this widget belongs to.
    #[bincode(with_serde)]
    pub workspace_id: Uuid,
    /// Position on the widget grid.
    pub position: GridPosition,
    /// Size classification.
    pub size: WidgetSize,
    /// Runtime lifecycle state.
    pub lifecycle: LifecycleState,
    /// Bincode-encoded per-widget configuration payload.
    pub config: Vec<u8>,
    /// When this instance was first created.
    #[bincode(with_serde)]
    pub created_at: DateTime<Utc>,
    /// When this instance was last updated.
    #[bincode(with_serde)]
    pub updated_at: DateTime<Utc>,
}

/// Widget grid coordinates (top-left corner).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub struct GridPosition {
    /// Zero-based column.
    pub col: u16,
    /// Zero-based row.
    pub row: u16,
}

/// Size classification of a widget on the grid.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum WidgetSize {
    Small,
    Medium,
    Large,
    ExtraLarge,
    /// Free-form size in grid cells.
    Free {
        /// Width in grid cells.
        w: u16,
        /// Height in grid cells.
        h: u16,
    },
}

/// Runtime lifecycle of a widget instance.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum LifecycleState {
    Active,
    Sleeping,
    Unloaded,
}

/// A virtual desktop.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct Workspace {
    /// Unique identifier.
    #[bincode(with_serde)]
    pub id: Uuid,
    /// User-facing name.
    pub name: String,
    /// Position among the user's workspaces. Expected range `1..=9`.
    pub ordinal: u8,
    /// Optional wallpaper path.
    pub wallpaper: Option<String>,
    /// When this workspace was created.
    #[bincode(with_serde)]
    pub created_at: DateTime<Utc>,
    /// When this workspace was last updated.
    #[bincode(with_serde)]
    pub updated_at: DateTime<Utc>,
}

/// Tags and color label attached to a filesystem path.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct FileTag {
    /// Canonical path of the tagged file or directory.
    pub path: String,
    /// Free-form user tags.
    pub tags: Vec<String>,
    /// Color label, if any.
    pub color_label: Option<ColorLabel>,
    /// Whether the user has starred this file.
    pub starred: bool,
    /// When the tag record was last updated.
    #[bincode(with_serde)]
    pub updated_at: DateTime<Utc>,
}

/// Color label attached to a file tag.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum ColorLabel {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Gray,
}

/// Persistent session state, written when Orchid exits cleanly.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SessionState {
    /// Currently focused workspace.
    #[bincode(with_serde)]
    pub active_workspace_id: Option<Uuid>,
    /// Open file manager tabs.
    pub open_file_manager_tabs: Vec<FileManagerTab>,
    /// Open terminal sessions.
    pub open_terminal_sessions: Vec<TerminalSession>,
    /// When this snapshot was taken.
    #[bincode(with_serde)]
    pub last_saved_at: DateTime<Utc>,
}

/// Snapshot of a single file-manager tab.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct FileManagerTab {
    /// Unique identifier for the tab.
    #[bincode(with_serde)]
    pub id: Uuid,
    /// Path currently displayed in this tab.
    pub path: String,
    /// Active view mode.
    pub view_mode: ViewMode,
    /// Current scroll position, normalized `0.0..=1.0` or pixel offset.
    pub scroll_position: f32,
}

/// View mode of a file-manager tab.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum ViewMode {
    Icons,
    List,
    Details,
    Gallery,
}

/// Snapshot of a single terminal session.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct TerminalSession {
    /// Unique identifier for the session.
    #[bincode(with_serde)]
    pub id: Uuid,
    /// Which backend this session uses.
    pub backend: TerminalBackend,
    /// Initial working directory (may have drifted since).
    pub working_directory: Option<String>,
    /// User-facing title.
    pub title: String,
}

/// Terminal backend used by a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum TerminalBackend {
    /// Windows PowerShell or PowerShell 7.
    PowerShell,
    /// Legacy `cmd.exe`.
    Cmd,
    /// Windows Subsystem for Linux with the given distro name.
    Wsl(String),
    /// SSH session with the given connection string.
    Ssh(String),
}

/// A cached blob keyed by a 32-byte hash.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct CacheEntry {
    /// Cache key, typically a BLAKE3 hash of the source content or path.
    pub key: [u8; 32],
    /// Kind of cached artefact.
    pub kind: CacheKind,
    /// When this entry was created.
    #[bincode(with_serde)]
    pub created_at: DateTime<Utc>,
    /// When this entry was most recently accessed.
    #[bincode(with_serde)]
    pub last_access_at: DateTime<Utc>,
    /// Size of `data` in bytes; duplicated here to avoid decoding for stats.
    pub size_bytes: u64,
    /// Opaque cached payload.
    pub data: Vec<u8>,
}

/// Kind of artefact stored in the cache.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum CacheKind {
    ThumbnailImage,
    PdfPagePreview,
    FileMetadata,
    SearchSnippet,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::codec::{bincode_decode, bincode_encode};

    fn now() -> DateTime<Utc> {
        // Chrono with full precision round-trips through bincode; pinning to
        // a nanosecond-rounded value removes sub-nanosecond noise.
        DateTime::<Utc>::from_timestamp_nanos(1_700_000_000_000_000_000)
    }

    fn roundtrip<T>(value: &T) -> T
    where
        T: bincode::Encode + bincode::Decode<()>,
    {
        let bytes = bincode_encode(value).unwrap();
        bincode_decode::<T>(&bytes).unwrap()
    }

    #[test]
    fn history_entry_roundtrip() {
        let entry = HistoryEntry {
            id: Uuid::nil(),
            timestamp: now(),
            action_id: "fs.move".into(),
            command_text: "orc fs move a b".into(),
            target: Some("a".into()),
            reversible_until: Some(now()),
            reverse_command: Some("orc fs move b a".into()),
            metadata: vec![1, 2, 3],
        };
        let back = roundtrip(&entry);
        assert_eq!(entry.id, back.id);
        assert_eq!(entry.timestamp, back.timestamp);
        assert_eq!(entry.action_id, back.action_id);
        assert_eq!(entry.metadata, back.metadata);
    }

    #[test]
    fn widget_instance_roundtrip() {
        let w = WidgetInstance {
            id: Uuid::nil(),
            widget_type: "weather".into(),
            workspace_id: Uuid::nil(),
            position: GridPosition { col: 1, row: 2 },
            size: WidgetSize::Free { w: 3, h: 4 },
            lifecycle: LifecycleState::Active,
            config: vec![9, 8, 7],
            created_at: now(),
            updated_at: now(),
        };
        let back = roundtrip(&w);
        assert_eq!(w.widget_type, back.widget_type);
        assert_eq!(w.position, back.position);
        assert_eq!(w.size, back.size);
        assert_eq!(w.lifecycle, back.lifecycle);
    }

    #[test]
    fn workspace_roundtrip() {
        let ws = Workspace {
            id: Uuid::nil(),
            name: "Work".into(),
            ordinal: 1,
            wallpaper: None,
            created_at: now(),
            updated_at: now(),
        };
        let back = roundtrip(&ws);
        assert_eq!(ws.name, back.name);
        assert_eq!(ws.ordinal, back.ordinal);
    }

    #[test]
    fn file_tag_roundtrip() {
        let tag = FileTag {
            path: "C:/docs/a.txt".into(),
            tags: vec!["work".into(), "urgent".into()],
            color_label: Some(ColorLabel::Red),
            starred: true,
            updated_at: now(),
        };
        let back = roundtrip(&tag);
        assert_eq!(tag.path, back.path);
        assert_eq!(tag.tags, back.tags);
        assert_eq!(tag.color_label, back.color_label);
        assert!(back.starred);
    }

    #[test]
    fn session_state_roundtrip() {
        let s = SessionState {
            active_workspace_id: Some(Uuid::nil()),
            open_file_manager_tabs: vec![FileManagerTab {
                id: Uuid::nil(),
                path: "C:/".into(),
                view_mode: ViewMode::Details,
                scroll_position: 0.25,
            }],
            open_terminal_sessions: vec![TerminalSession {
                id: Uuid::nil(),
                backend: TerminalBackend::Wsl("Ubuntu".into()),
                working_directory: Some("/home/me".into()),
                title: "bash".into(),
            }],
            last_saved_at: now(),
        };
        let back = roundtrip(&s);
        assert_eq!(s.open_file_manager_tabs.len(), back.open_file_manager_tabs.len());
        assert_eq!(s.open_terminal_sessions[0].backend, back.open_terminal_sessions[0].backend);
    }

    #[test]
    fn cache_entry_roundtrip() {
        let e = CacheEntry {
            key: [7u8; 32],
            kind: CacheKind::ThumbnailImage,
            created_at: now(),
            last_access_at: now(),
            size_bytes: 1024,
            data: vec![0, 1, 2, 3, 4],
        };
        let back = roundtrip(&e);
        assert_eq!(e.key, back.key);
        assert_eq!(e.kind, back.kind);
        assert_eq!(e.size_bytes, back.size_bytes);
        assert_eq!(e.data, back.data);
    }
}
