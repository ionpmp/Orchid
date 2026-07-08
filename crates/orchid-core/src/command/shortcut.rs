//! Keyboard shortcuts: [`Shortcut`], [`Modifiers`], [`Key`], and parsing.
//!
//! ## Reserved shortcuts
//!
//! Some key combinations are reserved by Windows or by Orchid's own
//! conflict-avoidance policy and cannot (or should not) be bound to
//! application commands. [`is_reserved`] returns a human-readable reason
//! when a shortcut falls into this set:
//!
//! * **`Win+L`** — system lock, not overridable.
//! * **`Win+Space`** — system IME switch on multilingual setups.
//! * **`Ctrl+Alt+<letter>`** — collides with `AltGr + <letter>` on
//!   European keyboard layouts.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

bitflags! {
    /// Bitfield of active modifier keys in a [`Shortcut`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct Modifiers: u8 {
        /// The <kbd>Ctrl</kbd> key.
        const CTRL  = 1 << 0;
        /// The <kbd>Alt</kbd> key.
        const ALT   = 1 << 1;
        /// The <kbd>Shift</kbd> key.
        const SHIFT = 1 << 2;
        /// The <kbd>Win</kbd> / Super / Meta key.
        const WIN   = 1 << 3;
    }
}

/// A single keyboard key in its canonical, layout-independent form.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    /// Printable character. Letters are stored lowercase.
    Char(char),
    /// Function key `F1`..`F24`.
    F(u8),
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Space,
    Comma,
    Period,
    Slash,
    Backtick,
    Minus,
    Equals,
    LeftBracket,
    RightBracket,
    Semicolon,
    Quote,
    Backslash,
}

/// A keyboard shortcut: zero or more modifiers plus exactly one [`Key`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shortcut {
    /// Set of active modifier keys.
    pub modifiers: Modifiers,
    /// Main key.
    pub key: Key,
}

impl Shortcut {
    /// Build a shortcut from its parts.
    #[must_use]
    pub fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }

    /// Parse a human-friendly shortcut string.
    ///
    /// Accepts any mix of `+` separators, optional whitespace, and
    /// case-insensitive modifiers: `"Ctrl+Shift+P"`, `"ctrl + shift + p"`
    /// and `"CTRL+SHIFT+P"` all produce the same [`Shortcut`].
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidShortcut`] if the string is empty, has no
    /// main key, references an unknown key name, or names the `AltGr`
    /// modifier (which is intentionally unsupported on Windows — use
    /// explicit `Ctrl+Alt` instead).
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::{Key, Modifiers, Shortcut};
    /// let s = Shortcut::parse("Ctrl+Shift+P").unwrap();
    /// assert_eq!(s.modifiers, Modifiers::CTRL | Modifiers::SHIFT);
    /// assert_eq!(s.key, Key::Char('p'));
    /// ```
    pub fn parse(s: &str) -> Result<Self> {
        if s.trim().is_empty() {
            return Err(CoreError::InvalidShortcut { input: s.into() });
        }

        let mut modifiers = Modifiers::empty();
        let mut key: Option<Key> = None;

        for raw in s.split('+') {
            let token = raw.trim();
            if token.is_empty() {
                return Err(CoreError::InvalidShortcut { input: s.into() });
            }
            let lower = token.to_ascii_lowercase();
            match lower.as_str() {
                "ctrl" | "control" => modifiers |= Modifiers::CTRL,
                "alt" | "option" => modifiers |= Modifiers::ALT,
                "shift" => modifiers |= Modifiers::SHIFT,
                "win" | "super" | "meta" | "cmd" | "command" => modifiers |= Modifiers::WIN,
                "altgr" | "alt-gr" => {
                    return Err(CoreError::InvalidShortcut {
                        input: format!(
                            "{s}: AltGr is not a supported modifier; use explicit Ctrl+Alt if intended"
                        ),
                    });
                }
                _ => {
                    if key.is_some() {
                        return Err(CoreError::InvalidShortcut {
                            input: format!("{s}: more than one main key"),
                        });
                    }
                    key = Some(parse_key(token).ok_or_else(|| CoreError::InvalidShortcut {
                        input: format!("{s}: unknown key `{token}`"),
                    })?);
                }
            }
        }

        let key = key.ok_or_else(|| CoreError::InvalidShortcut {
            input: format!("{s}: missing main key"),
        })?;
        Ok(Self { modifiers, key })
    }

    /// Canonical string form: modifiers in fixed order (Ctrl, Alt, Shift,
    /// Win), then the main key.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::Shortcut;
    /// let s = Shortcut::parse("shift + ctrl + p").unwrap();
    /// assert_eq!(s.to_string_canonical(), "Ctrl+Shift+P");
    /// ```
    #[must_use]
    pub fn to_string_canonical(&self) -> String {
        let mut parts = Vec::with_capacity(5);
        if self.modifiers.contains(Modifiers::CTRL) {
            parts.push("Ctrl".to_string());
        }
        if self.modifiers.contains(Modifiers::ALT) {
            parts.push("Alt".to_string());
        }
        if self.modifiers.contains(Modifiers::SHIFT) {
            parts.push("Shift".to_string());
        }
        if self.modifiers.contains(Modifiers::WIN) {
            parts.push("Win".to_string());
        }
        parts.push(key_to_canonical(self.key));
        parts.join("+")
    }

    /// Two shortcuts conflict when they share the exact same modifiers and
    /// key.
    #[must_use]
    pub fn is_conflict_with(&self, other: &Self) -> bool {
        self == other
    }
}

impl std::fmt::Display for Shortcut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_string_canonical())
    }
}

/// Reason the shortcut is reserved, or `None` if it is free for user binding.
///
/// # Examples
///
/// ```
/// use orchid_core::{is_reserved, Shortcut};
/// let s = Shortcut::parse("Win+L").unwrap();
/// assert!(is_reserved(&s).is_some());
/// ```
#[must_use]
pub fn is_reserved(s: &Shortcut) -> Option<&'static str> {
    // Win+L: system lock
    if s.modifiers == Modifiers::WIN && matches!(s.key, Key::Char('l')) {
        return Some("Win+L is reserved by Windows for the lock screen");
    }
    // Win+Space: system IME switch on multilingual installs
    if s.modifiers == Modifiers::WIN && matches!(s.key, Key::Space) {
        return Some("Win+Space is reserved by Windows for the IME switch");
    }
    // Ctrl+Alt+<letter>: collides with AltGr on European layouts
    if s.modifiers == (Modifiers::CTRL | Modifiers::ALT) {
        if let Key::Char(c) = s.key {
            if c.is_ascii_alphabetic() {
                return Some(
                    "Ctrl+Alt+<letter> collides with AltGr on European keyboard layouts",
                );
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Key parsing helpers
// ---------------------------------------------------------------------------

fn parse_key(token: &str) -> Option<Key> {
    let lower = token.to_ascii_lowercase();
    let named = match lower.as_str() {
        "enter" | "return" => Some(Key::Enter),
        "esc" | "escape" => Some(Key::Escape),
        "tab" => Some(Key::Tab),
        "backspace" | "bs" => Some(Key::Backspace),
        "del" | "delete" => Some(Key::Delete),
        "ins" | "insert" => Some(Key::Insert),
        "home" => Some(Key::Home),
        "end" => Some(Key::End),
        "pgup" | "pageup" | "page-up" => Some(Key::PageUp),
        "pgdn" | "pagedown" | "page-down" => Some(Key::PageDown),
        "up" | "arrowup" | "arrow-up" => Some(Key::ArrowUp),
        "down" | "arrowdown" | "arrow-down" => Some(Key::ArrowDown),
        "left" | "arrowleft" | "arrow-left" => Some(Key::ArrowLeft),
        "right" | "arrowright" | "arrow-right" => Some(Key::ArrowRight),
        "space" => Some(Key::Space),
        "," | "comma" => Some(Key::Comma),
        "." | "period" | "dot" => Some(Key::Period),
        "/" | "slash" => Some(Key::Slash),
        "`" | "backtick" | "grave" => Some(Key::Backtick),
        "-" | "minus" | "dash" => Some(Key::Minus),
        "=" | "equals" | "equal" => Some(Key::Equals),
        "[" | "leftbracket" | "left-bracket" => Some(Key::LeftBracket),
        "]" | "rightbracket" | "right-bracket" => Some(Key::RightBracket),
        ";" | "semicolon" => Some(Key::Semicolon),
        "'" | "quote" | "apostrophe" => Some(Key::Quote),
        "\\" | "backslash" => Some(Key::Backslash),
        "?" => Some(Key::Char('?')),
        _ => None,
    };
    if named.is_some() {
        return named;
    }

    // Function keys: "f1".."f24"
    if let Some(rest) = lower.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u8>() {
            if (1..=24).contains(&n) {
                return Some(Key::F(n));
            }
        }
    }

    // Single printable character
    let mut chars = token.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphabetic() {
            return Some(Key::Char(c.to_ascii_lowercase()));
        }
        if c.is_ascii_digit() {
            return Some(Key::Char(c));
        }
    }

    None
}

fn key_to_canonical(key: Key) -> String {
    match key {
        Key::Char(c) => c.to_ascii_uppercase().to_string(),
        Key::F(n) => format!("F{n}"),
        Key::Enter => "Enter".into(),
        Key::Escape => "Escape".into(),
        Key::Tab => "Tab".into(),
        Key::Backspace => "Backspace".into(),
        Key::Delete => "Delete".into(),
        Key::Insert => "Insert".into(),
        Key::Home => "Home".into(),
        Key::End => "End".into(),
        Key::PageUp => "PageUp".into(),
        Key::PageDown => "PageDown".into(),
        Key::ArrowUp => "ArrowUp".into(),
        Key::ArrowDown => "ArrowDown".into(),
        Key::ArrowLeft => "ArrowLeft".into(),
        Key::ArrowRight => "ArrowRight".into(),
        Key::Space => "Space".into(),
        Key::Comma => ",".into(),
        Key::Period => ".".into(),
        Key::Slash => "/".into(),
        Key::Backtick => "`".into(),
        Key::Minus => "-".into(),
        Key::Equals => "=".into(),
        Key::LeftBracket => "[".into(),
        Key::RightBracket => "]".into(),
        Key::Semicolon => ";".into(),
        Key::Quote => "'".into(),
        Key::Backslash => "\\".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_canonical_roundtrip() {
        let cases = [
            ("Ctrl+Shift+P", "Ctrl+Shift+P"),
            ("ctrl+shift+p", "Ctrl+Shift+P"),
            ("Alt+F4", "Alt+F4"),
            ("Win+Space", "Win+Space"),
            ("Win+?", "Win+?"),
            ("Ctrl+,", "Ctrl+,"),
            ("Ctrl+`", "Ctrl+`"),
            ("Shift+Tab", "Shift+Tab"),
            ("Enter", "Enter"),
        ];
        for (input, expected) in cases {
            let s = Shortcut::parse(input).unwrap();
            assert_eq!(s.to_string_canonical(), expected, "input={input}");
        }
    }

    #[test]
    fn parse_rejects_altgr() {
        let err = Shortcut::parse("AltGr+T").unwrap_err();
        match err {
            CoreError::InvalidShortcut { input } => assert!(input.contains("AltGr")),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_empty_and_modifier_only() {
        assert!(Shortcut::parse("").is_err());
        assert!(Shortcut::parse("Ctrl+").is_err());
        assert!(Shortcut::parse("+Ctrl+A").is_err());
        assert!(Shortcut::parse("Ctrl").is_err());
    }

    #[test]
    fn parse_rejects_two_main_keys() {
        assert!(Shortcut::parse("Ctrl+A+B").is_err());
    }

    #[test]
    fn reserved_detects_win_l_and_win_space() {
        assert!(is_reserved(&Shortcut::parse("Win+L").unwrap()).is_some());
        assert!(is_reserved(&Shortcut::parse("Win+Space").unwrap()).is_some());
    }

    #[test]
    fn reserved_detects_ctrl_alt_letter() {
        assert!(is_reserved(&Shortcut::parse("Ctrl+Alt+T").unwrap()).is_some());
        // digits / symbols are not reserved
        assert!(is_reserved(&Shortcut::parse("Ctrl+Alt+5").unwrap()).is_none());
    }

    #[test]
    fn conflict_detects_exact_match() {
        let a = Shortcut::parse("Ctrl+P").unwrap();
        let b = Shortcut::parse("Ctrl+P").unwrap();
        let c = Shortcut::parse("Ctrl+Shift+P").unwrap();
        assert!(a.is_conflict_with(&b));
        assert!(!a.is_conflict_with(&c));
    }
}
