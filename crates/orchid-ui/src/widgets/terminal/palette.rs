//! Build an [`orchid_terminal::TerminalPalette`] from application theme tokens.
//!
//! For MVP we ship two hand-curated palettes — Orchid Dark and Orchid Light —
//! and expose a [`ThemeFlavor`] selector. The fully-tokenised path where the
//! user's theme TOML contributes the sixteen ANSI colours is scheduled for
//! v1.x alongside the theme-authoring story.

use orchid_terminal::{Rgba, TerminalPalette};

/// Which themed palette to apply to a terminal view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeFlavor {
    /// Orchid Dark (default).
    Dark,
    /// Orchid Light.
    Light,
}

/// Build a [`TerminalPalette`] from a theme flavour.
#[must_use]
pub fn palette_from_flavor(flavor: ThemeFlavor) -> TerminalPalette {
    match flavor {
        ThemeFlavor::Dark => orchid_dark_palette(),
        ThemeFlavor::Light => orchid_light_palette(),
    }
}

fn orchid_dark_palette() -> TerminalPalette {
    // This matches the defaults in `orchid_terminal::TerminalPalette::default_dark`
    // but is defined here so the UI layer owns the source-of-truth colour
    // tokens instead of depending on the terminal crate's test defaults.
    TerminalPalette {
        default_fg: Rgba::rgb(0xE6, 0xEB, 0xF0),
        default_bg: Rgba::rgb(0x12, 0x14, 0x18),
        cursor: Rgba::rgb(0xC3, 0x7D, 0xDD),
        selection_bg: Rgba::rgb(0x36, 0x3C, 0x46),
        selection_fg: Rgba::rgb(0xFF, 0xFF, 0xFF),
        ansi: [
            Rgba::rgb(0x1D, 0x20, 0x26),
            Rgba::rgb(0xE0, 0x6C, 0x75),
            Rgba::rgb(0x98, 0xC3, 0x79),
            Rgba::rgb(0xE5, 0xC0, 0x7B),
            Rgba::rgb(0x61, 0xAF, 0xEF),
            Rgba::rgb(0xC3, 0x7D, 0xDD),
            Rgba::rgb(0x56, 0xB6, 0xC2),
            Rgba::rgb(0xAB, 0xB2, 0xBF),
            Rgba::rgb(0x5C, 0x63, 0x70),
            Rgba::rgb(0xF0, 0x83, 0x8E),
            Rgba::rgb(0xAD, 0xDB, 0x8F),
            Rgba::rgb(0xF7, 0xD0, 0x8F),
            Rgba::rgb(0x79, 0xC2, 0xF9),
            Rgba::rgb(0xD6, 0x97, 0xEE),
            Rgba::rgb(0x79, 0xC7, 0xD2),
            Rgba::rgb(0xE6, 0xEB, 0xF0),
        ],
    }
}

fn orchid_light_palette() -> TerminalPalette {
    TerminalPalette {
        default_fg: Rgba::rgb(0x1F, 0x24, 0x2E),
        default_bg: Rgba::rgb(0xF7, 0xF8, 0xFA),
        cursor: Rgba::rgb(0x8E, 0x4F, 0xC2),
        selection_bg: Rgba::rgb(0xD8, 0xDC, 0xE2),
        selection_fg: Rgba::rgb(0x1F, 0x24, 0x2E),
        ansi: [
            Rgba::rgb(0x2C, 0x31, 0x3C),
            Rgba::rgb(0xC7, 0x3E, 0x49),
            Rgba::rgb(0x64, 0x94, 0x40),
            Rgba::rgb(0xB5, 0x89, 0x00),
            Rgba::rgb(0x21, 0x75, 0xC4),
            Rgba::rgb(0x8E, 0x4F, 0xC2),
            Rgba::rgb(0x24, 0x8B, 0x99),
            Rgba::rgb(0x55, 0x5D, 0x6C),
            Rgba::rgb(0x83, 0x8A, 0x98),
            Rgba::rgb(0xE8, 0x50, 0x5D),
            Rgba::rgb(0x7A, 0xBB, 0x54),
            Rgba::rgb(0xD1, 0xA1, 0x1A),
            Rgba::rgb(0x38, 0x8C, 0xE6),
            Rgba::rgb(0xAC, 0x6B, 0xE1),
            Rgba::rgb(0x30, 0xA3, 0xB4),
            Rgba::rgb(0x1F, 0x24, 0x2E),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_palettes_expose_16_ansi_entries() {
        assert_eq!(palette_from_flavor(ThemeFlavor::Dark).ansi.len(), 16);
        assert_eq!(palette_from_flavor(ThemeFlavor::Light).ansi.len(), 16);
    }

    #[test]
    fn dark_and_light_differ() {
        let d = palette_from_flavor(ThemeFlavor::Dark);
        let l = palette_from_flavor(ThemeFlavor::Light);
        assert_ne!(d.default_bg, l.default_bg);
    }
}
