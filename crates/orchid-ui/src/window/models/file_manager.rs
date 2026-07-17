//! File manager widget Slint model builders.

use orchid_i18n::LocaleManager;
use slint::{Image, ModelRc, SharedString, VecModel};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::super::errors::fm_localized_error;
use crate::slint_generated::{
    FileManagerModel, FmBreadcrumb, FmConfirmDialog, FmContextAction, FmContextMenu,
    FmContextSubitem, FmEntry, FmManagedPolicyRow, FmManagedPolicyState, FmPane,
    FmPassphraseState, FmRenameState, FmSidebarItem, FmTab, FmTagChip, FmTagState,
};

/// Reuse Slint thumb images when the underlying RGBA `Arc` is unchanged.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct FmThumbCacheKey {
    ptr: usize,
    len: usize,
    width: u32,
    height: u32,
    tip: u64,
}

struct FmThumbCacheEntry {
    image: Image,
}

thread_local! {
    static FM_THUMB_CACHE: std::cell::RefCell<HashMap<FmThumbCacheKey, FmThumbCacheEntry>> =
        std::cell::RefCell::new(HashMap::new());
}
const FM_THUMB_CACHE_CAP: usize = 64;

fn fm_thumb_tip(bytes: &[u8]) -> u64 {
    let mut tip = 0u64;
    for (i, b) in bytes.iter().take(8).enumerate() {
        tip |= u64::from(*b) << (i * 8);
    }
    if bytes.len() > 8 {
        let mut tail = 0u64;
        for (i, b) in bytes.iter().rev().take(8).enumerate() {
            tail |= u64::from(*b) << (i * 8);
        }
        tip ^= tail.rotate_left(17);
    }
    tip
}

fn fm_rgba_to_image(rgba: &Arc<Vec<u8>>, width: u32, height: u32) -> Image {
    if width == 0 || height == 0 || rgba.is_empty() {
        return Image::default();
    }
    let key = FmThumbCacheKey {
        ptr: Arc::as_ptr(rgba) as usize,
        len: rgba.len(),
        width,
        height,
        tip: fm_thumb_tip(rgba.as_slice()),
    };
    let cached = FM_THUMB_CACHE.with(|cache| cache.borrow().get(&key).map(|c| c.image.clone()));
    if let Some(image) = cached {
        return image;
    }
    let buf =
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba.as_slice(), width, height);
    let image = Image::from_rgba8(buf);
    FM_THUMB_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= FM_THUMB_CACHE_CAP {
            if let Some(old) = cache.keys().next().copied() {
                cache.remove(&old);
            }
        }
        cache.insert(key, FmThumbCacheEntry { image: image.clone() });
    });
    image
}
#[derive(Clone)]
pub(crate) struct FileManagerOverlays {
    pub(crate) context_menu: FmContextMenu,
    pub(crate) confirm_dialog: FmConfirmDialog,
    pub(crate) rename: FmRenameState,
    pub(crate) tag: FmTagState,
    pub(crate) tag_paths: Vec<String>,
    pub(crate) passphrase: FmPassphraseState,
    pub(crate) managed_policy: FmManagedPolicyState,
    pub(crate) passphrase_paths: Vec<String>,
    pub(crate) passphrase_purpose: Option<orchid_widgets::builtin::file_manager::PassphrasePurpose>,
    pub(crate) create_folder_parent: Option<String>,
    pub(crate) drag_active: bool,
    pub(crate) drag_paths: Vec<String>,
    pub(crate) drag_drop_target: String,
    pub(crate) drag_target_pane: i32,
}

pub(crate) fn empty_file_manager_model(locale: &LocaleManager) -> FileManagerModel {
    FileManagerModel {
        panes: ModelRc::new(VecModel::default()),
        active_pane: 0,
        dual_pane: false,
        dual_pane_label: locale.tr("fm-dual-pane-on").into(),
        clipboard_indicator: SharedString::new(),
        activity_indicator: SharedString::new(),
        transfer_active: false,
        transfer_progress: 0.0,
        sidebar_items: build_sidebar_items(locale, "", &[], &[]),
        context_menu: empty_context_menu(),
        confirm_dialog: empty_confirm_dialog(),
        rename: empty_rename_state(),
        tag: empty_tag_state(),
        passphrase: empty_passphrase_state(),
        managed_policy: empty_managed_policy_state(),
        show_hidden: false,
        show_hidden_label: locale.tr("fm-show-hidden-off").into(),
        single_click_open: false,
        single_click_open_label: locale.tr("fm-click-single-off").into(),
        request_autofocus: false,
        drag_active: false,
        drag_drop_target: SharedString::new(),
        drag_target_pane: -1,
    }
}

pub(crate) fn fm_passphrase_dialog_labels(
    locale: &LocaleManager,
    purpose: orchid_widgets::builtin::file_manager::PassphrasePurpose,
) -> (String, String, String) {
    use orchid_widgets::builtin::file_manager::PassphrasePurpose;
    match purpose {
        PassphrasePurpose::Encrypt => (
            locale.tr("fm-encrypt-title"),
            locale.tr("fm-passphrase-encrypt-hint"),
            locale.tr("fm-action-encrypt"),
        ),
        PassphrasePurpose::Decrypt => (
            locale.tr("fm-decrypt-title"),
            locale.tr("fm-passphrase-decrypt-hint"),
            locale.tr("fm-action-decrypt"),
        ),
        PassphrasePurpose::Reveal | PassphrasePurpose::RevealInViewer => (
            locale.tr("fm-reveal-title"),
            locale.tr("fm-passphrase-reveal-hint"),
            locale.tr("fm-action-reveal"),
        ),
    }
}

pub(crate) fn empty_passphrase_state() -> FmPassphraseState {
    FmPassphraseState {
        active: false,
        proposed_passphrase: SharedString::new(),
        title: SharedString::new(),
        hint: SharedString::new(),
        ok_label: SharedString::new(),
        cancel_label: SharedString::new(),
        biometric_available: false,
        biometric_label: SharedString::new(),
    }
}

pub(crate) fn empty_managed_policy_state() -> FmManagedPolicyState {
    FmManagedPolicyState {
        active: false,
        title: SharedString::new(),
        path: SharedString::new(),
        rows: ModelRc::new(VecModel::default()),
        close_label: SharedString::new(),
    }
}

pub(crate) fn build_managed_policy_state(
    locale: &LocaleManager,
    path: &str,
    policy: Option<&orchid_fs::ManagedFolderPolicy>,
) -> FmManagedPolicyState {
    let policy = policy.cloned().unwrap_or_default();
    let max_size = policy
        .max_size_bytes
        .map(|n| locale.format_byte_size(n))
        .unwrap_or_else(|| locale.tr("fm-policy-unlimited"));
    let retention = policy
        .retention_days
        .map(|d| {
            locale.tr_args(
                "fm-policy-retention-days",
                &orchid_i18n::FluentArgs::new().with("days", d.to_string()),
            )
        })
        .unwrap_or_else(|| locale.tr("fm-policy-forever"));
    let excludes = if policy.exclude_patterns.is_empty() {
        locale.tr("fm-policy-none")
    } else {
        policy.exclude_patterns.join(", ")
    };
    let rows = vec![
        FmManagedPolicyRow {
            label: locale.tr("fm-policy-max-size").into(),
            value: max_size.into(),
        },
        FmManagedPolicyRow {
            label: locale.tr("fm-policy-retention").into(),
            value: retention.into(),
        },
        FmManagedPolicyRow {
            label: locale.tr("fm-policy-excludes").into(),
            value: excludes.into(),
        },
    ];
    FmManagedPolicyState {
        active: true,
        title: locale.tr("fm-managed-policy-title").into(),
        path: path.into(),
        rows: ModelRc::new(VecModel::from(rows)),
        close_label: locale.tr("fm-info-close").into(),
    }
}

pub(crate) fn empty_tag_state() -> FmTagState {
    FmTagState {
        active: false,
        proposed_tag: SharedString::new(),
        title: SharedString::new(),
        ok_label: SharedString::new(),
        cancel_label: SharedString::new(),
    }
}

pub(crate) fn empty_context_menu() -> FmContextMenu {
    FmContextMenu {
        visible: false,
        x: 0.0,
        y: 0.0,
        actions: ModelRc::new(VecModel::default()),
        target_paths: ModelRc::new(VecModel::default()),
    }
}

pub(crate) fn empty_confirm_dialog() -> FmConfirmDialog {
    FmConfirmDialog {
        visible: false,
        title: SharedString::new(),
        message: SharedString::new(),
        confirm_label: SharedString::new(),
        cancel_label: SharedString::new(),
        pending_action: SharedString::new(),
        pending_paths: ModelRc::new(VecModel::default()),
    }
}

pub(crate) fn empty_rename_state() -> FmRenameState {
    FmRenameState {
        active: false,
        path: SharedString::new(),
        proposed_name: SharedString::new(),
        title: SharedString::new(),
        ok_label: SharedString::new(),
        cancel_label: SharedString::new(),
    }
}

fn fm_sidebar_id_for_path(path: &str) -> Option<&'static str> {
    match path {
        "virtual:recent" => Some("fav:recent"),
        "virtual:starred" => Some("fav:starred"),
        "virtual:tags" => Some("fav:tags"),
        "virtual:categories/images" => Some("cat:images"),
        "virtual:categories/documents" => Some("cat:documents"),
        "virtual:categories/video" => Some("cat:video"),
        "virtual:categories/audio" => Some("cat:audio"),
        "virtual:categories/archives" => Some("cat:archives"),
        "virtual:network" => Some("net:places"),
        _ => None,
    }
}

fn managed_sidebar_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.to_string())
}

fn managed_sidebar_label(
    locale: &LocaleManager,
    folder: &orchid_widgets::ManagedFolderSidebarPayload,
) -> String {
    let name = managed_sidebar_name(&folder.path);
    let has_policy = folder.policy_max_bytes.is_some()
        || folder.policy_retention_days.is_some()
        || folder.policy_exclude_count > 0;
    if folder.files_tracked > 0 {
        if has_policy {
            locale.tr_args(
                "fm-sidebar-managed-folder-policy",
                &orchid_i18n::FluentArgs::new()
                    .with("name", name.as_str())
                    .with("count", folder.files_tracked.to_string())
                    .with("dedup", locale.format_byte_size(folder.dedup_bytes)),
            )
        } else {
            locale.tr_args(
                "fm-sidebar-managed-folder",
                &orchid_i18n::FluentArgs::new()
                    .with("name", name.as_str())
                    .with("count", folder.files_tracked.to_string())
                    .with("dedup", locale.format_byte_size(folder.dedup_bytes)),
            )
        }
    } else if has_policy {
        locale.tr_args(
            "fm-sidebar-managed-policy-only",
            &orchid_i18n::FluentArgs::new().with("name", name.as_str()),
        )
    } else {
        name
    }
}

fn active_managed_sidebar_index(
    active_path: &str,
    managed_folders: &[orchid_widgets::ManagedFolderSidebarPayload],
) -> Option<usize> {
    let p = std::path::Path::new(active_path);
    for (i, folder) in managed_folders.iter().enumerate() {
        let r = std::path::Path::new(&folder.path);
        if p == r || p.starts_with(r) {
            return Some(i);
        }
    }
    None
}

fn active_network_sidebar_index(
    active_path: &str,
    network_mounts: &[orchid_widgets::NetworkMountPayload],
) -> Option<usize> {
    network_mounts
        .iter()
        .position(|m| m.uri == active_path)
}

fn fm_build_tab_status_text(locale: &LocaleManager, t: &orchid_widgets::TabPayload) -> String {
    if let (Some(tracked), Some(dedup_bytes)) =
        (t.managed_files_tracked, t.managed_dedup_bytes)
    {
        locale.tr_args(
            "fm-status-managed",
            &orchid_i18n::FluentArgs::new()
                .with("items", t.item_count.to_string())
                .with("selected", t.selection_count.to_string())
                .with("tracked", tracked.to_string())
                .with("dedup", locale.format_byte_size(dedup_bytes)),
        )
    } else {
        locale.tr_args(
            "fm-status-bar",
            &orchid_i18n::FluentArgs::new()
                .with("items", t.item_count.to_string())
                .with("selected", t.selection_count.to_string()),
        )
    }
}

fn fm_virtual_path_display(locale: &LocaleManager, path: &str) -> String {
    orchid_widgets::builtin::file_manager::label_key_for_virtual_path(path)
        .map(|key| locale.tr(key))
        .unwrap_or_else(|| path.to_string())
}

fn fm_virtual_breadcrumb_label(locale: &LocaleManager, path: &str, fallback: &str) -> String {
    orchid_widgets::builtin::file_manager::label_key_for_virtual_path(path)
        .map(|key| locale.tr(key))
        .unwrap_or_else(|| fallback.to_string())
}

fn fm_tab_error_text(locale: &LocaleManager, error: Option<&str>) -> SharedString {
    error
        .map(|e| fm_localized_error(locale, e).into())
        .unwrap_or_default()
}

fn fm_tab_error_action_label(locale: &LocaleManager, error: Option<&str>) -> SharedString {
    match error {
        Some("network-placeholder") => locale.tr("settings-open-config-file").into(),
        _ => SharedString::default(),
    }
}

pub(crate) fn build_sidebar_items(
    locale: &LocaleManager,
    active_path: &str,
    managed_folders: &[orchid_widgets::ManagedFolderSidebarPayload],
    network_mounts: &[orchid_widgets::NetworkMountPayload],
) -> ModelRc<FmSidebarItem> {
    let active_id = fm_sidebar_id_for_path(active_path);
    let active_managed = active_managed_sidebar_index(active_path, managed_folders);
    let active_network = active_network_sidebar_index(active_path, network_mounts);
    let mut items = vec![
        FmSidebarItem {
            id: "section:favorites".into(),
            label: locale.tr("fm-sidebar-favorites").into(),
            icon: "sidebar-favorites".into(),
            indent: 0,
            is_section_header: true,
            is_active: false,
        },
        FmSidebarItem {
            id: "fav:starred".into(),
            label: locale.tr("fm-virtual-starred").into(),
            icon: "sidebar-starred".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("fav:starred"),
        },
        FmSidebarItem {
            id: "fav:tags".into(),
            label: locale.tr("fm-virtual-tags").into(),
            icon: "sidebar-tags".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("fav:tags"),
        },
        FmSidebarItem {
            id: "fav:recent".into(),
            label: locale.tr("fm-virtual-recent").into(),
            icon: "sidebar-recent".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("fav:recent"),
        },
        FmSidebarItem {
            id: "section:categories".into(),
            label: locale.tr("fm-sidebar-categories").into(),
            icon: "sidebar-categories".into(),
            indent: 0,
            is_section_header: true,
            is_active: false,
        },
        FmSidebarItem {
            id: "cat:images".into(),
            label: locale.tr("fm-category-images").into(),
            icon: "sidebar-images".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("cat:images"),
        },
        FmSidebarItem {
            id: "cat:documents".into(),
            label: locale.tr("fm-category-documents").into(),
            icon: "sidebar-documents".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("cat:documents"),
        },
        FmSidebarItem {
            id: "cat:video".into(),
            label: locale.tr("fm-category-video").into(),
            icon: "sidebar-video".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("cat:video"),
        },
        FmSidebarItem {
            id: "cat:audio".into(),
            label: locale.tr("fm-category-audio").into(),
            icon: "sidebar-audio".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("cat:audio"),
        },
        FmSidebarItem {
            id: "cat:archives".into(),
            label: locale.tr("fm-category-archives").into(),
            icon: "sidebar-archives".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("cat:archives"),
        },
    ];
    if !managed_folders.is_empty() {
        items.push(FmSidebarItem {
            id: "section:managed".into(),
            label: locale.tr("fm-sidebar-managed").into(),
            icon: "sidebar-managed".into(),
            indent: 0,
            is_section_header: true,
            is_active: false,
        });
        for (i, folder) in managed_folders.iter().enumerate() {
            items.push(FmSidebarItem {
                id: format!("managed:{i}").into(),
                label: managed_sidebar_label(locale, folder).into(),
                icon: "sidebar-managed".into(),
                indent: 1,
                is_section_header: false,
                is_active: active_managed == Some(i),
            });
        }
    }
    items.push(FmSidebarItem {
        id: "section:network".into(),
        label: locale.tr("fm-sidebar-network").into(),
        icon: "sidebar-network".into(),
        indent: 0,
        is_section_header: true,
        is_active: false,
    });
    items.push(FmSidebarItem {
        id: "net:places".into(),
        label: locale.tr("fm-sidebar-network-all").into(),
        icon: "sidebar-network".into(),
        indent: 1,
        is_section_header: false,
        is_active: active_id == Some("net:places") && active_network.is_none(),
    });
    for (i, mount) in network_mounts.iter().enumerate() {
        items.push(FmSidebarItem {
            id: format!("net:{i}").into(),
            label: mount.name.clone().into(),
            icon: "sidebar-mount".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_network == Some(i),
        });
    }
    ModelRc::new(VecModel::from(items))
}

pub(crate) fn build_file_manager_model(
    p: &orchid_widgets::FileManagerPayload,
    overlays: FileManagerOverlays,
    instance_id: Uuid,
    locale: &LocaleManager,
    request_autofocus: bool,
) -> FileManagerModel {
    let active_path = p
        .panes
        .get(p.active_pane as usize)
        .and_then(|pp| pp.tabs.get(pp.active_tab as usize))
        .map(|t| t.path_display.clone())
        .unwrap_or_default();
    let sidebar_items =
        build_sidebar_items(locale, &active_path, &p.managed_folders, &p.network_mounts);
    let sort_name_label = locale.tr("fm-sort-name");
    let sort_size_label = locale.tr("fm-sort-size");
    let sort_modified_label = locale.tr("fm-sort-modified");
    let sort_type_label = locale.tr("fm-sort-type");
    let panes: Vec<FmPane> = p
        .panes
        .iter()
        .map(|pp| {
            let tabs: Vec<FmTab> = pp
                .tabs
                .iter()
                .map(|t| {
                    let entries: Vec<FmEntry> = t
                        .entries
                        .iter()
                        .map(|e| {
                            let tags: Vec<FmTagChip> = e
                                .tags
                                .iter()
                                .map(|tag| FmTagChip {
                                    label: tag.clone().into(),
                                    color: slint::Color::from_argb_u8(255, 0x4d, 0x82, 0xff),
                                })
                                .collect();
                            let thumb_img = if e.has_thumbnail {
                                e.thumbnail_rgba
                                    .as_ref()
                                    .map(|rgba| fm_rgba_to_image(rgba, e.thumbnail_width, e.thumbnail_height))
                                    .unwrap_or_default()
                            } else {
                                Image::default()
                            };
                            FmEntry {
                                path: e.path.clone().into(),
                                name: e.name.clone().into(),
                                is_dir: e.is_dir,
                                size_text: e.size_text.clone().into(),
                                modified_text: e.modified_text.clone().into(),
                                type_text: e.type_text.clone().into(),
                                icon: e.icon.clone().into(),
                                has_thumbnail: e.has_thumbnail,
                                thumbnail_key: e.thumbnail_key.clone().unwrap_or_default().into(),
                                thumbnail: thumb_img,
                                is_selected: e.is_selected,
                                is_hidden: e.is_hidden,
                                is_encrypted: e.is_encrypted,
                                is_managed: e.is_managed,
                                is_starred: e.is_starred,
                                color_label: e.color_label.clone().unwrap_or_default().into(),
                                tags: ModelRc::new(VecModel::from(tags)),
                            }
                        })
                        .collect();

                    let breadcrumbs: Vec<FmBreadcrumb> = t
                        .breadcrumbs
                        .iter()
                        .map(|(bp, bl)| FmBreadcrumb {
                            path: bp.clone().into(),
                            label: fm_virtual_breadcrumb_label(locale, bp, bl).into(),
                        })
                        .collect();

                    FmTab {
                        id: t.tab_id.clone().into(),
                        path_display: fm_virtual_path_display(locale, &t.path_display).into(),
                        breadcrumbs: ModelRc::new(VecModel::from(breadcrumbs)),
                        can_back: t.can_go_back,
                        can_forward: t.can_go_forward,
                        view_mode: view_mode_to_int(t.view_mode),
                        entries: ModelRc::new(VecModel::from(entries)),
                        selection_count: t.selection_count as i32,
                        status_text: fm_build_tab_status_text(locale, t).into(),
                        quick_filter: t.quick_filter.clone().into(),
                        is_loading: t.is_loading,
                        error: fm_tab_error_text(locale, t.error.as_deref()),
                        error_action_label: fm_tab_error_action_label(locale, t.error.as_deref()),
                        sort_by: t.sort_by as i32,
                        sort_descending: t.sort_descending,
                        sort_name_label: sort_name_label.clone().into(),
                        sort_size_label: sort_size_label.clone().into(),
                        sort_modified_label: sort_modified_label.clone().into(),
                        sort_type_label: sort_type_label.clone().into(),
                    }
                })
                .collect();
            FmPane {
                tabs: ModelRc::new(VecModel::from(tabs)),
                active_tab: pp.active_tab as i32,
            }
        })
        .collect();

    let show_hidden = orchid_widgets::builtin::file_manager::show_hidden(instance_id)
        .unwrap_or(false);
    let single_click_open = orchid_widgets::builtin::file_manager::click_behavior(instance_id)
        .map(|b| b == orchid_widgets::builtin::file_manager::ClickBehavior::SingleToOpen)
        .unwrap_or(false);
    let activity_indicator = if p.transfer_active {
        let percent = (p.transfer_progress * 100.0).round() as u32;
        let key = if p.transfer_is_copy {
            "fm-copying"
        } else {
            "fm-moving"
        };
        locale.tr_args(
            key,
            &orchid_i18n::FluentArgs::new()
                .with(
                    "name",
                    p.transfer_current.as_deref().unwrap_or(""),
                )
                .with("percent", percent.to_string()),
        )
    } else if let Some(err) = p.transfer_error.as_ref() {
        locale.tr_args(
            "fm-transfer-failed",
            &orchid_i18n::FluentArgs::new().with("reason", fm_localized_error(locale, err)),
        )
    } else if let Some(err) = p.passphrase_error.as_ref() {
        locale.tr_args(
            "fm-passphrase-failed",
            &orchid_i18n::FluentArgs::new().with("reason", fm_localized_error(locale, err)),
        )
    } else if let Some(name) = p.ingest_error.as_ref() {
        locale.tr_args(
            "fm-ingest-failed",
            &orchid_i18n::FluentArgs::new().with("name", name.as_str()),
        )
    } else if p.ingest_in_flight > 0 {
        if let Some(name) = p.activity_indicator.as_ref().filter(|s| !s.is_empty()) {
            locale.tr_args(
                "fm-ingesting",
                &orchid_i18n::FluentArgs::new()
                    .with("name", name.as_str())
                    .with("count", p.ingest_in_flight.to_string()),
            )
        } else {
            locale.tr_args(
                "fm-ingesting-count",
                &orchid_i18n::FluentArgs::new().with("count", p.ingest_in_flight.to_string()),
            )
        }
    } else if let Some(key) = p.activity_notice_key.as_ref() {
        let args = match p.activity_notice_name.as_ref() {
            Some(name) => orchid_i18n::FluentArgs::new().with("name", name.as_str()),
            None => orchid_i18n::FluentArgs::new(),
        };
        locale.tr_args(key, &args)
    } else {
        p.activity_indicator
            .as_ref()
            .map(|name| {
                locale.tr_args(
                    "fm-ingested",
                    &orchid_i18n::FluentArgs::new().with("name", name.as_str()),
                )
            })
            .unwrap_or_default()
    };

    let clipboard_indicator = if p.clipboard_count > 0 {
        let key = if p.clipboard_is_cut {
            "fm-clipboard-cut"
        } else {
            "fm-clipboard-copy"
        };
        locale.tr_args(
            key,
            &orchid_i18n::FluentArgs::new().with("count", p.clipboard_count.to_string()),
        )
    } else {
        String::new()
    };

    FileManagerModel {
        panes: ModelRc::new(VecModel::from(panes)),
        active_pane: i32::from(p.active_pane),
        dual_pane: p.dual_pane,
        dual_pane_label: if p.dual_pane {
            locale.tr("fm-dual-pane-off").into()
        } else {
            locale.tr("fm-dual-pane-on").into()
        },
        clipboard_indicator: clipboard_indicator.into(),
        activity_indicator: activity_indicator.into(),
        transfer_active: p.transfer_active,
        transfer_progress: p.transfer_progress,
        show_hidden,
        show_hidden_label: if show_hidden {
            locale.tr("fm-show-hidden-on").into()
        } else {
            locale.tr("fm-show-hidden-off").into()
        },
        single_click_open,
        single_click_open_label: if single_click_open {
            locale.tr("fm-click-single-on").into()
        } else {
            locale.tr("fm-click-single-off").into()
        },
        request_autofocus,
        drag_active: overlays.drag_active,
        drag_drop_target: overlays.drag_drop_target.clone().into(),
        drag_target_pane: overlays.drag_target_pane,
        sidebar_items,
        context_menu: overlays.context_menu,
        confirm_dialog: overlays.confirm_dialog,
        rename: overlays.rename,
        tag: overlays.tag,
        passphrase: overlays.passphrase,
        managed_policy: overlays.managed_policy,
    }
}
fn view_mode_to_int(vm: orchid_widgets::FmViewMode) -> i32 {
    use orchid_widgets::FmViewMode::*;
    match vm {
        Icons => 0,
        List => 1,
        Details => 2,
        Gallery => 3,
    }
}
fn fm_action_shortcut(id: &str) -> &'static str {
    match id {
        "fs.select-all" => "Ctrl+A",
        "fs.deselect-all" => "Esc",
        "fs.copy" => "Ctrl+C",
        "fs.paste" => "Ctrl+V",
        "fs.rename" => "F2",
        "fs.delete" => "Del",
        "fs.new-folder" => "Ctrl+Shift+N",
        _ => "",
    }
}

fn context_menu_item_label(
    a: &orchid_widgets::builtin::file_manager::ContextMenuItem,
    locale: &LocaleManager,
) -> SharedString {
    if a.id.starts_with("fs.tag:") || a.id.starts_with("fs.tag-remove:") || a.id.starts_with("fs.color-label:") {
        if a.id.starts_with("fs.tag-remove:") {
            return format!("− {}", a.label_key).into();
        }
        if a.id.starts_with("fs.color-label:") {
            return locale.tr(&a.label_key).into();
        }
        return a.label_key.clone().into();
    }
    locale.tr(&a.label_key).into()
}

fn context_menu_item_enabled(
    a: &orchid_widgets::builtin::file_manager::ContextMenuItem,
) -> bool {
    if a.id == "fs.tag-remove" || a.id == "fs.color-label" {
        return false;
    }
    a.enabled
}

fn build_context_subitems(
    actions: &[orchid_widgets::builtin::file_manager::ContextMenuItem],
    locale: &LocaleManager,
) -> Vec<FmContextSubitem> {
    let mut out = Vec::new();
    for a in actions {
        out.push(FmContextSubitem {
            id: a.id.clone().into(),
            label: context_menu_item_label(a, locale),
            icon: a.icon.into(),
            swatch_color: a.swatch_color.unwrap_or("").into(),
            enabled: a.enabled,
            is_separator: false,
        });
        if a.separator_after {
            out.push(FmContextSubitem {
                id: SharedString::new(),
                label: SharedString::new(),
                icon: SharedString::new(),
                swatch_color: SharedString::new(),
                enabled: false,
                is_separator: true,
            });
        }
    }
    out
}

pub(crate) fn build_context_menu_actions(
    actions: &[orchid_widgets::builtin::file_manager::ContextMenuItem],
    locale: &LocaleManager,
) -> Vec<FmContextAction> {
    let mut out = Vec::new();
    for a in actions {
        let children = build_context_subitems(&a.submenu, locale);
        out.push(FmContextAction {
            id: a.id.clone().into(),
            label: context_menu_item_label(a, locale),
            shortcut: fm_action_shortcut(&a.id).into(),
            icon: a.icon.into(),
            enabled: context_menu_item_enabled(a),
            is_separator: false,
            has_submenu: !a.submenu.is_empty(),
            children: ModelRc::new(VecModel::from(children)),
        });
        if a.separator_after {
            out.push(FmContextAction {
                id: SharedString::new(),
                label: SharedString::new(),
                shortcut: SharedString::new(),
                icon: SharedString::new(),
                enabled: false,
                is_separator: true,
                has_submenu: false,
                children: ModelRc::new(VecModel::default()),
            });
        }
    }
    out
}

pub(crate) fn build_context_menu(
    actions: &[orchid_widgets::builtin::file_manager::ContextMenuItem],
    target_paths: &[String],
    x: f32,
    y: f32,
    locale: &LocaleManager,
) -> FmContextMenu {
    let actions_vec = build_context_menu_actions(actions, locale);
    let paths_vec: Vec<SharedString> = target_paths.iter().cloned().map(Into::into).collect();
    FmContextMenu {
        visible: true,
        x,
        y,
        actions: ModelRc::new(VecModel::from(actions_vec)),
        target_paths: ModelRc::new(VecModel::from(paths_vec)),
    }
}
