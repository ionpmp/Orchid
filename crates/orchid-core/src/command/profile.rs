//! OS / commander shortcut profiles and user-remap resolution.
//!
//! Effective binding for an id is: **user override**, else the selected
//! [`ShortcutProfile`] default, else unbound. The Orchid profile keeps the
//! Norton Commander–style keys the file manager shipped with.

use std::collections::HashMap;

use super::shortcut::{is_reserved, Shortcut};

/// Built-in keyboard convention used to seed defaults before user remaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutProfile {
    /// Dual-pane Commander keys (`F3`–`F8`) plus Ctrl chords.
    Orchid,
    /// Windows Explorer / common Win32 conventions.
    Windows,
    /// macOS / Finder conventions (`Cmd` is the [`super::Key`] `Win` modifier).
    Macos,
    /// GNOME / KDE file-manager conventions.
    Linux,
}

impl ShortcutProfile {
    /// Parse a config value (`orchid`, `windows`, `macos`, `linux`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "orchid" | "commander" => Some(Self::Orchid),
            "windows" | "win" => Some(Self::Windows),
            "macos" | "mac" | "osx" | "darwin" => Some(Self::Macos),
            "linux" | "gnome" | "kde" => Some(Self::Linux),
            _ => None,
        }
    }

    /// Canonical config / combo value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Orchid => "orchid",
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Linux => "linux",
        }
    }
}

/// One remappable command or file-manager action.
#[derive(Debug, Clone, Copy)]
pub struct ProfileBinding {
    /// Stable command / action id (`settings.open`, `fs.rename`, …).
    pub id: &'static str,
    /// Fluent key for the Settings row label.
    pub label_key: &'static str,
    orchid: &'static str,
    windows: &'static str,
    macos: &'static str,
    linux: &'static str,
}

impl ProfileBinding {
    fn for_profile(self, profile: ShortcutProfile) -> &'static str {
        match profile {
            ShortcutProfile::Orchid => self.orchid,
            ShortcutProfile::Windows => self.windows,
            ShortcutProfile::Macos => self.macos,
            ShortcutProfile::Linux => self.linux,
        }
    }
}

/// App-level and file-manager bindings that Settings can remap.
pub const PROFILE_BINDINGS: &[ProfileBinding] = &[
    ProfileBinding {
        id: "command-palette",
        label_key: "settings-bind-command-palette",
        orchid: "Ctrl+Shift+P",
        windows: "Ctrl+Shift+P",
        macos: "Win+Shift+P",
        linux: "Ctrl+Shift+P",
    },
    ProfileBinding {
        id: "settings.open",
        label_key: "command-settings-open-name",
        orchid: "Ctrl+,",
        windows: "Ctrl+,",
        macos: "Win+,",
        linux: "Ctrl+,",
    },
    ProfileBinding {
        id: "onboarding.toggle_hint_mode",
        label_key: "command-onboarding-toggle_hint_mode-name",
        orchid: "Win+?",
        windows: "Win+?",
        macos: "Win+Shift+/",
        linux: "Win+?",
    },
    ProfileBinding {
        id: "widget.close",
        label_key: "command-widget-close-name",
        orchid: "Ctrl+W",
        windows: "Ctrl+W",
        macos: "Win+W",
        linux: "Ctrl+W",
    },
    ProfileBinding {
        id: "workspace.switch_next",
        label_key: "command-workspace-switch_next-name",
        orchid: "Ctrl+Alt+ArrowRight",
        windows: "Ctrl+Alt+ArrowRight",
        macos: "Alt+Win+ArrowRight",
        linux: "Ctrl+Alt+ArrowRight",
    },
    ProfileBinding {
        id: "workspace.switch_previous",
        label_key: "command-workspace-switch_previous-name",
        orchid: "Ctrl+Alt+ArrowLeft",
        windows: "Ctrl+Alt+ArrowLeft",
        macos: "Alt+Win+ArrowLeft",
        linux: "Ctrl+Alt+ArrowLeft",
    },
    ProfileBinding {
        id: "terminal.split_horizontal",
        label_key: "command-terminal-split_horizontal-name",
        orchid: "Ctrl+Shift+H",
        windows: "Ctrl+Shift+H",
        macos: "Win+Shift+H",
        linux: "Ctrl+Shift+H",
    },
    ProfileBinding {
        id: "terminal.split_vertical",
        label_key: "command-terminal-split_vertical-name",
        orchid: "Ctrl+Shift+J",
        windows: "Ctrl+Shift+J",
        macos: "Win+Shift+J",
        linux: "Ctrl+Shift+J",
    },
    ProfileBinding {
        id: "terminal.tab_new",
        label_key: "command-terminal-tab_new-name",
        orchid: "Ctrl+Shift+T",
        windows: "Ctrl+Shift+T",
        macos: "Win+Shift+T",
        linux: "Ctrl+Shift+T",
    },
    ProfileBinding {
        id: "terminal.close",
        label_key: "command-terminal-close-name",
        orchid: "Ctrl+Shift+W",
        windows: "Ctrl+Shift+W",
        macos: "Win+Shift+W",
        linux: "Ctrl+Shift+W",
    },
    ProfileBinding {
        id: "terminal.focus_next_pane",
        label_key: "command-terminal-focus_next_pane-name",
        orchid: "Ctrl+Shift+ArrowRight",
        windows: "Ctrl+Shift+ArrowRight",
        macos: "Win+Shift+ArrowRight",
        linux: "Ctrl+Shift+ArrowRight",
    },
    ProfileBinding {
        id: "terminal.focus_previous_pane",
        label_key: "command-terminal-focus_previous_pane-name",
        orchid: "Ctrl+Shift+ArrowLeft",
        windows: "Ctrl+Shift+ArrowLeft",
        macos: "Win+Shift+ArrowLeft",
        linux: "Ctrl+Shift+ArrowLeft",
    },
    ProfileBinding {
        id: "terminal.tab_next",
        label_key: "command-terminal-tab_next-name",
        orchid: "Ctrl+PageDown",
        windows: "Ctrl+PageDown",
        macos: "Win+Shift+]",
        linux: "Ctrl+PageDown",
    },
    ProfileBinding {
        id: "terminal.tab_previous",
        label_key: "command-terminal-tab_previous-name",
        orchid: "Ctrl+PageUp",
        windows: "Ctrl+PageUp",
        macos: "Win+Shift+[",
        linux: "Ctrl+PageUp",
    },
    ProfileBinding {
        id: "fs.select-all",
        label_key: "fm-action-select-all",
        orchid: "Ctrl+A",
        windows: "Ctrl+A",
        macos: "Win+A",
        linux: "Ctrl+A",
    },
    ProfileBinding {
        id: "fs.deselect-all",
        label_key: "fm-action-deselect-all",
        orchid: "Escape",
        windows: "Escape",
        macos: "Escape",
        linux: "Escape",
    },
    ProfileBinding {
        id: "fs.invert-selection",
        label_key: "fm-action-invert-selection",
        orchid: "*",
        windows: "*",
        macos: "*",
        linux: "*",
    },
    ProfileBinding {
        id: "fs.select-mask-add",
        label_key: "fm-action-select-mask",
        orchid: "+",
        windows: "+",
        macos: "+",
        linux: "+",
    },
    ProfileBinding {
        id: "fs.select-mask-sub",
        label_key: "fm-action-deselect-mask",
        orchid: "-",
        windows: "-",
        macos: "-",
        linux: "-",
    },
    ProfileBinding {
        id: "fs.copy",
        label_key: "fm-action-copy",
        orchid: "Ctrl+C",
        windows: "Ctrl+C",
        macos: "Win+C",
        linux: "Ctrl+C",
    },
    ProfileBinding {
        id: "fs.cut",
        label_key: "fm-action-cut",
        orchid: "Ctrl+X",
        windows: "Ctrl+X",
        macos: "Win+X",
        linux: "Ctrl+X",
    },
    ProfileBinding {
        id: "fs.paste",
        label_key: "fm-action-paste",
        orchid: "Ctrl+V",
        windows: "Ctrl+V",
        macos: "Win+V",
        linux: "Ctrl+V",
    },
    ProfileBinding {
        id: "fs.undo",
        label_key: "fm-action-undo",
        orchid: "Ctrl+Z",
        windows: "Ctrl+Z",
        macos: "Win+Z",
        linux: "Ctrl+Z",
    },
    ProfileBinding {
        id: "fs.redo",
        label_key: "fm-action-redo",
        orchid: "Ctrl+Y",
        windows: "Ctrl+Y",
        macos: "Win+Shift+Z",
        linux: "Ctrl+Shift+Z",
    },
    ProfileBinding {
        id: "fs.rename",
        label_key: "fm-action-rename",
        orchid: "F2",
        windows: "F2",
        macos: "F2",
        linux: "F2",
    },
    ProfileBinding {
        id: "fs.delete",
        label_key: "fm-action-delete",
        orchid: "F8",
        windows: "Delete",
        macos: "Win+Backspace",
        linux: "Delete",
    },
    ProfileBinding {
        id: "fs.delete-permanent",
        label_key: "fm-action-delete-permanent",
        orchid: "Shift+Delete",
        windows: "Shift+Delete",
        macos: "Win+Shift+Backspace",
        linux: "Shift+Delete",
    },
    ProfileBinding {
        id: "fs.new-folder",
        label_key: "fm-action-new-folder",
        orchid: "F7",
        windows: "Ctrl+Shift+N",
        macos: "Win+Shift+N",
        linux: "Ctrl+Shift+N",
    },
    ProfileBinding {
        id: "fs.new-file",
        label_key: "fm-action-new-file",
        orchid: "Shift+F4",
        windows: "",
        macos: "",
        linux: "",
    },
    ProfileBinding {
        id: "viewer.open",
        label_key: "fm-action-open-in-viewer",
        orchid: "F3",
        windows: "",
        macos: "Space",
        linux: "",
    },
    ProfileBinding {
        id: "viewer.edit",
        label_key: "fm-action-edit-in-viewer",
        orchid: "F4",
        windows: "",
        macos: "",
        linux: "",
    },
    ProfileBinding {
        id: "fs.copy-to-other",
        label_key: "fm-action-copy-other",
        orchid: "F5",
        windows: "",
        macos: "",
        linux: "",
    },
    ProfileBinding {
        id: "fs.move-to-other",
        label_key: "fm-action-move-other",
        orchid: "F6",
        windows: "",
        macos: "",
        linux: "",
    },
    ProfileBinding {
        id: "fs.open-tab",
        label_key: "fm-action-open-tab",
        orchid: "Ctrl+Shift+T",
        windows: "Ctrl+Shift+T",
        macos: "Win+Shift+T",
        linux: "Ctrl+Shift+T",
    },
    ProfileBinding {
        id: "fs.open-other-pane",
        label_key: "fm-action-open-other-pane",
        orchid: "Ctrl+Shift+Enter",
        windows: "Ctrl+Shift+Enter",
        macos: "Win+Shift+Enter",
        linux: "Ctrl+Shift+Enter",
    },
    ProfileBinding {
        id: "fs.branch-view",
        label_key: "fm-action-branch-view",
        orchid: "Ctrl+B",
        windows: "Ctrl+B",
        macos: "Win+B",
        linux: "Ctrl+B",
    },
    ProfileBinding {
        id: "fs.find",
        label_key: "fm-action-find",
        orchid: "Alt+F7",
        windows: "Ctrl+F",
        macos: "Win+F",
        linux: "Ctrl+F",
    },
    ProfileBinding {
        id: "fs.properties",
        label_key: "fm-action-properties",
        orchid: "Alt+Enter",
        windows: "Alt+Enter",
        macos: "Win+I",
        linux: "Alt+Enter",
    },
    ProfileBinding {
        id: "fs.address-bar",
        label_key: "settings-bind-address-bar",
        orchid: "Ctrl+L",
        windows: "Ctrl+L",
        macos: "Win+Shift+G",
        linux: "Ctrl+L",
    },
    ProfileBinding {
        id: "fs.tab-new",
        label_key: "settings-bind-tab-new",
        orchid: "Ctrl+T",
        windows: "Ctrl+T",
        macos: "Win+T",
        linux: "Ctrl+T",
    },
    ProfileBinding {
        id: "fs.drive-root",
        label_key: "settings-bind-drive-root",
        orchid: "Ctrl+\\",
        windows: "Ctrl+\\",
        macos: "Win+Shift+Up",
        linux: "Ctrl+\\",
    },
    ProfileBinding {
        id: "fs.drives-menu",
        label_key: "settings-bind-drives-menu",
        orchid: "Alt+F1",
        windows: "Alt+F1",
        macos: "",
        linux: "Alt+F1",
    },
];

/// Profile default string for `id`, or `None` if the id is not remappable.
///
/// `Some("")` means the profile leaves the action unbound.
#[must_use]
pub fn profile_default_str(profile: ShortcutProfile, id: &str) -> Option<&'static str> {
    PROFILE_BINDINGS
        .iter()
        .find(|b| b.id == id)
        .map(|b| b.for_profile(profile))
}

/// Resolve the effective shortcut for `id`.
#[must_use]
pub fn resolve_profile_shortcut(
    profile: ShortcutProfile,
    overrides: &HashMap<String, String>,
    id: &str,
) -> Option<Shortcut> {
    if let Some(raw) = overrides.get(id) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Shortcut::parse(trimmed).ok();
    }
    let def = profile_default_str(profile, id)?;
    if def.is_empty() {
        return None;
    }
    Shortcut::parse(def).ok()
}

/// Canonical display form; macOS profiles show `Cmd` instead of `Win`.
#[must_use]
pub fn display_profile_shortcut(profile: ShortcutProfile, shortcut: &Shortcut) -> String {
    let canonical = shortcut.to_string_canonical();
    if profile == ShortcutProfile::Macos {
        canonical.replace("Win+", "Cmd+")
    } else {
        canonical
    }
}

/// Whether `id` is a file-manager action (not a global command).
#[must_use]
pub fn is_file_manager_binding(id: &str) -> bool {
    id.starts_with("fs.") || id.starts_with("viewer.")
}

/// First remappable FM action whose effective shortcut matches `pressed`.
#[must_use]
pub fn lookup_fm_action(
    profile: ShortcutProfile,
    overrides: &HashMap<String, String>,
    pressed: &Shortcut,
) -> Option<&'static str> {
    for binding in PROFILE_BINDINGS {
        if !is_file_manager_binding(binding.id) {
            continue;
        }
        if resolve_profile_shortcut(profile, overrides, binding.id).as_ref() == Some(pressed) {
            return Some(binding.id);
        }
    }
    None
}

/// Another remappable id that already uses `shortcut`, if any.
#[must_use]
pub fn binding_conflict(
    profile: ShortcutProfile,
    overrides: &HashMap<String, String>,
    id: &str,
    shortcut: &Shortcut,
) -> Option<&'static str> {
    for binding in PROFILE_BINDINGS {
        if binding.id == id {
            continue;
        }
        if resolve_profile_shortcut(profile, overrides, binding.id).as_ref() == Some(shortcut) {
            return Some(binding.id);
        }
    }
    None
}

/// Reject reserved combos and conflicts when writing an override.
pub fn validate_override(
    profile: ShortcutProfile,
    overrides: &HashMap<String, String>,
    id: &str,
    shortcut: &Shortcut,
) -> std::result::Result<(), String> {
    if profile_default_str(profile, id).is_none() {
        return Err(format!("unknown remappable id `{id}`"));
    }
    if let Some(reason) = is_reserved(shortcut) {
        return Err(reason.to_string());
    }
    if let Some(other) = binding_conflict(profile, overrides, id, shortcut) {
        return Err(format!("conflicts with `{other}`"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_profile_aliases() {
        assert_eq!(
            ShortcutProfile::parse("Commander"),
            Some(ShortcutProfile::Orchid)
        );
        assert_eq!(ShortcutProfile::parse("mac"), Some(ShortcutProfile::Macos));
        assert_eq!(ShortcutProfile::parse("nope"), None);
    }

    #[test]
    fn orchid_keeps_commander_keys() {
        let empty = HashMap::new();
        assert_eq!(
            resolve_profile_shortcut(ShortcutProfile::Orchid, &empty, "fs.copy-to-other")
                .unwrap()
                .to_string_canonical(),
            "F5"
        );
        assert_eq!(
            resolve_profile_shortcut(ShortcutProfile::Windows, &empty, "fs.copy-to-other"),
            None
        );
    }

    #[test]
    fn macos_uses_cmd_for_copy() {
        let empty = HashMap::new();
        let sc = resolve_profile_shortcut(ShortcutProfile::Macos, &empty, "fs.copy").unwrap();
        assert_eq!(
            display_profile_shortcut(ShortcutProfile::Macos, &sc),
            "Cmd+C"
        );
        assert_eq!(
            lookup_fm_action(ShortcutProfile::Macos, &empty, &sc),
            Some("fs.copy")
        );
    }

    #[test]
    fn override_wins_and_can_unbind() {
        let mut overrides = HashMap::new();
        overrides.insert("fs.rename".into(), "F6".into());
        assert_eq!(
            resolve_profile_shortcut(ShortcutProfile::Orchid, &overrides, "fs.rename")
                .unwrap()
                .to_string_canonical(),
            "F6"
        );
        overrides.insert("fs.rename".into(), String::new());
        assert_eq!(
            resolve_profile_shortcut(ShortcutProfile::Orchid, &overrides, "fs.rename"),
            None
        );
    }

    #[test]
    fn conflict_detects_other_action() {
        let empty = HashMap::new();
        let f5 = Shortcut::parse("F5").unwrap();
        assert_eq!(
            binding_conflict(ShortcutProfile::Orchid, &empty, "fs.rename", &f5),
            Some("fs.copy-to-other")
        );
        assert!(validate_override(ShortcutProfile::Orchid, &empty, "fs.rename", &f5).is_err());
    }

    #[test]
    fn undo_redo_profile_defaults() {
        let empty = HashMap::new();
        let z = Shortcut::parse("Ctrl+Z").unwrap();
        assert_eq!(
            lookup_fm_action(ShortcutProfile::Orchid, &empty, &z),
            Some("fs.undo")
        );
        let y = Shortcut::parse("Ctrl+Y").unwrap();
        assert_eq!(
            lookup_fm_action(ShortcutProfile::Windows, &empty, &y),
            Some("fs.redo")
        );
        let macos_z = resolve_profile_shortcut(ShortcutProfile::Macos, &empty, "fs.undo").unwrap();
        assert_eq!(macos_z.to_string_canonical(), "Win+Z");
        let linux_redo =
            resolve_profile_shortcut(ShortcutProfile::Linux, &empty, "fs.redo").unwrap();
        assert_eq!(linux_redo.to_string_canonical(), "Ctrl+Shift+Z");
    }
}
