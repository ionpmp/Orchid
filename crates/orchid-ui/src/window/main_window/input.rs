//! Keyboard shortcuts, leader chords, gestures, and touch input.

use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::ComponentHandle;
use tracing::{debug, warn};

use orchid_core::{
    default_bindings_mirrored, CommandRegistry, InputEvent, RecognizedGesture, ScreenBounds,
    Shortcut, TouchEvent,
};
use orchid_widgets::WidgetPayload;

use crate::window::errors::viewer_localized_error;
use crate::window::spawn;

use super::MainWindowController;

impl MainWindowController {
    pub(super) fn command_palette_shortcut(&self) -> Shortcut {
        self.config
            .read()
            .shortcuts
            .overrides
            .get("command-palette")
            .and_then(|s| Shortcut::parse(s).ok())
            .unwrap_or_else(|| Shortcut::parse("Ctrl+Shift+P").expect("valid default shortcut"))
    }

    pub(super) fn leader_key_shortcut(&self) -> Option<Shortcut> {
        let cfg = self.config.read();
        let key = cfg.shortcuts.leader_key.as_ref()?;
        if key.is_empty() {
            return None;
        }
        Shortcut::parse(key).ok()
    }

    pub(super) fn clear_leader_pending(&self) {
        *self.leader_pending_until.lock() = None;
    }

    /// Ctrl+S fallback for the last edited text viewer (when focus left the TextInput).
    pub(super) fn try_viewer_text_ctrl_s(
        self: &Arc<Self>,
        mods: slint::winit_030::winit::keyboard::ModifiersState,
        logical: &slint::winit_030::winit::keyboard::Key,
    ) -> bool {
        use slint::winit_030::winit::keyboard::Key;
        if !mods.control_key() || mods.shift_key() || mods.alt_key() || mods.super_key() {
            return false;
        }
        let is_s = matches!(logical, Key::Character(s) if s.eq_ignore_ascii_case("s"));
        if !is_s {
            return false;
        }
        let Some(inst) = *self.last_text_edit_instance.lock() else {
            return false;
        };
        let cache = self.widget_manager.snapshot_cache();
        let Some(ws) = cache.get(inst) else {
            return false;
        };
        let WidgetPayload::Viewer(v) = &ws.payload else {
            return false;
        };
        let orchid_viewers::ViewerSnapshot::Text(t) = &v.snapshot else {
            return false;
        };
        if t.read_only {
            return false;
        }
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = orchid_widgets::builtin::viewer::text_save(inst).await {
                warn!(?e, "viewer text Ctrl+S");
                if let Some(c) = tw.upgrade() {
                    let title = c.locale.tr("widget-viewer-name");
                    let reason = viewer_localized_error(&c.locale, &e.to_string());
                    let body = c.locale.tr_args(
                        "viewer-text-save-failed",
                        &orchid_i18n::FluentArgs::new().with("reason", reason),
                    );
                    c.push_notification(&title, &body, 3);
                }
                return;
            }
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        });
        true
    }

    pub(super) fn try_activate_leader(
        &self,
        mods: slint::winit_030::winit::keyboard::ModifiersState,
        logical: &slint::winit_030::winit::keyboard::Key,
    ) -> bool {
        let Some(sc) = self.leader_key_shortcut() else {
            return false;
        };
        if !winit_modifiers_match(sc.modifiers, mods) || !winit_key_matches(sc.key, logical) {
            return false;
        }
        let timeout_ms = self.config.read().shortcuts.leader_timeout_ms;
        *self.leader_pending_until.lock() =
            Some(Instant::now() + Duration::from_millis(timeout_ms));
        debug!(target: "orchid_ui::shortcuts", "leader-key armed");
        true
    }

    pub(super) fn try_leader_chord(
        &self,
        mods: slint::winit_030::winit::keyboard::ModifiersState,
        logical: &slint::winit_030::winit::keyboard::Key,
    ) -> Option<String> {
        use slint::winit_030::winit::keyboard::{Key, NamedKey};
        {
            let guard = self.leader_pending_until.lock();
            let until = (*guard)?;
            if Instant::now() > until {
                drop(guard);
                self.clear_leader_pending();
                return None;
            }
        }

        if mods.control_key() || mods.alt_key() || mods.super_key() {
            self.clear_leader_pending();
            return None;
        }

        let key_str = match logical {
            Key::Character(s) => {
                let ch = s.chars().next()?;
                if ch.is_ascii_alphabetic() {
                    ch.to_ascii_lowercase().to_string()
                } else {
                    self.clear_leader_pending();
                    return None;
                }
            }
            Key::Named(NamedKey::Escape) => {
                self.clear_leader_pending();
                return None;
            }
            _ => {
                self.clear_leader_pending();
                return None;
            }
        };

        let cmd_id = self
            .config
            .read()
            .shortcuts
            .leader_bindings
            .get(&key_str)
            .cloned();
        self.clear_leader_pending();
        if let Some(ref id) = cmd_id {
            debug!(target: "orchid_ui::shortcuts", cmd_id = %id, key = %key_str, "leader chord");
        }
        cmd_id
    }

    pub(super) fn apply_command_shortcut_overrides(self: &Arc<Self>) {
        let overrides = self.config.read().shortcuts.overrides.clone();
        if overrides.is_empty() {
            return;
        }
        for result in self.command_registry.apply_shortcut_overrides(&overrides) {
            if let Err(reason) = result.outcome {
                warn!(
                    command = %result.command_id,
                    reason = %reason,
                    "shortcut override rejected"
                );
            }
        }
    }

    pub(super) fn apply_input_gesture_bindings(self: &Arc<Self>) {
        let cfg = self.config.read();
        let swap = matches!(cfg.input.primary_hand, orchid_storage::Hand::Left)
            || cfg.input.mirror_edge_swipes;
        self.input_mapper.set_bindings(default_bindings_mirrored(swap));
    }

    pub(super) fn dispatch_registry_shortcut(self: &Arc<Self>, cmd_id: String) {
        let this = Arc::clone(self);
        spawn::spawn_local_compat(async move {
            this.dispatch_command(&cmd_id).await;
            this.schedule_rebuild();
        });
    }

    pub(super) fn update_gesture_bounds(self: &Arc<Self>) {
        let win = self.window.window();
        let p = win.size();
        if p.width < 2 || p.height < 2 {
            return;
        }
        let log = p.to_logical(win.scale_factor());
        self.gesture_recognizer.lock().set_bounds(ScreenBounds::new(
            log.width,
            log.height,
        ));
    }

    pub(super) fn handle_recognized_gestures(
        self: &Arc<Self>,
        gestures: impl IntoIterator<Item = RecognizedGesture>,
    ) {
        let gestures: Vec<_> = gestures.into_iter().collect();
        if gestures.is_empty() {
            return;
        }
        let win = self.window.window();
        let p = win.size();
        if p.width < 2 || p.height < 2 {
            return;
        }
        let log = p.to_logical(win.scale_factor());
        let bounds = ScreenBounds::new(log.width, log.height);
        for gesture in gestures {
            if let Some(cmd_id) = self.input_mapper.resolve_gesture(&gesture, bounds) {
                debug!(target: "orchid_ui::gestures", cmd_id = %cmd_id, ?gesture, "gesture resolved");
                self.dispatch_registry_shortcut(cmd_id);
            }
        }
    }

    pub(super) fn feed_touch_input(self: &Arc<Self>, touch: TouchEvent) {
        let gestures = self.gesture_recognizer.lock().feed(&InputEvent::Touch(touch));
        self.handle_recognized_gestures(gestures);
    }
}

pub(super) fn resolve_registry_shortcut(
    registry: &CommandRegistry,
    shortcut: &Shortcut,
) -> Option<String> {
    registry.list_all().into_iter().find_map(|desc| {
        registry
            .effective_shortcut(&desc.id)
            .filter(|s| shortcuts_equivalent(s, shortcut))
            .map(|_| desc.id)
    })
}

/// Match shortcuts from winit, allowing an extra Shift for punctuation keys
/// (e.g. `Win+?` is typed as Win+Shift+? on US layouts).
pub(super) fn shortcuts_equivalent(expected: &Shortcut, actual: &Shortcut) -> bool {
    use orchid_core::{Key, Modifiers};
    if expected == actual {
        return true;
    }
    if expected.key != actual.key {
        return false;
    }
    if matches!(expected.key, Key::Char(c) if !c.is_ascii_alphabetic())
        && !expected.modifiers.contains(Modifiers::SHIFT)
        && actual.modifiers == expected.modifiers | Modifiers::SHIFT
    {
        return true;
    }
    false
}

pub(super) fn winit_to_shortcut(
    state: slint::winit_030::winit::keyboard::ModifiersState,
    logical: &slint::winit_030::winit::keyboard::Key,
) -> Option<Shortcut> {
    use orchid_core::{Key as Ok, Modifiers};
    use slint::winit_030::winit::keyboard::{Key, NamedKey};

    let mut modifiers = Modifiers::empty();
    if state.control_key() {
        modifiers |= Modifiers::CTRL;
    }
    if state.shift_key() {
        modifiers |= Modifiers::SHIFT;
    }
    if state.alt_key() {
        modifiers |= Modifiers::ALT;
    }
    if state.super_key() {
        modifiers |= Modifiers::WIN;
    }

    let key = match logical {
        Key::Character(s) => {
            let ch = s.chars().next()?;
            if ch.is_ascii_alphabetic() {
                Ok::Char(ch.to_ascii_lowercase())
            } else {
                Ok::Char(ch)
            }
        }
        Key::Named(NamedKey::Escape) => Ok::Escape,
        Key::Named(NamedKey::Enter) => Ok::Enter,
        Key::Named(NamedKey::Tab) => Ok::Tab,
        Key::Named(NamedKey::Backspace) => Ok::Backspace,
        Key::Named(NamedKey::Delete) => Ok::Delete,
        Key::Named(NamedKey::Insert) => Ok::Insert,
        Key::Named(NamedKey::Home) => Ok::Home,
        Key::Named(NamedKey::End) => Ok::End,
        Key::Named(NamedKey::PageUp) => Ok::PageUp,
        Key::Named(NamedKey::PageDown) => Ok::PageDown,
        Key::Named(NamedKey::ArrowUp) => Ok::ArrowUp,
        Key::Named(NamedKey::ArrowDown) => Ok::ArrowDown,
        Key::Named(NamedKey::ArrowLeft) => Ok::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => Ok::ArrowRight,
        Key::Named(NamedKey::Space) => Ok::Space,
        Key::Named(named) => Ok::F(winit_named_f_index(named)?),
        _ => return None,
    };
    Some(Shortcut::new(modifiers, key))
}

pub(super) fn winit_touch_to_orchid(
    touch: &slint::winit_030::winit::event::Touch,
    window: &slint::Window,
) -> Option<TouchEvent> {
    use orchid_core::{Point, TouchPhase};
    use slint::winit_030::winit::event::TouchPhase as WinitTouchPhase;

    let scale = f64::from(window.scale_factor());
    let logical: slint::winit_030::winit::dpi::LogicalPosition<f64> =
        touch.location.to_logical(scale);
    let phase = match touch.phase {
        WinitTouchPhase::Started => TouchPhase::Began,
        WinitTouchPhase::Moved => TouchPhase::Moved,
        WinitTouchPhase::Ended => TouchPhase::Ended,
        WinitTouchPhase::Cancelled => TouchPhase::Cancelled,
    };
    Some(TouchEvent {
        pointer_id: touch.id as u32,
        phase,
        position: Point::new(logical.x as f32, logical.y as f32),
        pressure: 1.0,
        size: 10.0,
        timestamp: Instant::now(),
    })
}

pub(super) fn winit_modifiers_match(
    shortcut_mods: orchid_core::Modifiers,
    state: slint::winit_030::winit::keyboard::ModifiersState,
) -> bool {
    use orchid_core::Modifiers;
    state.control_key() == shortcut_mods.contains(Modifiers::CTRL)
        && state.shift_key() == shortcut_mods.contains(Modifiers::SHIFT)
        && state.alt_key() == shortcut_mods.contains(Modifiers::ALT)
        && state.super_key() == shortcut_mods.contains(Modifiers::WIN)
}

pub(super) fn winit_key_matches(
    shortcut_key: orchid_core::Key,
    logical: &slint::winit_030::winit::keyboard::Key,
) -> bool {
    use orchid_core::Key as Ok;
    use slint::winit_030::winit::keyboard::{Key, NamedKey};
    match (shortcut_key, logical) {
        (Ok::Char(c), Key::Character(s)) => s.as_str().eq_ignore_ascii_case(&c.to_string()),
        (Ok::Escape, Key::Named(NamedKey::Escape)) => true,
        (Ok::Enter, Key::Named(NamedKey::Enter)) => true,
        (Ok::Tab, Key::Named(NamedKey::Tab)) => true,
        (Ok::Backspace, Key::Named(NamedKey::Backspace)) => true,
        (Ok::Delete, Key::Named(NamedKey::Delete)) => true,
        (Ok::Insert, Key::Named(NamedKey::Insert)) => true,
        (Ok::Home, Key::Named(NamedKey::Home)) => true,
        (Ok::End, Key::Named(NamedKey::End)) => true,
        (Ok::PageUp, Key::Named(NamedKey::PageUp)) => true,
        (Ok::PageDown, Key::Named(NamedKey::PageDown)) => true,
        (Ok::ArrowUp, Key::Named(NamedKey::ArrowUp)) => true,
        (Ok::ArrowDown, Key::Named(NamedKey::ArrowDown)) => true,
        (Ok::ArrowLeft, Key::Named(NamedKey::ArrowLeft)) => true,
        (Ok::ArrowRight, Key::Named(NamedKey::ArrowRight)) => true,
        (Ok::Space, Key::Named(NamedKey::Space)) => true,
        (Ok::F(n), Key::Named(named)) => winit_named_f_index(named) == Some(n),
        _ => false,
    }
}

fn winit_named_f_index(key: &slint::winit_030::winit::keyboard::NamedKey) -> Option<u8> {
    use slint::winit_030::winit::keyboard::NamedKey;
    Some(match key {
        NamedKey::F1 => 1,
        NamedKey::F2 => 2,
        NamedKey::F3 => 3,
        NamedKey::F4 => 4,
        NamedKey::F5 => 5,
        NamedKey::F6 => 6,
        NamedKey::F7 => 7,
        NamedKey::F8 => 8,
        NamedKey::F9 => 9,
        NamedKey::F10 => 10,
        NamedKey::F11 => 11,
        NamedKey::F12 => 12,
        NamedKey::F13 => 13,
        NamedKey::F14 => 14,
        NamedKey::F15 => 15,
        NamedKey::F16 => 16,
        NamedKey::F17 => 17,
        NamedKey::F18 => 18,
        NamedKey::F19 => 19,
        NamedKey::F20 => 20,
        NamedKey::F21 => 21,
        NamedKey::F22 => 22,
        NamedKey::F23 => 23,
        NamedKey::F24 => 24,
        NamedKey::F25 => 25,
        NamedKey::F26 => 26,
        NamedKey::F27 => 27,
        NamedKey::F28 => 28,
        NamedKey::F29 => 29,
        NamedKey::F30 => 30,
        NamedKey::F31 => 31,
        NamedKey::F32 => 32,
        NamedKey::F33 => 33,
        NamedKey::F34 => 34,
        NamedKey::F35 => 35,
        _ => return None,
    })
}
