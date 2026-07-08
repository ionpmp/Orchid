//! Factory functions for bundled colour themes.

use super::tokens::{
    Color, ColorTokens, DesignTokens, RadiusTokens, SpacingTokens, TypographyTokens,
};
use super::{Theme, ThemeMeta};

fn theme(meta: ThemeMeta, color: ColorTokens) -> Theme {
    Theme {
        meta,
        tokens: DesignTokens {
            color,
            typography: TypographyTokens::default(),
            radius: RadiusTokens::default(),
            spacing: SpacingTokens::default(),
        },
    }
}

/// Default Orchid dark theme.
#[must_use]
pub fn orchid_dark_theme() -> Theme {
    theme(
        ThemeMeta {
            id: "orchid-dark".into(),
            display_name: "Orchid Dark".into(),
            is_dark: true,
        },
        ColorTokens {
            surface_base: Color::rgb(0x17, 0x18, 0x1E),
            surface_raised: Color::rgb(0x20, 0x22, 0x2A),
            text_primary: Color::rgb(0xEB, 0xEC, 0xF0),
            text_secondary: Color::rgb(0xAE, 0xB0, 0xBC),
            text_tertiary: Color::rgb(0x80, 0x84, 0x94),
            accent_brand: Color::rgb(0xC4, 0x9B, 0xE6),
            border_default: Color::rgba(0xFF, 0xFF, 0xFF, 0x14),
        },
    )
}

/// Default Orchid light theme.
#[must_use]
pub fn orchid_light_theme() -> Theme {
    theme(
        ThemeMeta {
            id: "orchid-light".into(),
            display_name: "Orchid Light".into(),
            is_dark: false,
        },
        ColorTokens {
            surface_base: Color::rgb(0xF6, 0xF6, 0xFA),
            surface_raised: Color::rgb(0xFF, 0xFF, 0xFF),
            text_primary: Color::rgb(0x1A, 0x1B, 0x22),
            text_secondary: Color::rgb(0x49, 0x4B, 0x58),
            text_tertiary: Color::rgb(0x6D, 0x70, 0x7C),
            accent_brand: Color::rgb(0x7A, 0x4E, 0xA8),
            border_default: Color::rgba(0, 0, 0, 0x14),
        },
    )
}

/// Solarized Dark (Ethan Schoonover palette).
#[must_use]
pub fn solarized_dark_theme() -> Theme {
    theme(
        ThemeMeta {
            id: "solarized-dark".into(),
            display_name: "Solarized Dark".into(),
            is_dark: true,
        },
        ColorTokens {
            surface_base: Color::rgb(0x00, 0x2B, 0x36),   // base03
            surface_raised: Color::rgb(0x07, 0x36, 0x42), // base02
            text_primary: Color::rgb(0x83, 0x94, 0x96),    // base0
            text_secondary: Color::rgb(0x65, 0x7B, 0x83), // base00
            text_tertiary: Color::rgb(0x58, 0x6E, 0x75),  // base01
            accent_brand: Color::rgb(0x26, 0x8B, 0xD2),   // blue
            border_default: Color::rgba(0x93, 0xA2, 0xA1, 0x40), // base1 @ 25%
        },
    )
}

/// Solarized Light (Ethan Schoonover palette).
#[must_use]
pub fn solarized_light_theme() -> Theme {
    theme(
        ThemeMeta {
            id: "solarized-light".into(),
            display_name: "Solarized Light".into(),
            is_dark: false,
        },
        ColorTokens {
            surface_base: Color::rgb(0xFD, 0xF6, 0xE3),   // base3
            surface_raised: Color::rgb(0xEE, 0xE8, 0xD5), // base2
            text_primary: Color::rgb(0x65, 0x7B, 0x83),  // base00
            text_secondary: Color::rgb(0x58, 0x6E, 0x75), // base01
            text_tertiary: Color::rgb(0x93, 0xA2, 0xA1), // base1
            accent_brand: Color::rgb(0x26, 0x8B, 0xD2),  // blue
            border_default: Color::rgba(0x58, 0x6E, 0x75, 0x40), // base01 @ 25%
        },
    )
}

/// Nord dark polar-night palette.
#[must_use]
pub fn nord_dark_theme() -> Theme {
    theme(
        ThemeMeta {
            id: "nord-dark".into(),
            display_name: "Nord Dark".into(),
            is_dark: true,
        },
        ColorTokens {
            surface_base: Color::rgb(0x2E, 0x34, 0x40),   // nord0
            surface_raised: Color::rgb(0x3B, 0x42, 0x52), // nord1
            text_primary: Color::rgb(0xEC, 0xEF, 0xF4),  // nord6
            text_secondary: Color::rgb(0xD8, 0xDE, 0xE9), // nord4
            text_tertiary: Color::rgb(0x4C, 0x56, 0x6A), // nord3
            accent_brand: Color::rgb(0x88, 0xC0, 0xD0),  // nord8
            border_default: Color::rgba(0xD8, 0xDE, 0xE9, 0x30), // nord4 @ ~19%
        },
    )
}

/// Catppuccin Mocha.
#[must_use]
pub fn catppuccin_mocha_theme() -> Theme {
    theme(
        ThemeMeta {
            id: "catppuccin-mocha".into(),
            display_name: "Catppuccin Mocha".into(),
            is_dark: true,
        },
        ColorTokens {
            surface_base: Color::rgb(0x1E, 0x1E, 0x2E),   // base
            surface_raised: Color::rgb(0x31, 0x32, 0x44), // surface0
            text_primary: Color::rgb(0xCD, 0xD6, 0xF4),  // text
            text_secondary: Color::rgb(0xBA, 0xC2, 0xDE), // subtext1
            text_tertiary: Color::rgb(0xA6, 0xAD, 0xC8), // subtext0
            accent_brand: Color::rgb(0xCB, 0xA6, 0xF7),  // mauve
            border_default: Color::rgba(0xCD, 0xD6, 0xF4, 0x20),
        },
    )
}

/// Catppuccin Latte.
#[must_use]
pub fn catppuccin_latte_theme() -> Theme {
    theme(
        ThemeMeta {
            id: "catppuccin-latte".into(),
            display_name: "Catppuccin Latte".into(),
            is_dark: false,
        },
        ColorTokens {
            surface_base: Color::rgb(0xEF, 0xF1, 0xF5),   // base
            surface_raised: Color::rgb(0xCC, 0xD0, 0xDA), // surface0
            text_primary: Color::rgb(0x4C, 0x4F, 0x69),  // text
            text_secondary: Color::rgb(0x5C, 0x5F, 0x77), // subtext1
            text_tertiary: Color::rgb(0x6C, 0x6F, 0x85), // subtext0
            accent_brand: Color::rgb(0x88, 0x39, 0xEF),  // mauve
            border_default: Color::rgba(0x4C, 0x4F, 0x69, 0x20),
        },
    )
}

/// High-contrast dark (WCAG-oriented).
#[must_use]
pub fn high_contrast_dark_theme() -> Theme {
    theme(
        ThemeMeta {
            id: "high-contrast-dark".into(),
            display_name: "High Contrast Dark".into(),
            is_dark: true,
        },
        ColorTokens {
            surface_base: Color::rgb(0x00, 0x00, 0x00),
            surface_raised: Color::rgb(0x1A, 0x1A, 0x1A),
            text_primary: Color::rgb(0xFF, 0xFF, 0xFF),
            text_secondary: Color::rgb(0xE0, 0xE0, 0xE0),
            text_tertiary: Color::rgb(0xB0, 0xB0, 0xB0),
            accent_brand: Color::rgb(0xFF, 0xFF, 0x00), // yellow
            border_default: Color::rgb(0xFF, 0xFF, 0xFF),
        },
    )
}

/// High-contrast light (WCAG-oriented).
#[must_use]
pub fn high_contrast_light_theme() -> Theme {
    theme(
        ThemeMeta {
            id: "high-contrast-light".into(),
            display_name: "High Contrast Light".into(),
            is_dark: false,
        },
        ColorTokens {
            surface_base: Color::rgb(0xFF, 0xFF, 0xFF),
            surface_raised: Color::rgb(0xF0, 0xF0, 0xF0),
            text_primary: Color::rgb(0x00, 0x00, 0x00),
            text_secondary: Color::rgb(0x1A, 0x1A, 0x1A),
            text_tertiary: Color::rgb(0x40, 0x40, 0x40),
            accent_brand: Color::rgb(0x00, 0x00, 0xFF), // blue
            border_default: Color::rgb(0x00, 0x00, 0x00),
        },
    )
}

/// Every theme shipped inside the binary.
#[must_use]
pub fn all_bundled_themes() -> Vec<Theme> {
    vec![
        orchid_dark_theme(),
        orchid_light_theme(),
        solarized_dark_theme(),
        solarized_light_theme(),
        nord_dark_theme(),
        catppuccin_mocha_theme(),
        catppuccin_latte_theme(),
        high_contrast_dark_theme(),
        high_contrast_light_theme(),
    ]
}
