//! Context-menu descriptor builder.
//!
//! Produces a flat (with submenus) list of actionable items whose
//! `enabled` flag depends on the current selection. The items carry
//! command ids that the UI layer maps to [`orchid_core::Action`]
//! dispatches.

use orchid_fs::FsEntry;

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

    if !has_selection {
        items.push(item(
            "fs.new-folder",
            "fm-action-new-folder",
            "action-new-folder",
            true,
        ));
        items.last_mut().unwrap().separator_after = true;
    }

    items.push(ContextMenuItem {
        id: if count > 1 { "fs.open-all".into() } else { "fs.open".into() },
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
        ],
    });
    items.last_mut().unwrap().separator_after = true;

    items.push(item("fs.copy", "fm-action-copy", "action-copy", has_selection));
    items.push(item("fs.cut", "fm-action-cut", "action-cut", has_selection));
    items.push(sep(item(
        "fs.paste",
        "fm-action-paste",
        "action-paste",
        inputs.clipboard_has_contents,
    )));

    items.push(item("fs.rename", "fm-action-rename", "action-rename", single));
    items.push(sep(item(
        "fs.delete",
        "fm-action-delete",
        "action-delete",
        has_selection,
    )));

    items.push(ContextMenuItem {
        id: "fs.tag-add".into(),
        label_key: "fm-action-add-tag".into(),
        icon: "action-tag",
        swatch_color: None,
        enabled: has_selection && !inputs.known_tags.is_empty(),
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
            color_item("fs.color-label:orange", "fm-color-orange", "orange", has_selection),
            color_item("fs.color-label:yellow", "fm-color-yellow", "yellow", has_selection),
            color_item("fs.color-label:green", "fm-color-green", "green", has_selection),
            color_item("fs.color-label:blue", "fm-color-blue", "blue", has_selection),
            color_item("fs.color-label:purple", "fm-color-purple", "purple", has_selection),
            color_item("fs.color-label:gray", "fm-color-gray", "gray", has_selection),
            color_item("fs.color-label:none", "fm-color-none", "none", has_selection),
        ],
    });
    items.push(sep(if inputs.any_starred && inputs.all_starred {
        item("fs.unstar", "fm-action-unstar", "action-star", has_selection)
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
    items.last_mut().unwrap().separator_after = true;

    items.push(item(
        "fs.select-all",
        "fm-action-select-all",
        "action-select-all",
        inputs.entry_count > 0,
    ));
    items.push(sep(item(
        "fs.deselect-all",
        "fm-action-deselect-all",
        "action-deselect",
        inputs.selection_count > 0,
    )));

    items.push(item(
        "fs.properties",
        "fm-action-properties",
        "action-properties",
        has_selection,
    ));

    items
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

    #[test]
    fn empty_background_offers_new_folder() {
        let menu = build_for_selection(&[], ContextMenuInputs::default());
        let nf = menu.iter().find(|i| i.id == "fs.new-folder").unwrap();
        assert!(nf.enabled);
    }

    #[test]
    fn no_selection_disables_most_actions() {
        let menu = build_for_selection(&[], ContextMenuInputs::default());
        let copy = menu.iter().find(|i| i.id == "fs.copy").unwrap();
        assert!(!copy.enabled);
        let paste = menu.iter().find(|i| i.id == "fs.paste").unwrap();
        assert!(!paste.enabled);
    }

    #[test]
    fn clipboard_flag_enables_paste() {
        let inputs = ContextMenuInputs {
            clipboard_has_contents: true,
            ..Default::default()
        };
        let menu = build_for_selection(&[], inputs);
        let paste = menu.iter().find(|i| i.id == "fs.paste").unwrap();
        assert!(paste.enabled);
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
}
