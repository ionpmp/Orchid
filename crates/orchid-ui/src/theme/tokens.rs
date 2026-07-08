//! Design-token primitives used by themes.

use slint::Color as SlintColor;

/// 8-bit sRGB colour (with alpha). Convert to Slint's colour type via
/// [`Color::to_slint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Opaque RGB colour.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xFF }
    }

    /// RGBA colour.
    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Convert to Slint's `Color`.
    #[must_use]
    pub fn to_slint(self) -> SlintColor {
        SlintColor::from_argb_u8(self.a, self.r, self.g, self.b)
    }
}

/// Colour tokens referenced from Slint's `Theme` global.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct ColorTokens {
    pub surface_base: Color,
    pub surface_raised: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_tertiary: Color,
    pub accent_brand: Color,
    pub border_default: Color,
}

/// Typography tokens.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct TypographyTokens {
    pub font_family_sans: String,
    pub font_family_mono: String,
    pub size_sm: f32,
    pub size_md: f32,
    pub size_lg: f32,
    pub size_xl: f32,
    pub size_2xl: f32,
    pub size_3xl: f32,
    pub weight_regular: u16,
    pub weight_medium: u16,
    pub weight_semibold: u16,
}

impl Default for TypographyTokens {
    fn default() -> Self {
        Self {
            font_family_sans: "Segoe UI, Inter, sans-serif".into(),
            // Prefer a single monospace design (Cascadia Mono) to avoid per-glyph mixed fallbacks
            // that make column spacing look irregular next to "Cascadia Code, ...".
            font_family_mono: "Cascadia Mono, Cascadia Code, Consolas, ui-monospace, monospace".into(),
            size_sm: 12.0,
            size_md: 14.0,
            size_lg: 18.0,
            size_xl: 22.0,
            size_2xl: 28.0,
            size_3xl: 36.0,
            weight_regular: 400,
            weight_medium: 500,
            weight_semibold: 600,
        }
    }
}

/// Border-radius scale.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct RadiusTokens {
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
}

impl Default for RadiusTokens {
    fn default() -> Self {
        Self {
            sm: 4.0,
            md: 8.0,
            lg: 16.0,
        }
    }
}

/// Spacing scale. `unit` is the base spacing increment (4 px by default).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct SpacingTokens {
    pub unit: f32,
}

impl Default for SpacingTokens {
    fn default() -> Self {
        Self { unit: 4.0 }
    }
}

/// Bundle of every token type pushed into the Slint `Theme` global.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct DesignTokens {
    pub color: ColorTokens,
    pub typography: TypographyTokens,
    pub radius: RadiusTokens,
    pub spacing: SpacingTokens,
}
