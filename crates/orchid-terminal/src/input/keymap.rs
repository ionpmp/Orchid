//! Keyboard → PTY byte encoder.
//!
//! Implements the subset of xterm-style sequences we need. Application
//! cursor / keypad modes flip between `ESC [` and `ESC O` prefixes for the
//! arrow / function families.

use orchid_core::{Key, Modifiers};

use crate::error::Result;
use crate::input::paste::{sanitise_paste, BP_END, BP_START};

/// State mirroring DECCKM / DECKPAM / bracketed-paste / mouse mode.
#[derive(Debug, Clone, Copy)]
pub struct InputEncoder {
    /// DECCKM: arrow keys emit `ESC O …` instead of `ESC [ …`.
    pub application_cursor: bool,
    /// DECKPAM: keypad keys emit xterm-style sequences.
    pub application_keypad: bool,
    /// Whether the application has requested bracketed-paste mode.
    pub bracketed_paste: bool,
    /// Mouse-reporting mode.
    pub mouse_mode: MouseMode,
    /// Whether UTF-8 input is honoured as-is (always true for Orchid).
    pub utf8_mode: bool,
}

impl Default for InputEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Mouse reporting variants.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseMode {
    Off,
    Normal,
    Sgr,
    SgrPixels,
}

/// Which button an encoded mouse event reports.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButtonReport {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

/// Press / release / move, for mouse encoding.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Move,
}

impl InputEncoder {
    /// Default encoder: DECCKM off, no mouse reporting, bracketed-paste off.
    #[must_use]
    pub fn new() -> Self {
        Self {
            application_cursor: false,
            application_keypad: false,
            bracketed_paste: false,
            mouse_mode: MouseMode::Off,
            utf8_mode: true,
        }
    }

    /// Convert a key + modifier combo into the byte sequence a shell expects.
    /// Returns empty when the key should not emit anything (e.g. modifier
    /// keys pressed alone — Orchid never hands those to us anyway).
    #[must_use]
    pub fn encode_key(&self, key: Key, modifiers: Modifiers) -> Vec<u8> {
        use Key::*;
        let ctrl = modifiers.contains(Modifiers::CTRL);
        let alt = modifiers.contains(Modifiers::ALT);
        let shift = modifiers.contains(Modifiers::SHIFT);

        // Ctrl+letter / Ctrl+symbol table comes first — the canonical 0x00..0x1f range.
        if ctrl {
            if let Char(c) = key {
                if let Some(b) = ctrl_byte(c) {
                    // Ctrl+Shift+<x> falls back to the same byte today;
                    // applications that want more differentiation use the
                    // CSI-u encoding which isn't implemented yet.
                    let _ = shift;
                    let out = if alt { vec![0x1B, b] } else { vec![b] };
                    return out;
                }
            }
        }

        match key {
            Char(c) => {
                if alt {
                    let mut buf = Vec::with_capacity(5);
                    buf.push(0x1B);
                    push_char(&mut buf, if shift { c.to_ascii_uppercase() } else { c });
                    buf
                } else {
                    let mut buf = Vec::with_capacity(4);
                    push_char(&mut buf, if shift { c.to_ascii_uppercase() } else { c });
                    buf
                }
            }
            Enter => vec![b'\r'],
            Escape => vec![0x1B],
            Tab => {
                if shift {
                    vec![0x1B, b'[', b'Z']
                } else {
                    vec![b'\t']
                }
            }
            Backspace => vec![0x7F],
            Delete => csi_tilde(3, modifiers),
            Insert => csi_tilde(2, modifiers),
            Home => cursor_sequence(self.application_cursor, b'H', modifiers),
            End => cursor_sequence(self.application_cursor, b'F', modifiers),
            PageUp => csi_tilde(5, modifiers),
            PageDown => csi_tilde(6, modifiers),
            ArrowUp => cursor_sequence(self.application_cursor, b'A', modifiers),
            ArrowDown => cursor_sequence(self.application_cursor, b'B', modifiers),
            ArrowLeft => cursor_sequence(self.application_cursor, b'D', modifiers),
            ArrowRight => cursor_sequence(self.application_cursor, b'C', modifiers),
            Space => vec![b' '],
            Comma => vec![b','],
            Period => vec![b'.'],
            Slash => vec![b'/'],
            Backtick => vec![b'`'],
            Minus => vec![b'-'],
            Equals => vec![b'='],
            LeftBracket => vec![b'['],
            RightBracket => vec![b']'],
            Semicolon => vec![b';'],
            Quote => vec![b'\''],
            Backslash => vec![b'\\'],
            F(n) => function_key_sequence(n, modifiers),
        }
    }

    /// Encode a standalone UTF-8 character.
    #[must_use]
    pub fn encode_char(&self, c: char) -> Vec<u8> {
        let mut buf = Vec::with_capacity(4);
        push_char(&mut buf, c);
        buf
    }

    /// Encode a paste payload. When `bracketed_paste` is on, brackets the
    /// payload with `ESC [ 200 ~` / `ESC [ 201 ~`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::TerminalError::PasteRejected`] when the payload
    /// looks like an injection attempt (see [`crate::input::paste`]).
    pub fn encode_paste(&self, text: &str) -> Result<Vec<u8>> {
        let normalised = sanitise_paste(text)?;
        if self.bracketed_paste {
            let mut out = Vec::with_capacity(normalised.len() + BP_START.len() + BP_END.len());
            out.extend_from_slice(BP_START);
            out.extend_from_slice(normalised.as_bytes());
            out.extend_from_slice(BP_END);
            Ok(out)
        } else {
            Ok(normalised.into_bytes())
        }
    }

    /// Encode a mouse event per the current [`MouseMode`]. Returns empty when
    /// mouse mode is off.
    #[must_use]
    pub fn encode_mouse(
        &self,
        col: u16,
        row: u16,
        button: MouseButtonReport,
        action: MouseAction,
        modifiers: Modifiers,
    ) -> Vec<u8> {
        let mut code = button_code(button);
        if modifiers.contains(Modifiers::SHIFT) {
            code |= 0x04;
        }
        if modifiers.contains(Modifiers::ALT) {
            code |= 0x08;
        }
        if modifiers.contains(Modifiers::CTRL) {
            code |= 0x10;
        }
        match self.mouse_mode {
            MouseMode::Off => Vec::new(),
            MouseMode::Sgr | MouseMode::SgrPixels => {
                let final_byte = match action {
                    MouseAction::Press | MouseAction::Move => b'M',
                    MouseAction::Release => b'm',
                };
                format!("\x1b[<{code};{};{}{}", col + 1, row + 1, final_byte as char)
                    .into_bytes()
            }
            MouseMode::Normal => {
                // Legacy X10 — cap coordinates at 223 (94 + 128).
                let cb = 32 + code as u16;
                let cx = 32 + (col + 1).min(223);
                let cy = 32 + (row + 1).min(223);
                vec![0x1B, b'[', b'M', cb as u8, cx as u8, cy as u8]
            }
        }
    }
}

fn ctrl_byte(c: char) -> Option<u8> {
    // Map printable ASCII to its Ctrl counterpart (xterm canonical).
    match c {
        '@' | ' ' => Some(0x00),
        'a'..='z' => Some((c as u8) - b'a' + 1),
        'A'..='Z' => Some((c as u8) - b'A' + 1),
        '[' => Some(0x1B),
        '\\' => Some(0x1C),
        ']' => Some(0x1D),
        '^' => Some(0x1E),
        '_' => Some(0x1F),
        '?' => Some(0x7F),
        _ => None,
    }
}

fn push_char(buf: &mut Vec<u8>, c: char) {
    let mut tmp = [0u8; 4];
    let s = c.encode_utf8(&mut tmp);
    buf.extend_from_slice(s.as_bytes());
}

fn cursor_sequence(application: bool, final_byte: u8, modifiers: Modifiers) -> Vec<u8> {
    let prefix: &[u8] = if application {
        b"\x1bO"
    } else {
        b"\x1b["
    };
    let mod_code = modifier_code(modifiers);
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(prefix);
    if mod_code != 1 {
        out.extend_from_slice(format!("1;{mod_code}").as_bytes());
    }
    out.push(final_byte);
    out
}

fn csi_tilde(code: u16, modifiers: Modifiers) -> Vec<u8> {
    let mod_code = modifier_code(modifiers);
    if mod_code == 1 {
        format!("\x1b[{code}~").into_bytes()
    } else {
        format!("\x1b[{code};{mod_code}~").into_bytes()
    }
}

fn function_key_sequence(n: u8, modifiers: Modifiers) -> Vec<u8> {
    // Canonical xterm codes: F1..F4 => ESC O P/Q/R/S; F5.. => CSI tilde.
    let mod_code = modifier_code(modifiers);
    match n {
        1 => mod_prefixed(b'P', mod_code),
        2 => mod_prefixed(b'Q', mod_code),
        3 => mod_prefixed(b'R', mod_code),
        4 => mod_prefixed(b'S', mod_code),
        5 => csi_tilde(15, modifiers),
        6 => csi_tilde(17, modifiers),
        7 => csi_tilde(18, modifiers),
        8 => csi_tilde(19, modifiers),
        9 => csi_tilde(20, modifiers),
        10 => csi_tilde(21, modifiers),
        11 => csi_tilde(23, modifiers),
        12 => csi_tilde(24, modifiers),
        _ => Vec::new(),
    }
}

fn mod_prefixed(final_byte: u8, mod_code: u16) -> Vec<u8> {
    if mod_code == 1 {
        vec![0x1B, b'O', final_byte]
    } else {
        format!("\x1b[1;{mod_code}{}", final_byte as char).into_bytes()
    }
}

fn modifier_code(modifiers: Modifiers) -> u16 {
    // xterm's `1;X` code table: 1 none, 2 shift, 3 alt, 4 shift+alt,
    // 5 ctrl, 6 shift+ctrl, 7 alt+ctrl, 8 shift+alt+ctrl.
    let mut code: u16 = 1;
    if modifiers.contains(Modifiers::SHIFT) {
        code += 1;
    }
    if modifiers.contains(Modifiers::ALT) {
        code += 2;
    }
    if modifiers.contains(Modifiers::CTRL) {
        code += 4;
    }
    code
}

fn button_code(b: MouseButtonReport) -> u8 {
    match b {
        MouseButtonReport::Left => 0,
        MouseButtonReport::Middle => 1,
        MouseButtonReport::Right => 2,
        MouseButtonReport::WheelUp => 64,
        MouseButtonReport::WheelDown => 65,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_core::{Key, Modifiers};

    #[test]
    fn ctrl_c_is_etx() {
        let e = InputEncoder::new();
        let out = e.encode_key(Key::Char('c'), Modifiers::CTRL);
        assert_eq!(out, vec![0x03]);
    }

    #[test]
    fn up_arrow_normal_vs_application() {
        let mut e = InputEncoder::new();
        assert_eq!(
            e.encode_key(Key::ArrowUp, Modifiers::empty()),
            vec![0x1B, b'[', b'A']
        );
        e.application_cursor = true;
        assert_eq!(
            e.encode_key(Key::ArrowUp, Modifiers::empty()),
            vec![0x1B, b'O', b'A']
        );
    }

    #[test]
    fn alt_letter_prefixes_escape() {
        let e = InputEncoder::new();
        let out = e.encode_key(Key::Char('x'), Modifiers::ALT);
        assert_eq!(out, vec![0x1B, b'x']);
    }

    #[test]
    fn enter_and_backspace() {
        let e = InputEncoder::new();
        assert_eq!(
            e.encode_key(Key::Enter, Modifiers::empty()),
            vec![b'\r']
        );
        assert_eq!(
            e.encode_key(Key::Backspace, Modifiers::empty()),
            vec![0x7F]
        );
    }

    #[test]
    fn shift_tab_emits_backtab() {
        let e = InputEncoder::new();
        let out = e.encode_key(Key::Tab, Modifiers::SHIFT);
        assert_eq!(out, vec![0x1B, b'[', b'Z']);
    }

    #[test]
    fn paste_brackets_when_enabled() {
        let mut e = InputEncoder::new();
        e.bracketed_paste = true;
        let out = e.encode_paste("hello").unwrap();
        assert!(out.starts_with(BP_START));
        assert!(out.ends_with(BP_END));
    }

    #[test]
    fn paste_rejects_injection() {
        let e = InputEncoder::new();
        let err = e.encode_paste("a\x1b[201~b").unwrap_err();
        assert!(matches!(err, crate::TerminalError::PasteRejected));
    }

    #[test]
    fn f4_emits_esco_s() {
        let e = InputEncoder::new();
        let out = e.encode_key(Key::F(4), Modifiers::empty());
        assert_eq!(out, vec![0x1B, b'O', b'S']);
    }
}
