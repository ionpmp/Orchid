//! Terminal widget handlers and Slint key encoding.

use std::sync::Arc;

use slint::ComponentHandle;
use slint::Image;
use slint::ModelRc;
use slint::SharedString;
use slint::VecModel;
use tracing::{debug, trace, warn};
use uuid::Uuid;

use orchid_terminal::SplitDirection;
use orchid_widgets::TerminalPayload;

use crate::terminal_raster;
use crate::widgets::terminal::{
    add_tab, close_focused_pane_or_tab, close_pane, close_tab, focus_next_pane, focus_pane,
    focus_previous_pane, set_split_ratio, split_horizontal, split_vertical, switch_tab,
    switch_tab_relative,
};
use crate::window::spawn;
use crate::window::models::{build_terminal_model, pane_payload_to_terminal};
use crate::slint_generated::TerminalPaneModel;

use super::MainWindowController;

impl MainWindowController {
    pub(super) fn resize_terminal_pty_to_content(
        self: &Arc<Self>,
        inst: Uuid,
        viewport_w: f32,
        viewport_h: f32,
    ) -> bool {
        let w = viewport_w.max(1.0);
        let h = viewport_h.max(1.0);
        let layout = self
            .terminal_deps
            .layouts
            .lock()
            .get(&inst)
            .cloned();
        let Some(layout) = layout else {
            return false;
        };
        let snap = layout.snapshot();
        let Some(tab) = snap.tabs.get(snap.active_tab) else {
            return false;
        };
        if tab.panes.is_empty() {
            return false;
        }
        let mut any = false;
        for pane in &tab.panes {
            let pw = w * (pane.bounds.right - pane.bounds.left);
            let ph = h * (pane.bounds.bottom - pane.bounds.top);
            let pty = self.font_metrics.fit(pw.max(1.0), ph.max(1.0));
            {
                let last = self.last_terminal_viewport_pty.lock();
                if last.get(&pane.session) == Some(&(pty.cols, pty.rows)) {
                    continue;
                }
            }
            let Ok(s) = self.session_manager.get(pane.session) else {
                continue;
            };
            if let Err(e) = s.resize(pty) {
                warn!(?e, "pty");
                continue;
            }
            self.last_terminal_viewport_pty
                .lock()
                .insert(pane.session, (pty.cols, pty.rows));
            any = true;
        }
        any
    }

    pub(super) fn raster_terminal_payload(&self, t: &TerminalPayload) -> Image {
        if let Some(ref f) = self.mono_font {
            let size_md = self.theme.current().tokens.typography.size_md;
            let acc = self.theme.current().tokens.color.accent_brand;
            let ccol = [acc.r, acc.g, acc.b, acc.a];
            let cw = self.font_metrics.cell_width_px as u32;
            let ch = self.font_metrics.cell_height_px as u32;
            let scale = self.window.window().scale_factor();
            let glyph_fb = self.mono_font_glyph_fallback.as_ref();
            terminal_raster::render_terminal(
                t,
                f,
                glyph_fb,
                size_md,
                cw,
                ch,
                scale,
                ccol,
            )
            .unwrap_or_default()
        } else {
            Image::default()
        }
    }

    pub(super) fn build_terminal_pane_models(&self, t: &TerminalPayload) -> ModelRc<TerminalPaneModel> {
        let panes: Vec<TerminalPaneModel> = if t.panes.is_empty() {
            let mini = TerminalPayload {
                cols: t.cols,
                rows: t.rows,
                cells: t.cells.clone(),
                cursor_col: t.cursor_col,
                cursor_row: t.cursor_row,
                cursor_visible: t.cursor_visible,
                tabs: Vec::new(),
                active_tab: 0,
                panes: Vec::new(),
                dividers: Vec::new(),
            };
            vec![TerminalPaneModel {
                session_id: SharedString::new(),
                left: 0.0,
                top: 0.0,
                right: 1.0,
                bottom: 1.0,
                is_focused: true,
                show_close: false,
                cols: i32::from(t.cols),
                rows: i32::from(t.rows),
                cells: build_terminal_model(&mini),
                pixels: self.raster_terminal_payload(&mini),
                cursor_col: i32::from(t.cursor_col),
                cursor_row: i32::from(t.cursor_row),
                cursor_visible: t.cursor_visible,
            }]
        } else {
            t.panes
                .iter()
                .map(|p| {
                    let mini = pane_payload_to_terminal(p);
                    TerminalPaneModel {
                        session_id: p.session_id.clone().into(),
                        left: p.left,
                        top: p.top,
                        right: p.right,
                        bottom: p.bottom,
                        is_focused: p.is_focused,
                        show_close: p.show_close,
                        cols: i32::from(p.cols),
                        rows: i32::from(p.rows),
                        cells: build_terminal_model(&mini),
                        pixels: self.raster_terminal_payload(&mini),
                        cursor_col: i32::from(p.cursor_col),
                        cursor_row: i32::from(p.cursor_row),
                        cursor_visible: p.cursor_visible,
                    }
                })
                .collect()
        };
        ModelRc::new(VecModel::from(panes))
    }

    pub(super) fn on_terminal_key(
        self: &Arc<Self>,
        id: &SharedString,
        text: &SharedString,
        ctrl: bool,
        shift: bool,
        alt: bool,
    ) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let Some(sid) = self.session_routing.lock().get(&inst).copied() else {
            trace!(
                target: "orchid_ui::terminal_input",
                %inst,
                "key ignored: no session routing (PTY not ready for this instance)"
            );
            return;
        };
        let Ok(session) = self.session_manager.get(sid) else {
            return;
        };
        let encoder = session.encoder.read();
        let encoded = encode_slint_key_event(text.as_str(), ctrl, shift, alt, &encoder);
        if encoded.is_empty() {
            return;
        }
        trace!(
            target: "orchid_ui::terminal_input",
            ch_len = text.as_str().chars().count(),
            bytes = ?encoded,
            "encode key for PTY"
        );
        if let Err(e) = session.send_input(&encoded) {
            warn!(?e, "input");
            return;
        }
        debug!(
            target: "orchid_ui::terminal_input",
            %sid,
            sent = encoded.len(),
            "forwarding terminal key"
        );
    }
    pub(super) fn on_terminal_viewport(self: &Arc<Self>, id: &SharedString, w: f32, h: f32) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        // `content` width/height `changed` fires on every live resize step; do not
        // resize the PTY here — that thrashes the shell and triggers extra rebuilds.
        // `TerminalView` uses `image-fit: fill` until the PTY is committed in
        // [`on_widget_resize_ended`] and the next non-preview rebuild.
        if self.drag_offset.lock().contains_key(&inst) {
            return;
        }
        if self.resize_override.lock().contains_key(&inst) {
            return;
        }
        if self.resize_terminal_pty_to_content(inst, w, h) {
            self.schedule_rebuild();
        }
    }

    pub(super) fn on_terminal_tab_clicked(self: &Arc<Self>, id: &SharedString, tab_idx: i32) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        if tab_idx < 0 {
            return;
        }
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        let idx = tab_idx as usize;
        spawn::spawn_local_compat(async move {
            if let Err(e) = switch_tab(&deps, inst, idx) {
                warn!(?e, %inst, tab_idx = idx, "terminal tab switch");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(super) fn on_terminal_tab_new(self: &Arc<Self>, id: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = add_tab(&deps, inst).await {
                warn!(?e, %inst, "terminal tab add");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(super) fn on_terminal_tab_closed(self: &Arc<Self>, id: &SharedString, tab_idx: i32) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        if tab_idx < 0 {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        let idx = tab_idx as usize;
        spawn::spawn_local_compat(async move {
            if let Err(e) = close_tab(&deps, inst, idx).await {
                warn!(?e, %inst, tab_idx = idx, "terminal tab close");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(super) fn on_terminal_split_horizontal(self: &Arc<Self>, id: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = split_horizontal(&deps, inst).await {
                warn!(?e, %inst, "terminal split horizontal");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(super) fn on_terminal_split_vertical(self: &Arc<Self>, id: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = split_vertical(&deps, inst).await {
                warn!(?e, %inst, "terminal split vertical");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(super) fn on_terminal_pane_clicked(self: &Arc<Self>, id: &SharedString, session_id: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let Ok(sid) = Uuid::parse_str(session_id.as_str()) else {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = focus_pane(&deps, inst, sid) {
                warn!(?e, %inst, %sid, "terminal pane focus");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(super) fn on_terminal_pane_closed(self: &Arc<Self>, id: &SharedString, session_id: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let Ok(sid) = Uuid::parse_str(session_id.as_str()) else {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = close_pane(&deps, inst, sid).await {
                warn!(?e, %inst, %sid, "terminal pane close");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(super) fn on_terminal_split_drag_moved(
        self: &Arc<Self>,
        id: &SharedString,
        first: &SharedString,
        second: &SharedString,
        fx: f32,
        fy: f32,
    ) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let Ok(first_uuid) = Uuid::parse_str(first.as_str()) else {
            return;
        };
        let Ok(second_uuid) = Uuid::parse_str(second.as_str()) else {
            return;
        };
        let ratio = {
            let layouts = self.terminal_deps.layouts.lock();
            let Some(layout) = layouts.get(&inst) else {
                return;
            };
            let snap = layout.snapshot();
            let Some(tab) = snap.tabs.get(snap.active_tab) else {
                return;
            };
            let Some(div) = tab
                .dividers
                .iter()
                .find(|d| d.first_session == first_uuid && d.second_session == second_uuid)
            else {
                return;
            };
            match div.direction {
                SplitDirection::Horizontal => {
                    let pw = div.parent_bounds.right - div.parent_bounds.left;
                    if pw <= 0.0 {
                        return;
                    }
                    ((fx - div.parent_bounds.left) / pw).clamp(0.05, 0.95)
                }
                SplitDirection::Vertical => {
                    let ph = div.parent_bounds.bottom - div.parent_bounds.top;
                    if ph <= 0.0 {
                        return;
                    }
                    ((fy - div.parent_bounds.top) / ph).clamp(0.05, 0.95)
                }
            }
        };
        let deps = self.terminal_deps.clone();
        if let Err(e) = set_split_ratio(&deps, inst, first_uuid, second_uuid, ratio) {
            warn!(?e, %inst, %first_uuid, %second_uuid, "terminal split drag");
        }
        self.schedule_rebuild();
    }

    pub(super) fn on_terminal_shortcut(self: &Arc<Self>, id: &SharedString, action: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        let act = action.to_string();
        spawn::spawn_local_compat(async move {
            let result = match act.as_str() {
                "split-h" => split_horizontal(&deps, inst).await,
                "split-v" => split_vertical(&deps, inst).await,
                "tab-new" => add_tab(&deps, inst).await,
                "close" => close_focused_pane_or_tab(&deps, inst).await,
                "focus-next" => focus_next_pane(&deps, inst),
                "focus-prev" => focus_previous_pane(&deps, inst),
                "tab-next" => switch_tab_relative(&deps, inst, 1),
                "tab-prev" => switch_tab_relative(&deps, inst, -1),
                _ => Ok(()),
            };
            if let Err(e) = result {
                warn!(?e, %inst, action = %act, "terminal shortcut");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
}

/// Slint `KeyboardModifiers` → [`orchid_core::Modifiers`].
fn slint_kb_modifiers(ctrl: bool, shift: bool, alt: bool) -> orchid_core::Modifiers {
    use orchid_core::Modifiers;
    let mut mods = Modifiers::empty();
    if ctrl {
        mods |= Modifiers::CTRL;
    }
    if shift {
        mods |= Modifiers::SHIFT;
    }
    if alt {
        mods |= Modifiers::ALT;
    }
    mods
}

/// Peel Slint embedded modifier key identities (U+0010..=U+0013) from `text`.
fn peel_slint_modifier_prefix(
    text: &str,
    mut mods: orchid_core::Modifiers,
) -> (&str, orchid_core::Modifiers) {
    use orchid_core::Modifiers;
    let mut t = text;
    loop {
        let Some(c) = t.chars().next() else {
            break;
        };
        let embedded = match c as u32 {
            0x10 => Some(Modifiers::SHIFT),
            0x11 => Some(Modifiers::CTRL),
            0x12 | 0x13 => Some(Modifiers::ALT),
            _ => None,
        };
        if let Some(m) = embedded {
            mods |= m;
            t = &t[c.len_utf8()..];
        } else {
            break;
        }
    }
    (t, mods)
}

/// Map a Slint `KeyEvent.text` code point to [`orchid_core::Key`] (see `i-slint-common` `key_codes`).
fn slint_codepoint_to_key(cp: char) -> Option<orchid_core::Key> {
    use orchid_core::Key;
    match cp as u32 {
        0x08 => Some(Key::Backspace),
        0x09 => Some(Key::Tab),
        0x0A | 0x0D => Some(Key::Enter),
        0x1B => Some(Key::Escape),
        0x7F => Some(Key::Delete),
        0x20 => Some(Key::Space),
        0xF700 => Some(Key::ArrowUp),
        0xF701 => Some(Key::ArrowDown),
        0xF702 => Some(Key::ArrowLeft),
        0xF703 => Some(Key::ArrowRight),
        0xF704..=0xF71B => Some(Key::F((cp as u32 - 0xF704 + 1) as u8)),
        0xF727 => Some(Key::Insert),
        0xF729 => Some(Key::Home),
        0xF72B => Some(Key::End),
        0xF72C => Some(Key::PageUp),
        0xF72D => Some(Key::PageDown),
        _ => match cp {
            ',' => Some(Key::Comma),
            '.' => Some(Key::Period),
            '/' => Some(Key::Slash),
            '`' => Some(Key::Backtick),
            '-' => Some(Key::Minus),
            '=' => Some(Key::Equals),
            '[' => Some(Key::LeftBracket),
            ']' => Some(Key::RightBracket),
            ';' => Some(Key::Semicolon),
            '\'' => Some(Key::Quote),
            '\\' => Some(Key::Backslash),
            c if c.is_ascii_alphabetic() => Some(Key::Char(c.to_ascii_lowercase())),
            c if c.is_ascii_graphic() => Some(Key::Char(c)),
            _ => None,
        },
    }
}

/// Maps Slint `KeyEvent` (`text` + modifiers) to PTY bytes via [`orchid_terminal::InputEncoder`].
/// Falls back to [`encode_slint_key_text`] for multi-code-unit printable payloads.
fn encode_slint_key_event(
    text: &str,
    ctrl: bool,
    shift: bool,
    alt: bool,
    encoder: &orchid_terminal::InputEncoder,
) -> Vec<u8> {
    use orchid_core::{Key, Modifiers};

    let mods = slint_kb_modifiers(ctrl, shift, alt);
    let (peeled, mods) = peel_slint_modifier_prefix(text, mods);

    if peeled.is_empty() {
        trace!(
            target: "orchid_ui::terminal_input",
            "empty Slint key text after modifier peel (modifier-only or platform gap)"
        );
        return Vec::new();
    }

    // Slint Backtab identity (U+0019).
    if peeled == "\u{19}" {
        return encoder.encode_key(Key::Tab, mods | Modifiers::SHIFT);
    }

    // Pre-formed CSI / SS3 sequences from older Slint paths or platform quirks.
    if is_leading_escape_to_preserve(peeled) {
        return peeled.as_bytes().to_vec();
    }

    let trimmed = trim_slint_key_artifacts(peeled);
    if trimmed.is_empty() {
        if peeled.chars().count() == 1 {
            if let Some(key) = slint_codepoint_to_key(peeled.chars().next().expect("one char")) {
                return encoder.encode_key(key, mods);
            }
        }
        trace!(
            target: "orchid_ui::terminal_input",
            "key text was only Slint key-identity (PUA or modifier id); not forwarding to PTY"
        );
        return Vec::new();
    }

    if trimmed.chars().count() == 1 {
        let c = trimmed.chars().next().expect("one char");
        let cp = c as u32;
        if (0x10..=0x19).contains(&cp) {
            trace!(
                target: "orchid_ui::terminal_input",
                "Slint key id U+{:04X} only; not forwarding to PTY",
                cp
            );
            return Vec::new();
        }
        if let Some(key) = slint_codepoint_to_key(c) {
            return encoder.encode_key(key, mods);
        }
    }

    encode_slint_key_text(peeled)
}

/// True when `t` should not have its leading U+001B removed by [`trim_slint_key_artifacts`].
fn is_leading_escape_to_preserve(t: &str) -> bool {
    if !t.starts_with('\u{1b}') {
        return false;
    }
    t.chars().nth(1).is_some_and(|c| matches!(c, '[' | 'O'))
}

/// Strips Slint / winit key identity that is not user text (see `slint` `key_codes`):
/// - U+FEFF (BOM)
/// - Private use U+E000..=U+F8FF
/// - Slint modifier-style C0 U+0010..=U+0019 (incl. Backtab id 0x19)
/// - When 2+ code points: other C0 (U+00..=U+1F) except U+001B (we keep real ESC and
///   CSI/SS3 via [`is_leading_escape_to_preserve`]).
fn trim_slint_key_artifacts(text: &str) -> &str {
    let mut t = text;
    loop {
        if t.is_empty() {
            break;
        }
        if is_leading_escape_to_preserve(t) {
            break;
        }
        let Some(c) = t.chars().next() else {
            break;
        };
        let n = c as u32;
        if n == 0xFEFF {
            t = &t[c.len_utf8()..];
            continue;
        }
        if (0xE000..=0xF8FF).contains(&n) {
            t = &t[c.len_utf8()..];
            continue;
        }
        if t.chars().count() > 1 && (0x10..=0x19).contains(&n) {
            t = &t[c.len_utf8()..];
            continue;
        }
        if t.chars().count() > 1 && n < 0x20 && n != 0x1B {
            t = &t[c.len_utf8()..];
            continue;
        }
        break;
    }
    t
}

/// Maps Slint `KeyEvent.text` payloads to bytes for the PTY (printable / legacy paths).
fn encode_slint_key_text(text: &str) -> Vec<u8> {
    if text.is_empty() {
        return Vec::new();
    }
    if text == "\r\n" || text == "\n\r" {
        return vec![0x0D];
    }
    let t = trim_slint_key_artifacts(text);
    if t.is_empty() {
        trace!(
            target: "orchid_ui::terminal_input",
            "key text was only Slint key-identity (PUA or modifier id); not forwarding to PTY"
        );
        return Vec::new();
    }
    if t == "\r\n" || t == "\n\r" {
        return vec![0x0D];
    }
    let mut chars = t.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        let cp = c as u32;
        // Slint uses U+10..=U+19 (DC1..) as *modifier key identity* for Key.* wiring
        // when paired with a printable; alone they must not become raw C0 in the PTY
        // (DLE, DC1, ..), which would print as "extra" garbage before/after RU/EN.
        if (0x10..=0x19).contains(&cp) {
            trace!(
                target: "orchid_ui::terminal_input",
                "Slint key id U+{:04X} only; not forwarding to PTY",
                cp
            );
            return Vec::new();
        }
        match c {
            '\n' | '\r' => return vec![0x0D],
            '\u{8}' | '\u{7f}' => return vec![0x7F],
            '\t' => return vec![b'\t'],
            '\u{1b}' => return vec![0x1B],
            c if (c as u32) < 0x20 => return vec![c as u8],
            _ => {}
        }
    }
    t.as_bytes().to_vec()
}
#[cfg(test)]
mod key_encode_tests {
    use orchid_core::{Key, Modifiers};
    use orchid_terminal::InputEncoder;

    use super::{encode_slint_key_event, encode_slint_key_text};

    #[test]
    fn encodes_printable() {
        assert_eq!(&encode_slint_key_text("a"), b"a");
        assert_eq!(&encode_slint_key_text("hello"), b"hello");
    }

    #[test]
    fn strips_slint_pua_and_modifier_id_prefixes() {
        assert_eq!(&encode_slint_key_text("\u{F700}a"), b"a");
        assert_eq!(&encode_slint_key_text("\u{E000}Z"), b"Z");
        assert!(encode_slint_key_text("\u{F700}").is_empty());
        // Slint: Shift = U+0010; a stray prefix + '$' (Shift+4 on US layout) must
        // be a single 0x24, not 0x10, 0x24.
        assert_eq!(&encode_slint_key_text("\u{10}$"), b"$");
        assert_eq!(&encode_slint_key_text("\u{F700}\u{10}x"), b"x");
        // VT/FF/LF/CR and similar C0 + symbol (e.g. Shift+2/3 on some Winit paths)
        assert_eq!(&encode_slint_key_text("\u{0B}@"), b"@");
        assert_eq!(&encode_slint_key_text("\u{0A}#"), b"#");
        assert_eq!(&encode_slint_key_text("\u{FEFF}x"), b"x");
        // CSI/SS3 must stay intact
        assert_eq!(&encode_slint_key_text("\u{1b}[A"), b"\x1b[A");
        assert_eq!(&encode_slint_key_text("\u{1b}OP"), b"\x1bOP");
    }

    #[test]
    fn encodes_enter_as_cr() {
        assert_eq!(encode_slint_key_text("\n"), vec![0x0D]);
        assert_eq!(encode_slint_key_text("\r"), vec![0x0D]);
        assert_eq!(encode_slint_key_text("\r\n"), vec![0x0D]);
        assert_eq!(encode_slint_key_text("\n\r"), vec![0x0D]);
    }

    #[test]
    fn encodes_backspace_as_del() {
        assert_eq!(encode_slint_key_text("\u{8}"), vec![0x7F]);
        assert_eq!(encode_slint_key_text("\u{7f}"), vec![0x7F]);
    }

    #[test]
    fn encodes_tab() {
        assert_eq!(encode_slint_key_text("\t"), vec![b'\t']);
    }

    #[test]
    fn encodes_escape() {
        assert_eq!(encode_slint_key_text("\u{1b}"), vec![0x1B]);
    }

    #[test]
    fn empty_is_empty() {
        assert!(encode_slint_key_text("").is_empty());
    }

    #[test]
    fn slint_lone_modifier_id_sends_nothing() {
        // U+10..=U+19: Slint may emit these alone for modifier; never send as DLE/DC1/…
        for cp in 0x10u32..=0x19 {
            let c = char::from_u32(cp).expect("BMP C0");
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            assert!(
                encode_slint_key_text(s).is_empty(),
                "U+{cp:04X} should not be forwarded as raw C0"
            );
        }
    }

    #[test]
    fn utf8_passed_through() {
        assert_eq!(&encode_slint_key_text("ü"), "ü".as_bytes());
        assert_eq!(&encode_slint_key_text("日"), "日".as_bytes());
    }

    #[test]
    fn event_encoder_maps_lone_pua_arrow() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encode_slint_key_event("\u{F700}", false, false, false, &encoder),
            vec![0x1B, b'[', b'A']
        );
        assert_eq!(
            encode_slint_key_event("\u{F703}", false, false, false, &encoder),
            vec![0x1B, b'[', b'C']
        );
    }

    #[test]
    fn event_encoder_ctrl_c() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encode_slint_key_event("c", true, false, false, &encoder),
            vec![0x03]
        );
        assert_eq!(
            encode_slint_key_event("\u{11}c", false, false, false, &encoder),
            vec![0x03]
        );
    }

    #[test]
    fn event_encoder_f4_and_application_cursor() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encode_slint_key_event("\u{F707}", false, false, false, &encoder),
            vec![0x1B, b'O', b'S']
        );
        let mut app = InputEncoder::new();
        app.application_cursor = true;
        assert_eq!(
            encode_slint_key_event("\u{F700}", false, false, false, &app),
            vec![0x1B, b'O', b'A']
        );
    }

    #[test]
    fn event_encoder_shift_tab_and_backtab() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encode_slint_key_event("\t", false, true, false, &encoder),
            vec![0x1B, b'[', b'Z']
        );
        assert_eq!(
            encode_slint_key_event("\u{19}", false, false, false, &encoder),
            vec![0x1B, b'[', b'Z']
        );
    }

    #[test]
    fn event_encoder_preserves_csi_pass_through() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encode_slint_key_event("\u{1b}[A", false, false, false, &encoder),
            b"\x1b[A"
        );
    }

    #[test]
    fn event_encoder_printable_matches_text_path() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encode_slint_key_event("hello", false, false, false, &encoder),
            encode_slint_key_text("hello")
        );
        assert_eq!(
            encode_slint_key_event("a", false, false, false, &encoder),
            b"a"
        );
    }

    #[test]
    fn event_encoder_named_keys_via_input_encoder() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encoder.encode_key(Key::Enter, Modifiers::empty()),
            encode_slint_key_event("\n", false, false, false, &encoder)
        );
        assert_eq!(
            encoder.encode_key(Key::Backspace, Modifiers::empty()),
            encode_slint_key_event("\u{8}", false, false, false, &encoder)
        );
    }
}

