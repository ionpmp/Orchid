//! Pane payloads, sorting, persistence helpers, and the widget descriptor.

use super::*;

pub(crate) fn build_pane_payload(
    pane: &PaneState,
    pane_idx: u8,
    entries_map: &HashMap<Uuid, Arc<Vec<orchid_fs::FsEntry>>>,
    config: &FileManagerConfig,
    inner: &FileManagerInner,
) -> PanePayload {
    let tabs: Vec<TabPayload> = pane
        .tabs
        .iter()
        .enumerate()
        .map(|(tab_idx, tab)| {
            let entries = entries_map
                .get(&tab.id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let is_active_tab = tab_idx == pane.active_tab;
            build_tab_payload(tab, entries, config, inner, pane_idx, is_active_tab)
        })
        .collect();
    PanePayload {
        tabs,
        active_tab: pane.active_tab as u32,
    }
}

pub(crate) fn entry_display_name(name: &str, is_dir: bool, show_extensions: bool) -> String {
    if show_extensions || is_dir {
        return name.to_string();
    }
    match name.rfind('.') {
        Some(0) | None => name.to_string(),
        Some(i) => name[..i].to_string(),
    }
}

pub(crate) fn build_tab_payload(
    tab: &TabState,
    entries: &[orchid_fs::FsEntry],
    config: &FileManagerConfig,
    inner: &FileManagerInner,
    pane_idx: u8,
    is_active_tab: bool,
) -> TabPayload {
    let raw_path = tab.path.as_str();
    let (path_display, breadcrumbs) = if raw_path == "virtual:recycle" {
        let label = inner.deps.locale.tr("fm-virtual-recycle");
        (raw_path.to_string(), vec![(raw_path.to_string(), label)])
    } else if raw_path == "virtual:search" {
        (
            raw_path.to_string(),
            vec![(raw_path.to_string(), "Search".to_string())],
        )
    } else if let Some(id) = find::search_session_id(raw_path) {
        let label = inner
            .search_sessions
            .read()
            .get(&id)
            .map(|s| s.label.clone())
            .unwrap_or_else(|| "Search".to_string());
        (
            label.clone(),
            vec![
                ("virtual:search".to_string(), "Search".to_string()),
                (raw_path.to_string(), label),
            ],
        )
    } else {
        let crumbs = inner.navigator.breadcrumbs_for(&tab.path);
        (
            raw_path.to_string(),
            crumbs
                .into_iter()
                .map(|c| (c.path.as_str().to_string(), c.display_name))
                .collect(),
        )
    };

    let quick = tab.quick_filter.trim();
    let entries_filtered: Vec<&orchid_fs::FsEntry> = if quick.is_empty() {
        entries.iter().collect()
    } else {
        let q = quick.to_lowercase();
        entries
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&q))
            .collect()
    };

    let item_count = entries_filtered.len() as u32;
    // Format only the visible window of the active tab. Hidden tabs wait until shown.
    const DEFAULT_WINDOW: usize = 96;
    let (first, end) = if is_active_tab {
        let stored = inner.viewport_by_pane.read().get(&pane_idx).copied();
        clamp_entry_window(stored, entries_filtered.len(), DEFAULT_WINDOW)
    } else {
        (0, 0)
    };
    let entries_offset = first as u32;
    let locale = inner.deps.orchid_config.read().locale.clone();
    let thumb_cache = inner.thumbnail_rgba.read();
    let shell_cache = inner.shell_icon_rgba.read();
    let entry_payloads: Vec<EntryPayload> = entries_filtered[first..end]
        .iter()
        .copied()
        .map(|e| {
            let path_key = e.path.as_str();
            let icon_size = shell_icon_size_for_mode(tab.view_mode);
            let shell_key = shell_icon_cache_key(path_key, icon_size);
            // Prefer image previews; fall back to OS association icons.
            let (
                has_thumbnail,
                thumbnail_rgba,
                thumbnail_width,
                thumbnail_height,
                thumbnail_is_icon,
            ) = if let Some(t) = thumb_cache.get(path_key) {
                (
                    true,
                    Some(std::sync::Arc::clone(&t.rgba)),
                    t.width,
                    t.height,
                    false,
                )
            } else if let Some(t) = shell_cache.get(&shell_key) {
                (
                    true,
                    Some(std::sync::Arc::clone(&t.rgba)),
                    t.width,
                    t.height,
                    true,
                )
            } else {
                (false, None, 0, 0, false)
            };
            let is_dir = matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory);
            EntryPayload {
                path: path_key.to_string(),
                name: entry_display_name(&e.name, is_dir, config.show_extensions),
                is_dir,
                size_text: inner.deps.locale.format_byte_size(e.metadata.size),
                modified_text: e
                    .metadata
                    .modified
                    .map(|t| locale.format_datetime(t))
                    .unwrap_or_default(),
                type_text: orchid_fs::recycle_original_path(path_key)
                    .and_then(|orig| {
                        std::path::Path::new(&orig)
                            .parent()
                            .map(|p| p.display().to_string())
                            .filter(|s| !s.is_empty())
                            .or(Some(orig))
                    })
                    .unwrap_or_else(|| {
                        classify(
                            &inner.deps.locale,
                            &e.name,
                            matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory),
                        )
                    }),
                icon: if matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory) {
                    "folder".into()
                } else {
                    "file".into()
                },
                has_thumbnail,
                thumbnail_key: None,
                thumbnail_rgba,
                thumbnail_width,
                thumbnail_height,
                thumbnail_is_icon,
                is_selected: tab.selection.is_selected(path_key),
                is_hidden: e.metadata.hidden,
                is_encrypted: e.metadata.extended.is_encrypted,
                is_managed: e.metadata.extended.is_managed,
                is_starred: e.metadata.extended.starred,
                color_label: e.metadata.extended.color_label.map(color_label_to_str),
                tags: e.metadata.extended.tags.clone(),
            }
        })
        .collect();
    let selection_count = tab.selection.count() as u32;
    let selection_bytes = inner.selection_bytes_in_tab(tab);
    let managed_stats_guard = inner.managed_stats.read();
    let managed_stats = managed_stats_guard.get(tab.path.as_str());
    let (managed_files_tracked, managed_dedup_bytes) = managed_stats
        .map(|st| {
            let saved = st.logical_bytes.saturating_sub(st.physical_bytes);
            (Some(st.files_tracked as u32), Some(saved))
        })
        .unwrap_or((None, None));
    let error = if item_count == 0 {
        virtual_folders::empty_placeholder_for_path(tab.path.as_str())
            .map(String::from)
            .or_else(|| inner.tab_errors.read().get(&tab.id).cloned().flatten())
    } else {
        inner.tab_errors.read().get(&tab.id).cloned().flatten()
    };
    TabPayload {
        tab_id: tab.id.to_string(),
        path_display,
        breadcrumbs,
        can_go_back: !tab.history_back.is_empty(),
        can_go_forward: !tab.history_forward.is_empty(),
        view_mode: to_payload_mode(tab.view_mode),
        entries: entry_payloads,
        entries_offset,
        selection_count,
        item_count,
        selection_bytes,
        managed_files_tracked,
        managed_dedup_bytes,
        quick_filter: tab.quick_filter.clone(),
        is_loading: inner.loading_tabs.read().contains(&tab.id),
        error,
        sort_by: sort_by_to_u8(tab.sort_by),
        sort_descending: tab.sort_descending,
        branch_view: tab.branch_view,
    }
}

pub(crate) fn sort_by_to_u8(sort_by: SortBy) -> u8 {
    match sort_by {
        SortBy::Name => 0,
        SortBy::Size => 1,
        SortBy::Modified => 2,
        SortBy::Type => 3,
    }
}

pub(crate) fn sort_by_from_u8(column: u8) -> Option<SortBy> {
    match column {
        0 => Some(SortBy::Name),
        1 => Some(SortBy::Size),
        2 => Some(SortBy::Modified),
        3 => Some(SortBy::Type),
        _ => None,
    }
}

pub(crate) fn next_sort_by(current: SortBy) -> SortBy {
    match current {
        SortBy::Name => SortBy::Size,
        SortBy::Size => SortBy::Modified,
        SortBy::Modified => SortBy::Type,
        SortBy::Type => SortBy::Name,
    }
}

pub(crate) fn find_tab_by_id(state: &FileManagerState, tab_id: Uuid) -> Option<&TabState> {
    state
        .left_pane
        .tabs
        .iter()
        .find(|t| t.id == tab_id)
        .or_else(|| {
            state
                .right_pane
                .as_ref()
                .and_then(|p| p.tabs.iter().find(|t| t.id == tab_id))
        })
}

pub(crate) fn fs_event_paths(env: &orchid_core::EventEnvelope) -> Vec<orchid_fs::FsPath> {
    use orchid_fs::{FsCreatedEvent, FsDeletedEvent, FsModifiedEvent, FsRenamedEvent};
    if let Some(e) = env.downcast::<FsCreatedEvent>() {
        return vec![e.path.clone()];
    }
    if let Some(e) = env.downcast::<FsModifiedEvent>() {
        return vec![e.path.clone()];
    }
    if let Some(e) = env.downcast::<FsDeletedEvent>() {
        return vec![e.path.clone()];
    }
    if let Some(e) = env.downcast::<FsRenamedEvent>() {
        return vec![e.from.clone(), e.to.clone()];
    }
    Vec::new()
}

/// True when a filesystem event should refresh the listing of `dir`.
pub(crate) fn fs_event_affects_listing(event_path: &str, dir: &str) -> bool {
    if event_path == dir {
        return true;
    }
    match event_path.rsplit_once('/') {
        Some((parent, _)) => parent == dir,
        None => false,
    }
}

#[cfg(test)]
mod dir_watch_tests {
    use super::fs_event_affects_listing;

    #[test]
    fn listing_refresh_for_direct_children_only() {
        let dir = "local:c:/Users/me/Docs";
        assert!(fs_event_affects_listing(dir, dir));
        assert!(fs_event_affects_listing(
            "local:c:/Users/me/Docs/a.txt",
            dir
        ));
        assert!(!fs_event_affects_listing(
            "local:c:/Users/me/Docs/sub/a.txt",
            dir
        ));
        assert!(!fs_event_affects_listing("local:c:/Users/me/Other", dir));
    }
}

pub(crate) fn sort_entries(
    entries: &mut Vec<orchid_fs::FsEntry>,
    sort_by: SortBy,
    descending: bool,
) {
    use std::cmp::Ordering;

    // Precompute sort keys once — the previous comparator allocated lowercase
    // strings on every comparison (≈ O(n log n) allocs).
    let mut keyed: Vec<_> = std::mem::take(entries)
        .into_iter()
        .map(|e| {
            let is_dir = matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory);
            let name_key = e.name.to_lowercase();
            let size = e.metadata.size;
            let modified = e.metadata.modified.map(|t| t.timestamp()).unwrap_or(0);
            let ext_key = e
                .path
                .extension()
                .map(|ext| ext.to_lowercase())
                .unwrap_or_default();
            (e, is_dir, name_key, size, modified, ext_key)
        })
        .collect();

    keyed.sort_by(|a, b| {
        let dir_ord = match (a.1, b.1) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => Ordering::Equal,
        };
        if dir_ord != Ordering::Equal {
            return if descending {
                dir_ord.reverse()
            } else {
                dir_ord
            };
        }
        let field = match sort_by {
            SortBy::Name => a.2.cmp(&b.2),
            SortBy::Size => a.3.cmp(&b.3),
            SortBy::Modified => a.4.cmp(&b.4),
            SortBy::Type => a.5.cmp(&b.5).then_with(|| a.2.cmp(&b.2)),
        };
        if descending {
            field.reverse()
        } else {
            field
        }
    });

    *entries = keyed.into_iter().map(|(e, ..)| e).collect();
}

pub(crate) fn to_payload_mode(mode: ViewMode) -> FmViewMode {
    match mode {
        ViewMode::Icons => FmViewMode::Icons,
        ViewMode::List => FmViewMode::List,
        ViewMode::Details => FmViewMode::Details,
        ViewMode::Gallery => FmViewMode::Gallery,
    }
}

/// Max extension length shown in the Type column.
///
/// Longer "extensions" (e.g. `.dotnetUserLevelCache`) produce unreadable
/// labels like `DOTNETUSERLEVELCACHE file` that clip badly in a narrow column.
const TYPE_COLUMN_MAX_EXT_LEN: usize = 8;

/// Short file extension suitable for the Type column, if any.
pub(crate) fn type_column_extension(name: &str) -> Option<&str> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() || ext.len() > TYPE_COLUMN_MAX_EXT_LEN {
        return None;
    }
    if !ext
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '_')
    {
        return None;
    }
    Some(ext)
}

pub(crate) fn classify(locale: &orchid_i18n::LocaleManager, name: &str, is_dir: bool) -> String {
    if is_dir {
        return locale.tr("fm-properties-kind-folder");
    }
    // Prefer a short extension label over raw MIME (`image/png`) — Properties
    // already surfaces the MIME type. Unknown / absurdly long extensions fall
    // back to a generic "File" so the column stays readable.
    if let Some(ext) = type_column_extension(name) {
        return locale.tr_args(
            "fm-type-ext-file",
            &orchid_i18n::FluentArgs::new().with("ext", ext.to_ascii_uppercase()),
        );
    }
    locale.tr("fm-properties-kind-file")
}

pub(crate) fn is_image_entry(e: &orchid_fs::FsEntry) -> bool {
    if e.metadata
        .mime
        .as_deref()
        .map(|m| m.starts_with("image/"))
        .unwrap_or(false)
    {
        return true;
    }
    e.path
        .extension()
        .is_some_and(orchid_viewers::is_image_file_extension)
}

pub(crate) fn viewer_thumb_size(size: config::ThumbnailSize) -> orchid_viewers::ThumbnailSize {
    match size {
        config::ThumbnailSize::Small => orchid_viewers::ThumbnailSize::Small,
        config::ThumbnailSize::Medium => orchid_viewers::ThumbnailSize::Medium,
        config::ThumbnailSize::Large => orchid_viewers::ThumbnailSize::Large,
    }
}

pub(crate) fn color_label_to_str(label: orchid_storage::ColorLabel) -> String {
    match label {
        orchid_storage::ColorLabel::Red => "red",
        orchid_storage::ColorLabel::Orange => "orange",
        orchid_storage::ColorLabel::Yellow => "yellow",
        orchid_storage::ColorLabel::Green => "green",
        orchid_storage::ColorLabel::Blue => "blue",
        orchid_storage::ColorLabel::Purple => "purple",
        orchid_storage::ColorLabel::Gray => "gray",
    }
    .to_string()
}

pub(crate) fn color_label_from_action_id(action_id: &str) -> Option<orchid_storage::ColorLabel> {
    use orchid_storage::ColorLabel;
    match action_id.strip_prefix("fs.color-label:") {
        Some("red") => Some(ColorLabel::Red),
        Some("orange") => Some(ColorLabel::Orange),
        Some("yellow") => Some(ColorLabel::Yellow),
        Some("green") => Some(ColorLabel::Green),
        Some("blue") => Some(ColorLabel::Blue),
        Some("purple") => Some(ColorLabel::Purple),
        Some("gray") => Some(ColorLabel::Gray),
        Some("none") | Some("clear") => None,
        _ => None,
    }
}

/// Descriptor with a default initial path of the user's home directory.
#[must_use]
pub fn descriptor(deps: FileManagerDeps) -> WidgetDescriptor {
    let default_path = default_initial_path();
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, bytes| {
        let persisted = match bytes {
            Some(b) if !b.is_empty() => decode_persisted(b)?,
            _ => FileManagerPersisted {
                config: FileManagerConfig::default(),
                session: None,
                path_visits: Vec::new(),
            },
        };
        Ok(Box::new(FileManagerWidget::from_persisted(
            ctx.instance_id,
            deps.clone(),
            ctx.bus.clone(),
            persisted,
            default_path.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-fm-name",
        description_key: "widget-fm-desc",
        icon_name: "file-manager",
        category: WidgetCategory::Productivity,
        default_size: WidgetSize::ExtraLarge,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}

pub(crate) fn default_initial_path() -> orchid_fs::FsPath {
    if let Some(home) = dirs_home() {
        if let Ok(p) = orchid_fs::FsPath::from_local(&home) {
            return p;
        }
    }
    orchid_fs::FsPath::new("local:/").unwrap_or_else(|_| {
        // unreachable in practice; fall back to a known-good absolute path.
        orchid_fs::FsPath::new("local:c:/").expect("constant path parses")
    })
}

pub(crate) fn parse_persisted_path(path: &str, fallback: &orchid_fs::FsPath) -> orchid_fs::FsPath {
    orchid_fs::FsPath::new(path).unwrap_or_else(|_| fallback.clone())
}

pub(crate) fn tab_from_persisted(pt: &PersistedTab, fallback: &orchid_fs::FsPath) -> TabState {
    let id = Uuid::parse_str(&pt.id).unwrap_or_else(|_| Uuid::new_v4());
    TabState {
        id,
        path: parse_persisted_path(&pt.path, fallback),
        history_back: pt
            .history_back
            .iter()
            .filter_map(|p| orchid_fs::FsPath::new(p).ok())
            .collect(),
        history_forward: pt
            .history_forward
            .iter()
            .filter_map(|p| orchid_fs::FsPath::new(p).ok())
            .collect(),
        view_mode: pt.view_mode,
        selection: SelectionModel::new(),
        quick_filter: String::new(),
        scroll_position: 0.0,
        sort_by: pt.sort_by,
        sort_descending: pt.sort_descending,
        branch_view: false,
    }
}

pub(crate) fn tab_to_persisted(tab: &TabState) -> PersistedTab {
    PersistedTab {
        id: tab.id.to_string(),
        path: tab.path.as_str().to_string(),
        history_back: tab
            .history_back
            .iter()
            .map(|p| p.as_str().to_string())
            .collect(),
        history_forward: tab
            .history_forward
            .iter()
            .map(|p| p.as_str().to_string())
            .collect(),
        view_mode: tab.view_mode,
        sort_by: tab.sort_by,
        sort_descending: tab.sort_descending,
    }
}

pub(crate) fn pane_from_persisted(pane: &PersistedPane, fallback: &orchid_fs::FsPath) -> PaneState {
    let tabs: Vec<TabState> = pane
        .tabs
        .iter()
        .map(|t| tab_from_persisted(t, fallback))
        .collect();
    if tabs.is_empty() {
        return PaneState::with_single_tab(TabState::new(
            fallback.clone(),
            ViewMode::Details,
            SortBy::Name,
        ));
    }
    PaneState {
        active_tab: pane.active_tab.min(tabs.len().saturating_sub(1)),
        tabs,
    }
}

pub(crate) fn pane_to_persisted(pane: &PaneState) -> PersistedPane {
    PersistedPane {
        tabs: pane.tabs.iter().map(tab_to_persisted).collect(),
        active_tab: pane.active_tab,
    }
}

pub(crate) fn state_from_persisted(
    config: &FileManagerConfig,
    session: Option<&FileManagerSession>,
    fallback_path: orchid_fs::FsPath,
) -> FileManagerState {
    let Some(session) = session else {
        return FileManagerState::single_pane(
            fallback_path,
            config.default_view_mode,
            config.sort_by,
        );
    };

    let left_pane = pane_from_persisted(&session.left_pane, &fallback_path);
    let mut right_pane = session
        .right_pane
        .as_ref()
        .map(|p| pane_from_persisted(p, &left_pane.active_tab().path));

    if config.dual_pane && right_pane.is_none() {
        let path = left_pane.active_tab().path.clone();
        right_pane = Some(PaneState::with_single_tab(TabState::new(
            path,
            config.default_view_mode,
            config.sort_by,
        )));
    } else if !config.dual_pane {
        right_pane = None;
    }

    let active_pane = match session.active_pane {
        PersistedActivePane::Left => ActivePane::Left,
        PersistedActivePane::Right if config.dual_pane && right_pane.is_some() => ActivePane::Right,
        PersistedActivePane::Right => ActivePane::Left,
    };

    FileManagerState {
        left_pane,
        right_pane,
        active_pane,
    }
}

pub(crate) fn session_from_state(state: &FileManagerState) -> FileManagerSession {
    FileManagerSession {
        left_pane: pane_to_persisted(&state.left_pane),
        right_pane: state.right_pane.as_ref().map(pane_to_persisted),
        active_pane: match state.active_pane {
            ActivePane::Left => PersistedActivePane::Left,
            ActivePane::Right => PersistedActivePane::Right,
        },
    }
}

pub(crate) fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var("USERPROFILE")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var("HOME").ok().map(std::path::PathBuf::from))
}

pub(crate) fn live_inner(instance_id: Uuid) -> WidgetResult<Arc<FileManagerInner>> {
    FM_LIVE
        .get(&instance_id)
        .map(|e| Arc::clone(e.value()))
        .ok_or_else(|| WidgetError::InvalidStateForOperation("file-manager widget not live".into()))
}

pub(crate) fn map_fs_error(e: orchid_fs::FsError) -> WidgetError {
    WidgetError::InvalidStateForOperation(e.to_string())
}

pub(crate) async fn recycle_purge_action(
    inner: &Arc<FileManagerInner>,
    target_paths: Vec<String>,
    skip_confirm: bool,
) -> WidgetResult<ActionOutcome> {
    let paths: Vec<String> = target_paths
        .into_iter()
        .filter(|p| orchid_fs::is_recycle_item(p))
        .collect();
    if paths.is_empty() {
        return Ok(ActionOutcome::Done);
    }
    if inner.config.read().confirm_delete && !skip_confirm {
        return Ok(ActionOutcome::NeedsConfirmation {
            message: "fm-confirm-recycle-purge".into(),
            action_id: "fs.recycle-purge".into(),
            paths,
        });
    }
    orchid_fs::purge_recycle(&paths)
        .await
        .map_err(map_fs_error)?;
    inner.refresh_all_tabs().await;
    Ok(ActionOutcome::Done)
}

#[cfg(test)]
mod display_name_tests {
    use super::entry_display_name;

    #[test]
    fn keeps_extension_when_show_extensions() {
        assert_eq!(entry_display_name("readme.txt", false, true), "readme.txt");
    }

    #[test]
    fn strips_extension_when_hidden() {
        assert_eq!(entry_display_name("readme.txt", false, false), "readme");
    }

    #[test]
    fn keeps_dir_names_unchanged() {
        assert_eq!(entry_display_name("folder.txt", true, false), "folder.txt");
    }

    #[test]
    fn keeps_dotfiles_unchanged() {
        assert_eq!(entry_display_name(".gitignore", false, false), ".gitignore");
    }

    #[test]
    fn keeps_extensionless_files() {
        assert_eq!(entry_display_name("Makefile", false, false), "Makefile");
    }
}

#[cfg(test)]
mod type_column_tests {
    use super::type_column_extension;

    #[test]
    fn short_extensions_are_kept() {
        assert_eq!(type_column_extension("readme.txt"), Some("txt"));
        assert_eq!(type_column_extension("photo.jpeg"), Some("jpeg"));
        assert_eq!(type_column_extension("archive.tar.gz"), Some("gz"));
        assert_eq!(type_column_extension("Component.tsx"), Some("tsx"));
    }

    #[test]
    fn long_cache_extensions_are_rejected() {
        assert_eq!(
            type_column_extension("MachineId.v1.dotnetUserLevelCache"),
            None
        );
        assert_eq!(
            type_column_extension("7.0.302_IsDockerContainer.dotnetUserLevelCache"),
            None
        );
    }

    #[test]
    fn dotfiles_and_extensionless_are_rejected() {
        assert_eq!(type_column_extension(".gitignore"), None);
        assert_eq!(type_column_extension("Makefile"), None);
        assert_eq!(type_column_extension("trailing."), None);
    }
}

#[cfg(test)]
mod viewport_window_tests {
    use super::super::clamp_entry_window;

    #[test]
    fn missing_window_starts_at_top() {
        assert_eq!(clamp_entry_window(None, 200, 96), (0, 96));
        assert_eq!(clamp_entry_window(None, 40, 96), (0, 40));
    }

    #[test]
    fn stale_window_past_end_resets_to_top() {
        assert_eq!(clamp_entry_window(Some((200, 350)), 50, 96), (0, 50));
        assert_eq!(clamp_entry_window(Some((50, 50)), 50, 96), (0, 50));
    }

    #[test]
    fn valid_window_is_clamped_to_len() {
        assert_eq!(clamp_entry_window(Some((200, 350)), 300, 96), (200, 300));
        assert_eq!(clamp_entry_window(Some((0, 96)), 80, 96), (0, 80));
    }

    #[test]
    fn small_listing_ignores_stale_mid_window() {
        assert_eq!(clamp_entry_window(Some((10, 106)), 50, 96), (0, 50));
        assert_eq!(clamp_entry_window(Some((10, 106)), 90, 96), (10, 90));
    }
}
