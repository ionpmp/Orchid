//! Context-menu descriptor builder.
//!
//! Produces a flat (with submenus) list of actionable items whose
//! `enabled` flag depends on the current selection. The items carry
//! command ids that the UI layer maps to [`orchid_core::Action`]
//! dispatches.

use orchid_fs::FsEntry;
use orchid_i18n::LocaleManager;
use orchid_storage::LocaleConfig;

/// One entry in the file-manager context menu.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct ContextMenuItem {
    pub id: String,
    pub label_key: String,
    pub icon: &'static str,
    /// Color swatch id for submenu rows (`red`, `orange`, …, `none`). Empty uses `icon`.
    pub swatch_color: Option<&'static str>,
    pub enabled: bool,
    pub separator_after: bool,
    pub submenu: Vec<ContextMenuItem>,
}

/// Builder input flags — lets tests / UI override capability probes
/// without reaching into `orchid-fs` engines.
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct ContextMenuInputs {
    pub clipboard_has_contents: bool,
    /// Undo stack is non-empty.
    pub can_undo: bool,
    /// Redo stack is non-empty.
    pub can_redo: bool,
    pub all_encrypted: bool,
    pub any_encrypted: bool,
    pub all_managed: bool,
    pub all_starred: bool,
    pub any_starred: bool,
    /// Recent tag strings for quick-apply submenu entries.
    pub known_tags: Vec<String>,
    /// Union of tags on the current selection (for remove-tag actions).
    pub tags_on_selection: Vec<String>,
    /// Visible entries in the active listing (after quick filter).
    pub entry_count: usize,
    /// Number of selected paths.
    pub selection_count: usize,
    /// Selection is under a registered managed folder (policy dialog available).
    pub managed_policy_available: bool,
    /// Current folder can accept create/paste (not a virtual listing).
    pub can_create: bool,
    /// Dual-pane mode (compare / sync against the other pane).
    pub dual_pane: bool,
    /// Current folder is inside an archive (`archive:`).
    pub in_archive: bool,
    /// Current folder is the Recycle Bin virtual listing.
    pub in_recycle: bool,
    /// Selection includes at least one audio file the library player can play.
    pub selection_has_audio: bool,
    /// Selection includes at least one video file the video player can play.
    pub selection_has_video: bool,
}

/// Read-only header shown at the top of a file/folder context menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextMenuInfo {
    /// File name, or a localized “N items” summary for a multi-selection.
    pub name: String,
    /// Localized `Type: …` line. Empty when omitted.
    pub type_line: String,
    /// Localized `Size: …` line. Empty when omitted.
    pub size_line: String,
    /// Localized `Modified: …` line. Empty when omitted.
    pub modified_line: String,
    /// Localized `MIME: …` line. Empty when omitted (folders / multi-select).
    pub mime_line: String,
}

/// Format the properties header for the current selection, if any.
#[must_use]
pub fn info_for_selection(
    selection: &[FsEntry],
    locale: &LocaleManager,
    fmt_locale: &LocaleConfig,
) -> Option<ContextMenuInfo> {
    if selection.is_empty() {
        return None;
    }
    let name = if selection.len() == 1 {
        selection[0].name.clone()
    } else {
        locale.tr_args(
            "fm-properties-items",
            &orchid_i18n::FluentArgs::new().with("count", selection.len().to_string()),
        )
    };
    let kind = selection_kind_label(selection, locale);
    let type_line = locale.tr_args(
        "fm-properties-type",
        &orchid_i18n::FluentArgs::new().with("kind", kind),
    );
    let total_size: u64 = selection.iter().map(|e| e.metadata.size).sum();
    let size_line = locale.tr_args(
        "fm-properties-size",
        &orchid_i18n::FluentArgs::new().with("size", locale.format_byte_size(total_size)),
    );
    let (modified_line, mime_line) = if selection.len() == 1 {
        let meta = &selection[0].metadata;
        let modified = meta
            .modified
            .map(|t| fmt_locale.format_datetime(t))
            .unwrap_or_else(|| "—".into());
        let modified_line = locale.tr_args(
            "fm-properties-modified",
            &orchid_i18n::FluentArgs::new().with("modified", modified),
        );
        let mime_line =
            if let Some(orig) = orchid_fs::recycle_original_path(selection[0].path.as_str()) {
                locale.tr_args(
                    "fm-properties-original",
                    &orchid_i18n::FluentArgs::new().with("path", orig),
                )
            } else if meta.kind == orchid_fs::FsEntryKind::Directory {
                String::new()
            } else {
                let mime = meta.mime.clone().unwrap_or_else(|| "—".into());
                locale.tr_args(
                    "fm-properties-mime",
                    &orchid_i18n::FluentArgs::new().with("mime", mime),
                )
            };
        (modified_line, mime_line)
    } else {
        (String::new(), String::new())
    };
    Some(ContextMenuInfo {
        name,
        type_line,
        size_line,
        modified_line,
        mime_line,
    })
}

fn selection_kind_label(selection: &[FsEntry], locale: &LocaleManager) -> String {
    let all_dirs = selection
        .iter()
        .all(|e| e.metadata.kind == orchid_fs::FsEntryKind::Directory);
    let all_files = selection
        .iter()
        .all(|e| e.metadata.kind == orchid_fs::FsEntryKind::File);
    if all_dirs {
        locale.tr("fm-properties-kind-folder")
    } else if all_files {
        locale.tr("fm-properties-kind-file")
    } else {
        locale.tr("fm-properties-kind-mixed")
    }
}

/// Build the menu from the current selection and the extra flags.
#[must_use]
pub fn build_for_selection(
    selection: &[FsEntry],
    inputs: ContextMenuInputs,
) -> Vec<ContextMenuItem> {
    let count = selection.len();
    let has_selection = count > 0;
    let single_file = count == 1 && selection[0].metadata.kind == orchid_fs::FsEntryKind::File;
    let single = count == 1;
    let mut items = Vec::new();

    if inputs.in_recycle {
        return recycle_menu(has_selection, &inputs);
    }

    if !has_selection {
        return background_menu(&inputs);
    }

    items.push(ContextMenuItem {
        id: if count > 1 {
            "fs.open-all".into()
        } else {
            "fs.open".into()
        },
        label_key: if count > 1 {
            "fm-action-open-all".into()
        } else {
            "fm-action-open".into()
        },
        icon: "action-open",
        swatch_color: None,
        enabled: has_selection,
        separator_after: false,
        submenu: vec![
            item(
                "fs.open-external",
                "fm-action-open-default",
                "action-open",
                single_file,
            ),
            item(
                "fs.open-with",
                "fm-action-open-with",
                "action-open-with",
                single_file,
            ),
            item(
                "viewer.open",
                "fm-action-open-in-viewer",
                "widget-viewer",
                single_file,
            ),
            item(
                "viewer.edit",
                "fm-action-edit-in-viewer",
                "widget-viewer",
                single_file,
            ),
            item(
                "audio.play",
                "fm-action-play-in-audio-player",
                "audio-player",
                inputs.selection_has_audio,
            ),
            item(
                "audio.enqueue",
                "fm-action-enqueue-in-audio-player",
                "audio-player",
                inputs.selection_has_audio,
            ),
            item(
                "video.play",
                "fm-action-play-in-video-player",
                "video-player",
                inputs.selection_has_video,
            ),
            item(
                "video.enqueue",
                "fm-action-enqueue-in-video-player",
                "video-player",
                inputs.selection_has_video,
            ),
            item(
                "fs.file-assoc",
                "fm-action-file-assoc",
                "action-open-with",
                single_file,
            ),
            item(
                "fs.open-tab",
                "fm-action-open-tab",
                "action-new-tab",
                has_selection,
            ),
            item(
                "fs.open-other-pane",
                "fm-action-open-other-pane",
                "action-open",
                has_selection,
            ),
        ],
    });
    items.last_mut().unwrap().separator_after = true;

    items.push(ContextMenuItem {
        id: "fs.copy".into(),
        label_key: "fm-action-copy".into(),
        icon: "action-copy",
        swatch_color: None,
        enabled: has_selection,
        separator_after: false,
        submenu: vec![
            item(
                "fs.copy-to-other",
                "fm-action-copy-other",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.copy-verify",
                "fm-action-copy-verify",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.copy-newer",
                "fm-action-copy-newer",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.copy-structure",
                "fm-action-copy-structure",
                "action-copy",
                has_selection,
            ),
        ],
    });
    items.push(item("fs.cut", "fm-action-cut", "action-cut", has_selection));
    items.push(item(
        "fs.move-to-other",
        "fm-action-move-other",
        "action-cut",
        has_selection,
    ));
    items.push(sep(item(
        "fs.paste",
        "fm-action-paste",
        "action-paste",
        inputs.clipboard_has_contents,
    )));
    push_undo_redo(&mut items, &inputs);

    items.push(item(
        if single {
            "fs.rename"
        } else {
            "fs.batch-rename"
        },
        if single {
            "fm-action-rename"
        } else {
            "fm-action-batch-rename"
        },
        "action-rename",
        has_selection,
    ));
    items.push(ContextMenuItem {
        id: "fs.delete".into(),
        label_key: "fm-action-delete".into(),
        icon: "action-delete",
        swatch_color: None,
        enabled: has_selection,
        separator_after: true,
        submenu: vec![
            item(
                "fs.delete-recycle",
                "fm-action-delete-recycle",
                "action-delete",
                has_selection,
            ),
            item(
                "fs.delete-permanent",
                "fm-action-delete-permanent",
                "action-delete",
                has_selection,
            ),
        ],
    });

    items.push(ContextMenuItem {
        id: "fs.link".into(),
        label_key: "fm-action-link".into(),
        icon: "action-new-file",
        swatch_color: None,
        enabled: single,
        separator_after: true,
        submenu: vec![
            item(
                "fs.link-symlink",
                "fm-action-symlink",
                "action-new-file",
                single,
            ),
            item(
                "fs.link-hard",
                "fm-action-hardlink",
                "action-new-file",
                single,
            ),
            item(
                "fs.link-junction",
                "fm-action-junction",
                "action-new-folder",
                single,
            ),
        ],
    });

    items.push(tools_menu(
        has_selection,
        single_file,
        count >= 2,
        inputs.dual_pane,
    ));

    let any_archive = selection
        .iter()
        .any(|e| orchid_fs::looks_like_archive_name(&e.name));
    items.push(archive_menu(
        has_selection,
        any_archive,
        inputs.in_archive,
        inputs.dual_pane,
    ));

    items.push(ContextMenuItem {
        id: "fs.tag-add".into(),
        label_key: "fm-action-add-tag".into(),
        icon: "action-tag",
        swatch_color: None,
        enabled: has_selection,
        separator_after: false,
        submenu: inputs
            .known_tags
            .iter()
            .map(|t| ContextMenuItem {
                id: format!("fs.tag:{t}"),
                label_key: t.clone(),
                icon: "action-tag",
                swatch_color: None,
                enabled: has_selection,
                separator_after: false,
                submenu: Vec::new(),
            })
            .collect(),
    });
    if !inputs.tags_on_selection.is_empty() {
        items.push(ContextMenuItem {
            id: "fs.tag-remove".into(),
            label_key: "fm-action-remove-tag".into(),
            icon: "action-tag",
            swatch_color: None,
            enabled: has_selection,
            separator_after: false,
            submenu: inputs
                .tags_on_selection
                .iter()
                .map(|t| ContextMenuItem {
                    id: format!("fs.tag-remove:{t}"),
                    label_key: t.clone(),
                    icon: "action-tag",
                    swatch_color: None,
                    enabled: has_selection,
                    separator_after: false,
                    submenu: Vec::new(),
                })
                .collect(),
        });
    }
    items.push(ContextMenuItem {
        id: "fs.color-label".into(),
        label_key: "fm-action-color-label".into(),
        icon: "action-color",
        swatch_color: None,
        enabled: has_selection,
        separator_after: false,
        submenu: vec![
            color_item("fs.color-label:red", "fm-color-red", "red", has_selection),
            color_item(
                "fs.color-label:orange",
                "fm-color-orange",
                "orange",
                has_selection,
            ),
            color_item(
                "fs.color-label:yellow",
                "fm-color-yellow",
                "yellow",
                has_selection,
            ),
            color_item(
                "fs.color-label:green",
                "fm-color-green",
                "green",
                has_selection,
            ),
            color_item(
                "fs.color-label:blue",
                "fm-color-blue",
                "blue",
                has_selection,
            ),
            color_item(
                "fs.color-label:purple",
                "fm-color-purple",
                "purple",
                has_selection,
            ),
            color_item(
                "fs.color-label:gray",
                "fm-color-gray",
                "gray",
                has_selection,
            ),
            color_item(
                "fs.color-label:none",
                "fm-color-none",
                "none",
                has_selection,
            ),
        ],
    });
    items.push(sep(if inputs.any_starred && inputs.all_starred {
        item(
            "fs.unstar",
            "fm-action-unstar",
            "action-star",
            has_selection,
        )
    } else {
        item("fs.star", "fm-action-star", "action-star", has_selection)
    }));

    if inputs.any_encrypted {
        items.push(item(
            "fs.reveal",
            "fm-action-reveal",
            "action-reveal",
            inputs.all_encrypted,
        ));
        items.push(item(
            "fs.decrypt",
            "fm-action-decrypt",
            "action-decrypt",
            inputs.all_encrypted,
        ));
    } else {
        items.push(item(
            "fs.encrypt",
            "fm-action-encrypt",
            "action-encrypt",
            has_selection,
        ));
    }
    items.last_mut().unwrap().separator_after = true;

    if inputs.all_managed {
        items.push(item(
            "fs.remove-from-managed",
            "fm-action-remove-from-managed",
            "action-managed",
            has_selection,
        ));
    } else {
        items.push(item(
            "fs.add-to-managed",
            "fm-action-add-to-managed",
            "action-managed",
            has_selection,
        ));
    }
    if inputs.managed_policy_available {
        items.push(item(
            "fs.managed-policy",
            "fm-action-managed-policy",
            "action-managed",
            has_selection,
        ));
    }
    items.last_mut().unwrap().separator_after = true;

    items.push(item(
        "fs.select-all",
        "fm-action-select-all",
        "action-select-all",
        inputs.entry_count > 0,
    ));
    items.push(item(
        "fs.deselect-all",
        "fm-action-deselect-all",
        "action-deselect",
        inputs.selection_count > 0,
    ));
    items.push(select_more_menu(&inputs));
    if inputs.can_create {
        items.push(item(
            "fs.branch-view",
            "fm-action-branch-view",
            "action-select-all",
            true,
        ));
    }

    items
}

/// Recycle Bin: restore / purge selected items, or empty the bin.
fn recycle_menu(has_selection: bool, inputs: &ContextMenuInputs) -> Vec<ContextMenuItem> {
    let mut items = Vec::new();
    push_undo_redo(&mut items, inputs);
    if has_selection {
        items.push(item(
            "fs.recycle-restore",
            "fm-action-recycle-restore",
            "action-open",
            true,
        ));
        items.push(sep(item(
            "fs.recycle-purge",
            "fm-action-recycle-purge",
            "action-delete",
            true,
        )));
    }
    items.push(item(
        "fs.recycle-empty",
        "fm-action-recycle-empty",
        "action-delete",
        inputs.entry_count > 0,
    ));
    if inputs.entry_count > 0 {
        items.last_mut().unwrap().separator_after = true;
        items.push(item(
            "fs.select-all",
            "fm-action-select-all",
            "action-select-all",
            true,
        ));
        items.push(select_more_menu(inputs));
    }
    items
}

/// Right-click on empty listing space: only actions that apply without a target.
fn background_menu(inputs: &ContextMenuInputs) -> Vec<ContextMenuItem> {
    let mut items = Vec::new();
    if inputs.can_create {
        items.push(item(
            "fs.new-folder",
            "fm-action-new-folder",
            "action-new-folder",
            true,
        ));
        items.push(item(
            "fs.new-file",
            "fm-action-new-file",
            "action-new-file",
            true,
        ));
    }
    if inputs.clipboard_has_contents {
        if !items.is_empty() {
            items.last_mut().unwrap().separator_after = true;
        }
        items.push(item("fs.paste", "fm-action-paste", "action-paste", true));
    }
    if !items.is_empty() {
        items.last_mut().unwrap().separator_after = true;
    }
    push_undo_redo(&mut items, inputs);
    if inputs.entry_count > 0 {
        if !items.is_empty() {
            items.last_mut().unwrap().separator_after = true;
        }
        items.push(item(
            "fs.select-all",
            "fm-action-select-all",
            "action-select-all",
            true,
        ));
        items.push(select_more_menu(inputs));
    }
    if inputs.can_create {
        if !items.is_empty() {
            items.last_mut().unwrap().separator_after = true;
        }
        items.push(item("fs.find", "fm-action-find", "action-select-all", true));
        items.push(item(
            "fs.branch-view",
            "fm-action-branch-view",
            "action-select-all",
            true,
        ));
    }
    if inputs.dual_pane {
        if !items.is_empty() {
            items.last_mut().unwrap().separator_after = true;
        }
        items.push(item(
            "fs.compare-dirs",
            "fm-action-compare-dirs",
            "action-copy",
            true,
        ));
        items.push(item(
            "fs.sync-both",
            "fm-action-sync-both",
            "action-copy",
            true,
        ));
    }
    if inputs.in_archive {
        if !items.is_empty() {
            items.last_mut().unwrap().separator_after = true;
        }
        items.push(archive_menu(false, false, true, inputs.dual_pane));
    }
    items
}

fn select_more_menu(inputs: &ContextMenuInputs) -> ContextMenuItem {
    let n = inputs.entry_count > 0;
    ContextMenuItem {
        id: "fs.select-more".into(),
        label_key: "fm-action-select-more".into(),
        icon: "action-select-all",
        swatch_color: None,
        enabled: n,
        separator_after: false,
        submenu: vec![
            item(
                "fs.invert-selection",
                "fm-action-invert-selection",
                "action-select-all",
                n,
            ),
            item(
                "fs.select-files",
                "fm-action-select-files",
                "action-select-all",
                n,
            ),
            item(
                "fs.select-folders",
                "fm-action-select-folders",
                "action-select-all",
                n,
            ),
            item(
                "fs.select-mask-add",
                "fm-action-select-mask",
                "action-select-all",
                n,
            ),
            item(
                "fs.select-mask-sub",
                "fm-action-deselect-mask",
                "action-deselect",
                inputs.selection_count > 0,
            ),
            item(
                "fs.select-filter",
                "fm-action-select-filter",
                "action-select-all",
                n,
            ),
            item(
                "fs.select-hidden",
                "fm-action-select-hidden",
                "action-select-all",
                n,
            ),
            item(
                "fs.select-readonly",
                "fm-action-select-readonly",
                "action-select-all",
                n,
            ),
        ],
    }
}

fn push_undo_redo(items: &mut Vec<ContextMenuItem>, inputs: &ContextMenuInputs) {
    items.push(item(
        "fs.undo",
        "fm-action-undo",
        "action-undo",
        inputs.can_undo,
    ));
    items.push(sep(item(
        "fs.redo",
        "fm-action-redo",
        "action-redo",
        inputs.can_redo,
    )));
}

fn item(id: &str, label_key: &str, icon: &'static str, enabled: bool) -> ContextMenuItem {
    ContextMenuItem {
        id: id.into(),
        label_key: label_key.into(),
        icon,
        swatch_color: None,
        enabled,
        separator_after: false,
        submenu: Vec::new(),
    }
}

fn color_item(id: &str, label_key: &str, swatch: &'static str, enabled: bool) -> ContextMenuItem {
    ContextMenuItem {
        id: id.into(),
        label_key: label_key.into(),
        icon: "action-color",
        swatch_color: Some(swatch),
        enabled,
        separator_after: false,
        submenu: Vec::new(),
    }
}

fn sep(mut it: ContextMenuItem) -> ContextMenuItem {
    it.separator_after = true;
    it
}

fn tools_menu(
    has_selection: bool,
    single_file: bool,
    two_files: bool,
    dual: bool,
) -> ContextMenuItem {
    let mut submenu = vec![
        item("fs.find", "fm-action-find", "action-select-all", true),
        item(
            "fs.properties",
            "fm-action-properties",
            "action-copy",
            has_selection || single_file,
        ),
        item("fs.exif", "fm-action-exif", "action-copy", single_file),
        ContextMenuItem {
            id: "fs.image-edit".into(),
            label_key: "fm-action-image-edit".into(),
            icon: "action-copy",
            swatch_color: None,
            enabled: has_selection,
            separator_after: false,
            submenu: vec![
                item(
                    "fs.image-resize",
                    "fm-action-image-resize",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-canvas",
                    "fm-action-image-canvas",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-auto-straighten",
                    "fm-action-image-auto-straighten",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-adjust",
                    "fm-action-image-adjust",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-auto-levels",
                    "fm-action-image-auto-levels",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-auto-contrast",
                    "fm-action-image-auto-contrast",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-auto-color",
                    "fm-action-image-auto-color",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-gray",
                    "fm-action-image-gray",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-sepia",
                    "fm-action-image-sepia",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-invert",
                    "fm-action-image-invert",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-filter",
                    "fm-action-image-filter",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-sharpen",
                    "fm-action-image-sharpen",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-blur",
                    "fm-action-image-blur",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-despeckle",
                    "fm-action-image-despeckle",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-cartoon",
                    "fm-action-image-cartoon",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-sketch",
                    "fm-action-image-sketch",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-vignette",
                    "fm-action-image-vignette",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-redeye",
                    "fm-action-image-redeye",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-filter-save-look",
                    "fm-action-image-filter-save-look",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-annotate",
                    "fm-action-image-annotate",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-watermark",
                    "fm-action-image-watermark",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-wm-image",
                    "fm-action-image-wm-image",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-stamp",
                    "fm-action-image-stamp",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-convert",
                    "fm-action-image-convert",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-rotate",
                    "fm-action-image-rotate",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-thumbs",
                    "fm-action-image-thumbs",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-rename-tpl",
                    "fm-action-image-rename-tpl",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-batch",
                    "fm-action-image-batch",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-batch-preview",
                    "fm-action-image-batch-preview",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-batch-save",
                    "fm-action-image-batch-save",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-batch-cancel",
                    "fm-action-image-batch-cancel",
                    "action-copy",
                    true,
                ),
                item(
                    "fs.image-compare",
                    "fm-action-image-compare",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-pick",
                    "fm-action-image-pick",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-diff",
                    "fm-action-image-diff",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-composite",
                    "fm-action-image-composite",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-pano",
                    "fm-action-image-pano",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-hdr",
                    "fm-action-image-hdr",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-print",
                    "fm-action-image-print",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-print-preview",
                    "fm-action-image-print-preview",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-print-sheet",
                    "fm-action-image-print-sheet",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-print-nup",
                    "fm-action-image-print-nup",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-print-batch",
                    "fm-action-image-print-batch",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-export",
                    "fm-action-image-export",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-email",
                    "fm-action-image-email",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-share",
                    "fm-action-image-share",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-copy",
                    "fm-action-image-copy",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-paste",
                    "fm-action-image-paste",
                    "action-copy",
                    true,
                ),
                item(
                    "fs.image-wallpaper",
                    "fm-action-image-wallpaper",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-screenshot",
                    "fm-action-image-screenshot",
                    "action-copy",
                    true,
                ),
                item(
                    "fs.image-ico",
                    "fm-action-image-ico",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.image-favicon",
                    "fm-action-image-favicon",
                    "action-copy",
                    has_selection,
                ),
            ],
        },
        ContextMenuItem {
            id: "fs.meta".into(),
            label_key: "fm-action-meta".into(),
            icon: "action-copy",
            swatch_color: None,
            enabled: has_selection,
            separator_after: false,
            submenu: vec![
                item(
                    "fs.meta-edit",
                    "fm-action-meta-edit",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.meta-gps",
                    "fm-action-meta-gps",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.meta-date",
                    "fm-action-meta-date",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.meta-date-shift",
                    "fm-action-meta-date-shift",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.meta-strip",
                    "fm-action-meta-strip",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.meta-strip-gps",
                    "fm-action-meta-strip-gps",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.meta-copy",
                    "fm-action-meta-copy",
                    "action-copy",
                    two_files,
                ),
                item(
                    "fs.meta-export-csv",
                    "fm-action-meta-export-csv",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.meta-export-xml",
                    "fm-action-meta-export-xml",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.meta-import-csv",
                    "fm-action-meta-import-csv",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.meta-template-save",
                    "fm-action-meta-template-save",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.meta-template-apply",
                    "fm-action-meta-template-apply",
                    "action-copy",
                    has_selection,
                ),
            ],
        },
        item("fs.id3", "fm-action-id3", "action-copy", single_file),
        item(
            "fs.office-meta",
            "fm-action-office-meta",
            "action-copy",
            single_file,
        ),
        item(
            "fs.signature",
            "fm-action-signature",
            "action-copy",
            single_file,
        ),
        item(
            "fs.find-duplicates",
            "fm-action-find-duplicates",
            "action-copy",
            true,
        ),
        item("fs.find-large", "fm-action-find-large", "action-copy", true),
        item(
            "fs.compare-dirs",
            "fm-action-compare-dirs",
            "action-copy",
            dual,
        ),
        item(
            "fs.compare-dirs-bytes",
            "fm-action-compare-dirs-bytes",
            "action-copy",
            dual,
        ),
        item(
            "fs.compare-files",
            "fm-action-compare-files",
            "action-copy",
            (single_file && dual) || two_files,
        ),
        item("fs.sync-to-other", "fm-action-sync-to", "action-copy", dual),
        item(
            "fs.sync-from-other",
            "fm-action-sync-from",
            "action-copy",
            dual,
        ),
        item("fs.sync-both", "fm-action-sync-both", "action-copy", dual),
        item("fs.cloud-sync", "fm-action-cloud-sync", "action-copy", dual),
        item(
            "fs.network-bookmark",
            "fm-action-network-bookmark",
            "action-copy",
            true,
        ),
        item(
            "fs.network-connect",
            "fm-action-network-connect",
            "action-copy",
            true,
        ),
        item("fs.merge-to-other", "fm-action-merge", "action-copy", dual),
        item(
            "fs.split",
            "fm-action-split",
            "action-new-file",
            single_file,
        ),
        item(
            "fs.join",
            "fm-action-join",
            "action-new-file",
            has_selection,
        ),
    ];
    submenu.push(ContextMenuItem {
        id: "fs.hash".into(),
        label_key: "fm-action-hash".into(),
        icon: "action-copy",
        swatch_color: None,
        enabled: has_selection,
        separator_after: false,
        submenu: vec![
            item(
                "fs.hash-md5",
                "fm-action-hash-md5",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.hash-sha1",
                "fm-action-hash-sha1",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.hash-sha256",
                "fm-action-hash-sha256",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.hash-blake3",
                "fm-action-hash-blake3",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.hash-crc32",
                "fm-action-hash-crc32",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.hash-verify",
                "fm-action-hash-verify",
                "action-copy",
                has_selection,
            ),
        ],
    });
    submenu.push(ContextMenuItem {
        id: "fs.encode".into(),
        label_key: "fm-action-encode".into(),
        icon: "action-copy",
        swatch_color: None,
        enabled: single_file,
        separator_after: false,
        submenu: vec![
            item(
                "fs.encode-base64",
                "fm-action-encode-base64",
                "action-copy",
                single_file,
            ),
            item(
                "fs.decode-base64",
                "fm-action-decode-base64",
                "action-copy",
                single_file,
            ),
            item(
                "fs.encode-uue",
                "fm-action-encode-uue",
                "action-copy",
                single_file,
            ),
            item(
                "fs.decode-uue",
                "fm-action-decode-uue",
                "action-copy",
                single_file,
            ),
        ],
    });
    submenu.push(ContextMenuItem {
        id: "fs.attr".into(),
        label_key: "fm-action-attributes".into(),
        icon: "action-copy",
        swatch_color: None,
        enabled: has_selection,
        separator_after: false,
        submenu: vec![
            item(
                "fs.attr-readonly-on",
                "fm-action-attr-readonly-on",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.attr-readonly-off",
                "fm-action-attr-readonly-off",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.attr-hidden-on",
                "fm-action-attr-hidden-on",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.attr-hidden-off",
                "fm-action-attr-hidden-off",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.attr-system-on",
                "fm-action-attr-system-on",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.attr-system-off",
                "fm-action-attr-system-off",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.attr-archive-on",
                "fm-action-attr-archive-on",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.attr-archive-off",
                "fm-action-attr-archive-off",
                "action-copy",
                has_selection,
            ),
        ],
    });
    submenu.push(ContextMenuItem {
        id: "fs.touch".into(),
        label_key: "fm-action-touch".into(),
        icon: "action-copy",
        swatch_color: None,
        enabled: has_selection,
        separator_after: false,
        submenu: vec![
            item(
                "fs.touch-now",
                "fm-action-touch-now",
                "action-copy",
                has_selection,
            ),
            item(
                "fs.touch-set",
                "fm-action-touch-set",
                "action-copy",
                has_selection,
            ),
        ],
    });
    submenu.push(ContextMenuItem {
        id: "fs.case".into(),
        label_key: "fm-action-case".into(),
        icon: "action-rename",
        swatch_color: None,
        enabled: has_selection,
        separator_after: false,
        submenu: vec![
            item(
                "fs.case-lower",
                "fm-action-case-lower",
                "action-rename",
                has_selection,
            ),
            item(
                "fs.case-upper",
                "fm-action-case-upper",
                "action-rename",
                has_selection,
            ),
            item(
                "fs.case-title",
                "fm-action-case-title",
                "action-rename",
                has_selection,
            ),
        ],
    });
    submenu.push(item(
        "fs.chmod",
        "fm-action-chmod",
        "action-copy",
        has_selection,
    ));
    #[cfg(unix)]
    submenu.push(item(
        "fs.chown",
        "fm-action-chown",
        "action-copy",
        has_selection,
    ));
    #[cfg(windows)]
    {
        submenu.push(item(
            "fs.acl-view",
            "fm-action-acl-view",
            "action-copy",
            has_selection,
        ));
        submenu.push(item(
            "fs.acl-grant",
            "fm-action-acl-grant",
            "action-copy",
            has_selection,
        ));
        submenu.push(item(
            "fs.acl-reset",
            "fm-action-acl-reset",
            "action-copy",
            has_selection,
        ));
        submenu.push(ContextMenuItem {
            id: "fs.share".into(),
            label_key: "fm-action-share".into(),
            icon: "action-copy",
            swatch_color: None,
            enabled: has_selection,
            separator_after: false,
            submenu: vec![
                item(
                    "fs.share-view",
                    "fm-action-share-view",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.share-add",
                    "fm-action-share-add",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.share-remove",
                    "fm-action-share-remove",
                    "action-delete",
                    has_selection,
                ),
                item(
                    "fs.share-os",
                    "fm-action-share-os",
                    "action-copy",
                    has_selection,
                ),
            ],
        });
        submenu.push(ContextMenuItem {
            id: "fs.versions".into(),
            label_key: "fm-action-versions".into(),
            icon: "action-copy",
            swatch_color: None,
            enabled: has_selection,
            separator_after: false,
            submenu: vec![
                item(
                    "fs.versions-view",
                    "fm-action-versions-view",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.versions-restore",
                    "fm-action-versions-restore",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.versions-copy",
                    "fm-action-versions-copy",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.versions-os",
                    "fm-action-versions-os",
                    "action-copy",
                    has_selection,
                ),
            ],
        });
        submenu.push(ContextMenuItem {
            id: "fs.bitlocker".into(),
            label_key: "fm-action-bitlocker".into(),
            icon: "action-copy",
            swatch_color: None,
            enabled: has_selection,
            separator_after: false,
            submenu: vec![
                item(
                    "fs.bitlocker-view",
                    "fm-action-bitlocker-view",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.bitlocker-lock",
                    "fm-action-bitlocker-lock",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.bitlocker-unlock",
                    "fm-action-bitlocker-unlock",
                    "action-copy",
                    has_selection,
                ),
                item(
                    "fs.bitlocker-os",
                    "fm-action-bitlocker-os",
                    "action-copy",
                    true,
                ),
            ],
        });
    }
    ContextMenuItem {
        id: "fs.tools".into(),
        label_key: "fm-action-tools".into(),
        icon: "action-copy",
        swatch_color: None,
        enabled: true,
        separator_after: true,
        submenu,
    }
}

fn archive_menu(
    has_selection: bool,
    any_archive: bool,
    in_archive: bool,
    dual: bool,
) -> ContextMenuItem {
    let pack = has_selection && !in_archive;
    let browse = any_archive;
    let extract_all = any_archive || in_archive;
    let extract_sel = in_archive && has_selection;
    let add = if in_archive { dual } else { has_selection };
    let delete = in_archive && has_selection;
    let test = any_archive || in_archive;
    ContextMenuItem {
        id: "fs.archive".into(),
        label_key: "fm-action-archive".into(),
        icon: "action-copy",
        swatch_color: None,
        enabled: true,
        separator_after: true,
        submenu: vec![
            item(
                "fs.archive-open",
                "fm-action-archive-open",
                "action-open",
                browse,
            ),
            item(
                "fs.archive-extract",
                "fm-action-archive-extract",
                "action-copy",
                extract_all,
            ),
            item(
                "fs.archive-extract-selected",
                "fm-action-archive-extract-selected",
                "action-copy",
                extract_sel,
            ),
            item(
                "fs.archive-create",
                "fm-action-archive-create",
                "action-new-file",
                pack,
            ),
            item(
                "fs.archive-create-password",
                "fm-action-archive-create-password",
                "action-new-file",
                pack,
            ),
            item(
                "fs.archive-create-sfx",
                "fm-action-archive-create-sfx",
                "action-new-file",
                pack,
            ),
            item(
                "fs.archive-create-volume",
                "fm-action-archive-create-volume",
                "action-new-file",
                pack,
            ),
            item(
                "fs.archive-add",
                "fm-action-archive-add",
                "action-copy",
                add,
            ),
            item(
                "fs.archive-delete",
                "fm-action-archive-delete",
                "action-delete",
                delete,
            ),
            item(
                "fs.archive-test",
                "fm-action-archive-test",
                "action-copy",
                test,
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use orchid_fs::{ExtendedAttributes, FsEntry, FsEntryKind, FsMetadata, FsPath};

    fn entry(name: &str, kind: FsEntryKind, encrypted: bool) -> FsEntry {
        let meta = FsMetadata {
            kind,
            size: 0,
            created: None,
            modified: Some(Utc::now()),
            accessed: None,
            readonly: false,
            hidden: false,
            system: false,
            mime: None,
            extended: ExtendedAttributes {
                is_encrypted: encrypted,
                ..Default::default()
            },
        };
        FsEntry {
            path: FsPath::new(format!("local:/tmp/{name}")).unwrap(),
            name: name.into(),
            metadata: meta,
        }
    }

    fn empty_background() -> ContextMenuInputs {
        ContextMenuInputs {
            can_create: true,
            ..Default::default()
        }
    }

    #[test]
    fn empty_background_offers_create_actions() {
        let menu = build_for_selection(&[], empty_background());
        let ids: Vec<&str> = menu.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"fs.new-folder"));
        assert!(ids.contains(&"fs.new-file"));
        assert!(ids.contains(&"fs.branch-view"));
        assert!(ids.contains(&"fs.undo"));
        assert!(ids.contains(&"fs.redo"));
        assert!(!ids.contains(&"fs.copy"));
        assert!(!ids.contains(&"fs.open"));
        assert!(!ids.contains(&"fs.paste"));
    }

    #[test]
    fn empty_background_omits_create_on_virtual() {
        let menu = build_for_selection(&[], ContextMenuInputs::default());
        assert!(menu.iter().all(|i| i.id != "fs.new-folder"));
        assert!(menu.iter().all(|i| i.id != "fs.new-file"));
    }

    #[test]
    fn recycle_menu_offers_restore_and_empty() {
        let sel = vec![entry("gone.txt", FsEntryKind::File, false)];
        let menu = build_for_selection(
            &sel,
            ContextMenuInputs {
                in_recycle: true,
                entry_count: 1,
                selection_count: 1,
                ..Default::default()
            },
        );
        let ids: Vec<&str> = menu.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"fs.recycle-restore"));
        assert!(ids.contains(&"fs.recycle-purge"));
        assert!(ids.contains(&"fs.recycle-empty"));
        assert!(!ids.contains(&"fs.copy"));
        assert!(!ids.contains(&"fs.delete"));
        let empty = build_for_selection(
            &[],
            ContextMenuInputs {
                in_recycle: true,
                entry_count: 3,
                ..Default::default()
            },
        );
        let empty_ids: Vec<&str> = empty.iter().map(|i| i.id.as_str()).collect();
        assert!(empty_ids.contains(&"fs.recycle-empty"));
        assert!(!empty_ids.contains(&"fs.recycle-restore"));
        assert!(!empty_ids.contains(&"fs.new-folder"));
    }

    #[test]
    fn clipboard_flag_enables_paste() {
        let inputs = ContextMenuInputs {
            can_create: true,
            clipboard_has_contents: true,
            ..Default::default()
        };
        let menu = build_for_selection(&[], inputs);
        let paste = menu.iter().find(|i| i.id == "fs.paste").unwrap();
        assert!(paste.enabled);
        assert!(menu.iter().all(|i| i.id != "fs.copy"));
    }

    #[test]
    fn undo_flag_enables_undo() {
        let menu = build_for_selection(
            &[],
            ContextMenuInputs {
                can_create: true,
                can_undo: true,
                can_redo: true,
                ..Default::default()
            },
        );
        let undo = menu.iter().find(|i| i.id == "fs.undo").unwrap();
        let redo = menu.iter().find(|i| i.id == "fs.redo").unwrap();
        assert!(undo.enabled);
        assert!(redo.enabled);
    }

    #[test]
    fn encrypted_selection_swaps_encrypt_for_decrypt() {
        let sel = vec![entry("a", FsEntryKind::File, true)];
        let menu = build_for_selection(
            &sel,
            ContextMenuInputs {
                any_encrypted: true,
                all_encrypted: true,
                ..Default::default()
            },
        );
        assert!(menu.iter().any(|i| i.id == "fs.decrypt"));
        assert!(!menu.iter().any(|i| i.id == "fs.encrypt"));
    }

    #[test]
    fn tag_add_enabled_without_known_tags() {
        let sel = vec![entry("a", FsEntryKind::File, false)];
        let menu = build_for_selection(&sel, ContextMenuInputs::default());
        let tag_add = menu.iter().find(|i| i.id == "fs.tag-add").unwrap();
        assert!(tag_add.enabled);
    }

    #[test]
    fn selection_menu_omits_properties_action() {
        let sel = vec![entry("a", FsEntryKind::File, false)];
        let menu = build_for_selection(&sel, ContextMenuInputs::default());
        assert!(menu.iter().all(|i| i.id != "fs.properties"));
        let ids: Vec<&str> = menu.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"fs.deselect-all"));
        assert!(ids.contains(&"fs.select-more"));
        assert_eq!(ids.last().copied(), Some("fs.select-more"));
        let more = menu.iter().find(|i| i.id == "fs.select-more").unwrap();
        let sub: Vec<&str> = more.submenu.iter().map(|i| i.id.as_str()).collect();
        assert!(sub.contains(&"fs.invert-selection"));
        assert!(sub.contains(&"fs.select-files"));
        assert!(sub.contains(&"fs.select-mask-add"));
        assert!(sub.contains(&"fs.select-filter"));
        let tools = menu.iter().find(|i| i.id == "fs.tools").unwrap();
        let tool_ids: Vec<&str> = tools.submenu.iter().map(|i| i.id.as_str()).collect();
        assert!(tool_ids.contains(&"fs.compare-dirs"));
        assert!(tool_ids.contains(&"fs.split"));
        assert!(tool_ids.contains(&"fs.hash"));
        assert!(tool_ids.contains(&"fs.meta"));
        assert!(tool_ids.contains(&"fs.image-edit"));
        let image_edit = tools
            .submenu
            .iter()
            .find(|i| i.id == "fs.image-edit")
            .unwrap();
        assert!(image_edit.submenu.iter().any(|i| i.id == "fs.image-adjust"));
        assert!(image_edit.submenu.iter().any(|i| i.id == "fs.image-filter"));
        assert!(image_edit
            .submenu
            .iter()
            .any(|i| i.id == "fs.image-watermark"));
        assert!(image_edit
            .submenu
            .iter()
            .any(|i| i.id == "fs.image-compare"));
        assert!(image_edit.submenu.iter().any(|i| i.id == "fs.image-print"));
        assert!(image_edit.submenu.iter().any(|i| i.id == "fs.image-export"));
        assert!(image_edit
            .submenu
            .iter()
            .any(|i| i.id == "fs.image-screenshot"));
        let meta = tools.submenu.iter().find(|i| i.id == "fs.meta").unwrap();
        assert!(meta.submenu.iter().any(|i| i.id == "fs.meta-edit"));
        #[cfg(windows)]
        assert!(tool_ids.contains(&"fs.share"));
        #[cfg(windows)]
        assert!(tool_ids.contains(&"fs.versions"));
        #[cfg(windows)]
        assert!(tool_ids.contains(&"fs.bitlocker"));
        let hash = tools.submenu.iter().find(|i| i.id == "fs.hash").unwrap();
        assert!(hash.submenu.iter().any(|i| i.id == "fs.hash-sha256"));
        let open = menu.iter().find(|i| i.id == "fs.open").unwrap();
        let open_sub: Vec<&str> = open.submenu.iter().map(|i| i.id.as_str()).collect();
        assert!(open_sub.contains(&"fs.open-tab"));
        assert!(open_sub.contains(&"fs.open-other-pane"));
        let archive = menu.iter().find(|i| i.id == "fs.archive").unwrap();
        let arc_ids: Vec<&str> = archive.submenu.iter().map(|i| i.id.as_str()).collect();
        assert!(arc_ids.contains(&"fs.archive-create"));
        assert!(arc_ids.contains(&"fs.archive-test"));
    }

    #[test]
    fn archive_menu_inside_archive() {
        let sel = vec![entry("a.txt", FsEntryKind::File, false)];
        let menu = build_for_selection(
            &sel,
            ContextMenuInputs {
                in_archive: true,
                ..Default::default()
            },
        );
        let archive = menu.iter().find(|i| i.id == "fs.archive").unwrap();
        let extract_sel = archive
            .submenu
            .iter()
            .find(|i| i.id == "fs.archive-extract-selected")
            .unwrap();
        assert!(extract_sel.enabled);
        let create = archive
            .submenu
            .iter()
            .find(|i| i.id == "fs.archive-create")
            .unwrap();
        assert!(!create.enabled);
        let delete = archive
            .submenu
            .iter()
            .find(|i| i.id == "fs.archive-delete")
            .unwrap();
        assert!(delete.enabled);
    }

    fn test_locale() -> (LocaleManager, LocaleConfig) {
        (
            LocaleManager::new(orchid_i18n::default_language(), None).expect("locale"),
            LocaleConfig::default(),
        )
    }

    #[test]
    fn info_hidden_for_empty_selection() {
        let (locale, fmt) = test_locale();
        assert!(info_for_selection(&[], &locale, &fmt).is_none());
    }

    #[test]
    fn info_for_single_file_includes_properties() {
        let (locale, fmt) = test_locale();
        let mut file = entry("readme.txt", FsEntryKind::File, false);
        file.metadata.size = 2048;
        file.metadata.mime = Some("text/plain".into());
        let info = info_for_selection(&[file], &locale, &fmt).unwrap();
        assert_eq!(info.name, "readme.txt");
        assert!(info.type_line.contains("File"));
        assert!(!info.size_line.is_empty());
        assert!(!info.modified_line.is_empty());
        assert!(info.mime_line.contains("text/plain"));
    }

    #[test]
    fn info_for_multi_selection_summarizes() {
        let (locale, fmt) = test_locale();
        let mut a = entry("a.txt", FsEntryKind::File, false);
        a.metadata.size = 100;
        let dir = entry("docs", FsEntryKind::Directory, false);
        let info = info_for_selection(&[a, dir], &locale, &fmt).unwrap();
        assert!(info.name.contains("2"));
        assert!(info.modified_line.is_empty());
        assert!(info.mime_line.is_empty());
        assert!(!info.size_line.is_empty());
        assert!(!info.type_line.is_empty());
    }
}
