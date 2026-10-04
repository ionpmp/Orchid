//! Built-in theme and widget marketplace.
//!
//! Themes are JSON palettes copied into the user's themes directory. Widgets
//! are the ones already compiled into Orchid; installing one adds it to the
//! workspace. Nothing is downloaded.

use std::path::{Path, PathBuf};

use crate::theme::{
    Color, ColorTokensJson, DesignTokensJson, HexColor, ThemeDocument, ThemeMetaJson,
};

/// A palette the user can copy into the themes directory.
#[derive(Debug, Clone, Copy)]
pub struct CatalogTheme {
    /// File stem and `[appearance].theme` id.
    pub id: &'static str,
    /// Name shown in Settings.
    pub display_name: &'static str,
}

/// Palettes shipped with Orchid but not loaded until the user installs them.
#[must_use]
pub fn catalog_themes() -> &'static [CatalogTheme] {
    &CATALOG
}

/// Built-in widget type ids the marketplace can place on the workspace.
#[must_use]
pub fn catalog_widgets() -> &'static [&'static str] {
    WIDGETS
}

/// Fluent key for a catalog widget's dock name.
#[must_use]
pub fn widget_label_key(type_id: &str) -> &'static str {
    match type_id {
        "universal-search" => "dock-widget-search",
        "media-player" => "dock-widget-media",
        "password-manager" => "dock-widget-password",
        "file-manager" => "dock-widget-fm",
        "terminal" => "dock-widget-terminal",
        "weather" => "dock-widget-weather",
        "moon" => "dock-widget-moon",
        "jyotish" => "dock-widget-jyotish",
        "clock" => "dock-widget-clock",
        "system" => "dock-widget-system",
        "processes" => "dock-widget-processes",
        "optimize" => "dock-widget-optimize",
        "protect" => "dock-widget-protect",
        "calculator" => "dock-widget-calculator",
        "notes" => "dock-widget-notes",
        "agent" => "dock-widget-agent",
        "calendar" => "dock-widget-calendar",
        "rss" => "dock-widget-rss",
        "recent-files" => "dock-widget-recent-files",
        "audio-player" => "dock-widget-audio-player",
        "video-player" => "dock-widget-video-player",
        "media-viewer" => "dock-widget-media-viewer",
        "viewer" => "dock-widget-viewer",
        "browser" => "dock-widget-browser",
        "document-editor" => "dock-widget-document-editor",
        _ => "dock-widget-terminal",
    }
}

/// Write the catalog palette `{id}.json` into `dir`.
///
/// # Errors
///
/// Returns an error when `id` is not in the catalog, or when a file with that
/// name already belongs to a different theme.
pub fn install_theme(dir: &Path, id: &str) -> Result<(), String> {
    let doc = theme_document(id).ok_or_else(|| format!("unknown theme `{id}`"))?;
    let path = theme_path(dir, id)?;
    if path.exists() {
        match read_theme(&path) {
            Ok(existing) if existing.meta.id == id => {}
            Ok(_) => {
                return Err(format!(
                    "`{}` is not the catalog theme `{id}`",
                    path.display()
                ));
            }
            Err(reason) => return Err(reason),
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("create themes dir: {e}"))?;
    let json = serde_json::to_string_pretty(&doc).map_err(|e| format!("encode theme: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write theme: {e}"))?;
    Ok(())
}

/// Delete a previously installed catalog palette.
///
/// A missing file is success. A file whose theme id does not match is left
/// in place.
///
/// # Errors
///
/// Returns an error when `id` is not in the catalog or the file cannot be read.
pub fn remove_theme(dir: &Path, id: &str) -> Result<(), String> {
    if theme_document(id).is_none() {
        return Err(format!("unknown theme `{id}`"));
    }
    let path = theme_path(dir, id)?;
    if !path.is_file() {
        return Ok(());
    }
    let existing = read_theme(&path)?;
    if existing.meta.id != id {
        return Err(format!(
            "refusing to delete `{}`; it is not catalog theme `{id}`",
            path.display()
        ));
    }
    std::fs::remove_file(&path).map_err(|e| format!("remove theme: {e}"))?;
    Ok(())
}

/// Whether `{id}.json` in `dir` is this catalog palette.
#[must_use]
pub fn theme_installed(dir: &Path, id: &str) -> bool {
    let Ok(path) = theme_path(dir, id) else {
        return false;
    };
    read_theme(&path).ok().is_some_and(|doc| doc.meta.id == id)
}

fn theme_path(dir: &Path, id: &str) -> Result<PathBuf, String> {
    if !safe_id(id) || theme_document(id).is_none() {
        return Err(format!("unknown theme `{id}`"));
    }
    Ok(dir.join(format!("{id}.json")))
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn read_theme(path: &Path) -> Result<ThemeDocument, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read theme: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse theme: {e}"))
}

fn theme_document(id: &str) -> Option<ThemeDocument> {
    Some(match id {
        "market-ink" => palette(
            "market-ink",
            "Ink",
            true,
            Color::rgb(0x12, 0x12, 0x14),
            Color::rgb(0x1C, 0x1C, 0x22),
            Color::rgb(0xF4, 0xF1, 0xEA),
            Color::rgb(0xC8, 0xC2, 0xB4),
            Color::rgb(0x8A, 0x84, 0x78),
            Color::rgb(0xE8, 0xC9, 0x7A),
            Color::rgba(0xFF, 0xFF, 0xFF, 0x22),
        ),
        "market-dawn" => palette(
            "market-dawn",
            "Dawn",
            false,
            Color::rgb(0xF6, 0xF1, 0xE7),
            Color::rgb(0xFF, 0xFB, 0xF5),
            Color::rgb(0x2A, 0x24, 0x1C),
            Color::rgb(0x5C, 0x4E, 0x3A),
            Color::rgb(0x8A, 0x78, 0x62),
            Color::rgb(0xC4, 0x5C, 0x26),
            Color::rgba(0x2A, 0x24, 0x1C, 0x18),
        ),
        "market-pine" => palette(
            "market-pine",
            "Pine",
            true,
            Color::rgb(0x0E, 0x1A, 0x16),
            Color::rgb(0x16, 0x2A, 0x22),
            Color::rgb(0xE7, 0xF2, 0xEA),
            Color::rgb(0xA8, 0xC4, 0xB4),
            Color::rgb(0x6E, 0x8A, 0x7A),
            Color::rgb(0x7D, 0xC9, 0x8A),
            Color::rgba(0xFF, 0xFF, 0xFF, 0x1A),
        ),
        "market-ember" => palette(
            "market-ember",
            "Ember",
            true,
            Color::rgb(0x1A, 0x10, 0x0E),
            Color::rgb(0x2A, 0x18, 0x14),
            Color::rgb(0xFB, 0xF0, 0xE8),
            Color::rgb(0xE0, 0xB8, 0xA4),
            Color::rgb(0xA8, 0x78, 0x68),
            Color::rgb(0xFF, 0x6B, 0x3D),
            Color::rgba(0xFF, 0xFF, 0xFF, 0x1C),
        ),
        _ => return None,
    })
}

fn palette(
    id: &str,
    display_name: &str,
    is_dark: bool,
    surface_base: Color,
    surface_raised: Color,
    text_primary: Color,
    text_secondary: Color,
    text_tertiary: Color,
    accent_brand: Color,
    border_default: Color,
) -> ThemeDocument {
    ThemeDocument {
        meta: ThemeMetaJson {
            id: id.to_string(),
            display_name: display_name.to_string(),
            is_dark,
        },
        tokens: DesignTokensJson {
            color: ColorTokensJson {
                surface_base: HexColor(surface_base),
                surface_raised: HexColor(surface_raised),
                text_primary: HexColor(text_primary),
                text_secondary: HexColor(text_secondary),
                text_tertiary: HexColor(text_tertiary),
                accent_brand: HexColor(accent_brand),
                border_default: HexColor(border_default),
            },
            typography: Default::default(),
            radius: Default::default(),
            spacing: Default::default(),
        },
    }
}

const CATALOG: &[CatalogTheme] = &[
    CatalogTheme {
        id: "market-ink",
        display_name: "Ink",
    },
    CatalogTheme {
        id: "market-dawn",
        display_name: "Dawn",
    },
    CatalogTheme {
        id: "market-pine",
        display_name: "Pine",
    },
    CatalogTheme {
        id: "market-ember",
        display_name: "Ember",
    },
];

const WIDGETS: &[&str] = &[
    "terminal",
    "weather",
    "moon",
    "jyotish",
    "clock",
    "system",
    "processes",
    "optimize",
    "protect",
    "calculator",
    "notes",
    "calendar",
    "rss",
    "recent-files",
    "universal-search",
    "media-player",
    "audio-player",
    "video-player",
    "media-viewer",
    "password-manager",
    "viewer",
    "browser",
    "document-editor",
    "file-manager",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeManager;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("orchid-market-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn install_then_remove_roundtrips_and_reloads() {
        let dir = scratch("install");
        assert!(install_theme(&dir, "../market-ink").is_err());
        assert!(install_theme(&dir, "not-a-theme").is_err());
        install_theme(&dir, "market-ink").unwrap();
        assert!(theme_installed(&dir, "market-ink"));
        let mgr = ThemeManager::new(Some(dir.clone())).unwrap();
        assert!(mgr.list().iter().any(|t| t.id == "market-ink"));
        mgr.reload_installed();
        assert!(mgr.set_current("market-ink").is_ok());

        remove_theme(&dir, "market-ink").unwrap();
        assert!(!theme_installed(&dir, "market-ink"));
        mgr.reload_installed();
        assert!(mgr.list().iter().all(|t| t.id != "market-ink"));
        assert_eq!(mgr.current().meta.id, "orchid-dark");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_leaves_a_foreign_file_alone() {
        let dir = scratch("foreign");
        let path = dir.join("market-ink.json");
        std::fs::write(
            &path,
            r##"{"id":"someone-else","display_name":"Other","is_dark":true,"tokens":{"color":{"surface_base":"#000000","surface_raised":"#111111","text_primary":"#ffffff","text_secondary":"#cccccc","text_tertiary":"#888888","accent_brand":"#ff00ff","border_default":"#222222"}}}"##,
        )
        .unwrap();
        let err = remove_theme(&dir, "market-ink").unwrap_err();
        assert!(err.contains("refusing"));
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
