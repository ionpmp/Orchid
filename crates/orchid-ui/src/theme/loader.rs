//! JSON theme loader for user-installed themes under `paths.themes_dir`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::tokens::{
    Color, ColorTokens, DesignTokens, RadiusTokens, SpacingTokens, TypographyTokens,
};
use super::{Theme, ThemeMeta};

/// Theme metadata as it appears in a JSON theme file.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[allow(missing_docs)]
pub struct ThemeMetaJson {
    pub id: String,
    pub display_name: String,
    pub is_dark: bool,
}

/// Colour tokens with hex-string colours (`#RRGGBB` or `#RRGGBBAA`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[allow(missing_docs)]
pub struct ColorTokensJson {
    pub surface_base: HexColor,
    pub surface_raised: HexColor,
    pub text_primary: HexColor,
    pub text_secondary: HexColor,
    pub text_tertiary: HexColor,
    pub accent_brand: HexColor,
    pub border_default: HexColor,
}

/// Full design-token bundle for JSON themes (colour required; other groups default).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[allow(missing_docs)]
pub struct DesignTokensJson {
    pub color: ColorTokensJson,
    #[serde(default)]
    pub typography: TypographyTokens,
    #[serde(default)]
    pub radius: RadiusTokens,
    #[serde(default)]
    pub spacing: SpacingTokens,
}

/// On-disk theme document: metadata + token bundle.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[allow(missing_docs)]
pub struct ThemeDocument {
    #[serde(flatten)]
    pub meta: ThemeMetaJson,
    pub tokens: DesignTokensJson,
}

/// Hex colour string (`#RRGGBB` or `#RRGGBBAA`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HexColor(pub Color);

impl HexColor {
    /// Parse a `#RRGGBB` or `#RRGGBBAA` string.
    pub fn parse(s: &str) -> Result<Self, String> {
        parse_hex_color(s).map(HexColor)
    }
}

impl<'de> Deserialize<'de> for HexColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl Serialize for HexColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&color_to_hex(self.0))
    }
}

impl From<ThemeDocument> for Theme {
    fn from(doc: ThemeDocument) -> Self {
        let color = &doc.tokens.color;
        Self {
            meta: ThemeMeta {
                id: doc.meta.id,
                display_name: doc.meta.display_name,
                is_dark: doc.meta.is_dark,
            },
            tokens: DesignTokens {
                color: ColorTokens {
                    surface_base: color.surface_base.0,
                    surface_raised: color.surface_raised.0,
                    text_primary: color.text_primary.0,
                    text_secondary: color.text_secondary.0,
                    text_tertiary: color.text_tertiary.0,
                    accent_brand: color.accent_brand.0,
                    border_default: color.border_default.0,
                },
                typography: doc.tokens.typography,
                radius: doc.tokens.radius,
                spacing: doc.tokens.spacing,
            },
        }
    }
}

impl From<&Theme> for ThemeDocument {
    fn from(theme: &Theme) -> Self {
        let c = &theme.tokens.color;
        Self {
            meta: ThemeMetaJson {
                id: theme.meta.id.clone(),
                display_name: theme.meta.display_name.clone(),
                is_dark: theme.meta.is_dark,
            },
            tokens: DesignTokensJson {
                color: ColorTokensJson {
                    surface_base: HexColor(c.surface_base),
                    surface_raised: HexColor(c.surface_raised),
                    text_primary: HexColor(c.text_primary),
                    text_secondary: HexColor(c.text_secondary),
                    text_tertiary: HexColor(c.text_tertiary),
                    accent_brand: HexColor(c.accent_brand),
                    border_default: HexColor(c.border_default),
                },
                typography: theme.tokens.typography.clone(),
                radius: theme.tokens.radius,
                spacing: theme.tokens.spacing,
            },
        }
    }
}

/// Load every valid `.json` theme file in `dir`. Invalid files are skipped with a warning.
#[must_use]
pub fn load_themes_from_dir(dir: &Path) -> Vec<Theme> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(path = %dir.display(), error = %e, "failed to read themes directory");
            return Vec::new();
        }
    };

    let mut themes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match load_theme_file(&path) {
            Ok(theme) => themes.push(theme),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping invalid theme file");
            }
        }
    }
    themes
}

fn load_theme_file(path: &Path) -> Result<Theme, String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("read failed: {e}"))?;
    let doc: ThemeDocument =
        serde_json::from_str(&contents).map_err(|e| format!("parse failed: {e}"))?;
    Ok(Theme::from(doc))
}

fn parse_hex_color(s: &str) -> Result<Color, String> {
    let hex = s.strip_prefix('#').ok_or_else(|| format!("expected leading '#': {s}"))?;
    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| e.to_string())?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| e.to_string())?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| e.to_string())?;
            Ok(Color::rgb(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| e.to_string())?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| e.to_string())?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| e.to_string())?;
            let a = u8::from_str_radix(&hex[6..8], 16).map_err(|e| e.to_string())?;
            Ok(Color::rgba(r, g, b, a))
        }
        _ => Err(format!("expected 6 or 8 hex digits, got {hex}")),
    }
}

fn color_to_hex(color: Color) -> String {
    if color.a == 0xFF {
        format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
    } else {
        format!("#{:02X}{:02X}{:02X}{:02X}", color.r, color.g, color.b, color.a)
    }
}
