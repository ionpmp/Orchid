//! ANSI colour → RGBA resolution.

/// 8-bit RGBA. `a = 0xFF` means fully opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha.
    pub a: u8,
}

impl Rgba {
    /// Opaque colour from 24-bit RGB.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xFF }
    }

    /// Fully transparent.
    #[must_use]
    pub const fn transparent() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
    }
}

/// Which role a [`CellColor`] plays — foreground or background. Used by
/// [`resolve_color`] to fall back to the theme's default colour for the
/// role when the cell carries `CellColor::Default`.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorRole {
    Foreground,
    Background,
}

/// Typed cell colour: default (fill from theme), 8-bit palette index, or
/// explicit RGB.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellColor {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// Colour palette sourced from the active Orchid theme.
#[derive(Debug, Clone)]
pub struct TerminalPalette {
    /// Default foreground.
    pub default_fg: Rgba,
    /// Default background.
    pub default_bg: Rgba,
    /// Cursor fill colour.
    pub cursor: Rgba,
    /// Selection background.
    pub selection_bg: Rgba,
    /// Selection foreground.
    pub selection_fg: Rgba,
    /// Sixteen classic ANSI colours (0-7 normal, 8-15 bright).
    pub ansi: [Rgba; 16],
}

impl TerminalPalette {
    /// A reasonable dark-background default. The UI layer should supply a
    /// theme-specific palette instead — this one is here for tests and
    /// doctests.
    #[must_use]
    pub fn default_dark() -> Self {
        Self {
            default_fg: Rgba::rgb(0xD0, 0xD0, 0xD0),
            default_bg: Rgba::rgb(0x12, 0x14, 0x18),
            cursor: Rgba::rgb(0xC3, 0x7D, 0xDD),
            selection_bg: Rgba::rgb(0x36, 0x3C, 0x46),
            selection_fg: Rgba::rgb(0xFF, 0xFF, 0xFF),
            ansi: [
                Rgba::rgb(0x1D, 0x20, 0x26), // 0 black
                Rgba::rgb(0xE0, 0x6C, 0x75), // 1 red
                Rgba::rgb(0x98, 0xC3, 0x79), // 2 green
                Rgba::rgb(0xE5, 0xC0, 0x7B), // 3 yellow
                Rgba::rgb(0x61, 0xAF, 0xEF), // 4 blue
                Rgba::rgb(0xC3, 0x7D, 0xDD), // 5 magenta
                Rgba::rgb(0x56, 0xB6, 0xC2), // 6 cyan
                Rgba::rgb(0xAB, 0xB2, 0xBF), // 7 white
                Rgba::rgb(0x5C, 0x63, 0x70), // 8 bright black
                Rgba::rgb(0xF0, 0x83, 0x8E), // 9 bright red
                Rgba::rgb(0xAD, 0xDB, 0x8F), // 10 bright green
                Rgba::rgb(0xF7, 0xD0, 0x8F), // 11 bright yellow
                Rgba::rgb(0x79, 0xC2, 0xF9), // 12 bright blue
                Rgba::rgb(0xD6, 0x97, 0xEE), // 13 bright magenta
                Rgba::rgb(0x79, 0xC7, 0xD2), // 14 bright cyan
                Rgba::rgb(0xE6, 0xEB, 0xF0), // 15 bright white
            ],
        }
    }
}

/// Map a [`CellColor`] + [`ColorRole`] to an actual [`Rgba`] given the theme
/// palette.
///
/// # Examples
///
/// ```
/// use orchid_terminal::{resolve_color, CellColor, ColorRole, TerminalPalette};
/// let palette = TerminalPalette::default_dark();
/// let red = resolve_color(CellColor::Indexed(1), &palette, ColorRole::Foreground);
/// assert_eq!(red, palette.ansi[1]);
/// ```
#[must_use]
pub fn resolve_color(c: CellColor, palette: &TerminalPalette, role: ColorRole) -> Rgba {
    match c {
        CellColor::Default => match role {
            ColorRole::Foreground => palette.default_fg,
            ColorRole::Background => palette.default_bg,
        },
        CellColor::Indexed(idx) => {
            if (idx as usize) < palette.ansi.len() {
                palette.ansi[idx as usize]
            } else {
                xterm_256_color(idx)
            }
        }
        CellColor::Rgb(r, g, b) => Rgba::rgb(r, g, b),
    }
}

/// Convert a xterm 256-colour palette index to `Rgba`.
///
/// * 0..=15 are ANSI base colours (from a fixed xterm default palette).
/// * 16..=231 form the 6×6×6 RGB cube.
/// * 232..=255 is the grayscale ramp.
#[must_use]
pub fn xterm_256_color(idx: u8) -> Rgba {
    if idx < 16 {
        return XTERM_BASE_16[idx as usize];
    }
    if idx < 232 {
        let i = idx as u16 - 16;
        let r = i / 36;
        let g = (i / 6) % 6;
        let b = i % 6;
        return Rgba::rgb(
            cube_level(r as u8),
            cube_level(g as u8),
            cube_level(b as u8),
        );
    }
    let shade = 8 + (idx - 232) * 10;
    Rgba::rgb(shade, shade, shade)
}

fn cube_level(level: u8) -> u8 {
    // 0 -> 0, 1..=5 -> 55 + n * 40 (xterm canonical values)
    match level {
        0 => 0,
        n => 55 + n * 40,
    }
}

/// xterm 0..16 palette — kept consistent with common terminal emulators.
const XTERM_BASE_16: [Rgba; 16] = [
    Rgba::rgb(0x00, 0x00, 0x00),
    Rgba::rgb(0x80, 0x00, 0x00),
    Rgba::rgb(0x00, 0x80, 0x00),
    Rgba::rgb(0x80, 0x80, 0x00),
    Rgba::rgb(0x00, 0x00, 0x80),
    Rgba::rgb(0x80, 0x00, 0x80),
    Rgba::rgb(0x00, 0x80, 0x80),
    Rgba::rgb(0xC0, 0xC0, 0xC0),
    Rgba::rgb(0x80, 0x80, 0x80),
    Rgba::rgb(0xFF, 0x00, 0x00),
    Rgba::rgb(0x00, 0xFF, 0x00),
    Rgba::rgb(0xFF, 0xFF, 0x00),
    Rgba::rgb(0x00, 0x00, 0xFF),
    Rgba::rgb(0xFF, 0x00, 0xFF),
    Rgba::rgb(0x00, 0xFF, 0xFF),
    Rgba::rgb(0xFF, 0xFF, 0xFF),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base16_known_entries() {
        assert_eq!(xterm_256_color(0), Rgba::rgb(0x00, 0x00, 0x00));
        assert_eq!(xterm_256_color(15), Rgba::rgb(0xFF, 0xFF, 0xFF));
    }

    #[test]
    fn grayscale_ramp_endpoints() {
        // 232 is near-black, 255 is near-white.
        let dark = xterm_256_color(232);
        let light = xterm_256_color(255);
        assert!(dark.r < 20);
        assert!(light.r > 230);
    }

    #[test]
    fn index_231_is_near_white() {
        let c = xterm_256_color(231);
        assert_eq!(c, Rgba::rgb(255, 255, 255));
    }

    #[test]
    fn resolve_default_uses_palette_role() {
        let p = TerminalPalette::default_dark();
        assert_eq!(
            resolve_color(CellColor::Default, &p, ColorRole::Foreground),
            p.default_fg
        );
        assert_eq!(
            resolve_color(CellColor::Default, &p, ColorRole::Background),
            p.default_bg
        );
    }
}
