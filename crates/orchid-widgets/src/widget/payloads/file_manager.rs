//! File-manager widget payload.

/// Top-level file-manager payload.
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct FileManagerPayload {
    pub panes: Vec<PanePayload>,
    pub active_pane: u8,
    pub dual_pane: bool,
    /// Staged clipboard entry count (`0` = hidden).
    pub clipboard_count: u32,
    /// `true` when clipboard holds a cut (not copy).
    pub clipboard_is_cut: bool,
    /// Registered managed folders (path + ingest stats for sidebar).
    pub managed_folders: Vec<ManagedFolderSidebarPayload>,
    /// Configured network mounts (name + canonical URI) for sidebar / virtual folder.
    pub network_mounts: Vec<NetworkMountPayload>,
    /// Short-lived ingest activity label (file name).
    pub activity_indicator: Option<String>,
    /// Number of managed files currently being ingested.
    pub ingest_in_flight: u32,
    /// Copy/move transfer in progress.
    pub transfer_active: bool,
    /// 0.0–1.0 byte progress when [`Self::transfer_active`].
    pub transfer_progress: f32,
    /// True when the active transfer is a copy (false = move).
    pub transfer_is_copy: bool,
    /// File name currently being copied or moved.
    pub transfer_current: Option<String>,
    /// Recent transfer failure (raw message; localize in UI).
    pub transfer_error: Option<String>,
    /// Passphrase dialog failure (raw message; localize in UI).
    pub passphrase_error: Option<String>,
    /// Recent managed ingest failure (file name; localize in UI).
    pub ingest_error: Option<String>,
    /// Short-lived localized notice key (encrypt/decrypt/managed).
    pub activity_notice_key: Option<String>,
    pub activity_notice_name: Option<String>,
}

/// One configured network mount surfaced in the FM payload.
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct NetworkMountPayload {
    pub name: String,
    pub uri: String,
}

/// One managed folder surfaced in the FM sidebar.
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct ManagedFolderSidebarPayload {
    pub path: String,
    pub files_tracked: u32,
    pub dedup_bytes: u64,
}

/// One pane (left or right) with its tabs.
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct PanePayload {
    pub tabs: Vec<TabPayload>,
    pub active_tab: u32,
}

/// A single tab inside a pane.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct TabPayload {
    pub tab_id: String,
    pub path_display: String,
    pub breadcrumbs: Vec<(String, String)>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub view_mode: FmViewMode,
    pub entries: Vec<EntryPayload>,
    pub selection_count: u32,
    pub item_count: u32,
    /// Managed-folder ingest stats when viewing a registered root.
    pub managed_files_tracked: Option<u32>,
    pub managed_dedup_bytes: Option<u64>,
    pub quick_filter: String,
    pub is_loading: bool,
    pub error: Option<String>,
    /// Sort column index: 0 name, 1 size, 2 modified, 3 type.
    pub sort_by: u8,
    pub sort_descending: bool,
}

/// One entry row.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct EntryPayload {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size_text: String,
    pub modified_text: String,
    pub type_text: String,
    pub icon: String,
    pub has_thumbnail: bool,
    pub thumbnail_key: Option<String>,
    /// RGBA8 pixels when a thumbnail was generated for icon/gallery modes.
    pub thumbnail_rgba: Option<Vec<u8>>,
    pub thumbnail_width: u32,
    pub thumbnail_height: u32,
    pub is_selected: bool,
    pub is_hidden: bool,
    pub is_encrypted: bool,
    pub is_managed: bool,
    pub is_starred: bool,
    pub color_label: Option<String>,
    pub tags: Vec<String>,
}

/// View mode shown in the pane.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FmViewMode {
    Icons,
    List,
    #[default]
    Details,
    Gallery,
}
