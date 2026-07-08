//! Theme manager + design-token types.
//!
//! Nine colour themes are bundled: the default Orchid pair plus Solarized,
//! Nord, Catppuccin, and high-contrast variants. Additional themes may be
//! installed as JSON files under [`paths.themes_dir`](orchid_storage::paths::OrchidPaths::themes_dir);
//! they are loaded when [`ThemeManager::new`] is constructed.

pub mod bundled;
pub mod loader;
pub mod tokens;

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::error::{Result, UiError};
pub use loader::{
    ColorTokensJson, DesignTokensJson, HexColor, ThemeDocument, ThemeMetaJson,
    load_themes_from_dir,
};
pub use tokens::{
    Color, ColorTokens, DesignTokens, RadiusTokens, SpacingTokens, TypographyTokens,
};

/// Identifying metadata attached to every theme.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct ThemeMeta {
    pub id: String,
    pub display_name: String,
    pub is_dark: bool,
}

/// A theme is metadata + a set of [`DesignTokens`].
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct Theme {
    pub meta: ThemeMeta,
    pub tokens: DesignTokens,
}

/// Theme registry + active-theme pointer.
pub struct ThemeManager {
    themes: RwLock<Vec<Theme>>,
    current: RwLock<Arc<Theme>>,
    #[allow(dead_code)]
    extra_dir: Option<PathBuf>,
}

impl std::fmt::Debug for ThemeManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThemeManager")
            .field("current", &self.current.read().meta.id)
            .finish()
    }
}

impl ThemeManager {
    /// Build a manager seeded with all bundled themes.
    ///
    /// When `extra_dir` is set, every valid `.json` file in that directory is
    /// appended to the registry (invalid files are skipped with a warning).
    ///
    /// # Errors
    ///
    /// Currently infallible; the `Result` wrapper is kept so the disk loader
    /// can propagate IO errors without an API break.
    pub fn new(extra_dir: Option<PathBuf>) -> Result<Self> {
        let mut themes = bundled::all_bundled_themes();
        if let Some(ref dir) = extra_dir {
            themes.extend(loader::load_themes_from_dir(dir));
        }
        let dark = themes
            .iter()
            .find(|t| t.meta.id == "orchid-dark")
            .cloned()
            .expect("bundled orchid-dark theme");
        let current = Arc::new(dark);
        Ok(Self {
            themes: RwLock::new(themes),
            current: RwLock::new(current),
            extra_dir,
        })
    }

    /// Switch the active theme by id.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::ThemeNotFound`] when the id is not registered.
    pub fn set_current(&self, id: &str) -> Result<()> {
        let themes = self.themes.read();
        let Some(theme) = themes.iter().find(|t| t.meta.id == id) else {
            return Err(UiError::ThemeNotFound(id.to_string()));
        };
        let next = Arc::new(theme.clone());
        drop(themes);
        *self.current.write() = next;
        Ok(())
    }

    /// Shared snapshot of the active theme.
    #[must_use]
    pub fn current(&self) -> Arc<Theme> {
        Arc::clone(&*self.current.read())
    }

    /// Every theme currently registered.
    #[must_use]
    pub fn list(&self) -> Vec<ThemeMeta> {
        self.themes.read().iter().map(|t| t.meta.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loader::ThemeDocument;

    #[test]
    fn default_is_dark() {
        let mgr = ThemeManager::new(None).unwrap();
        assert_eq!(mgr.current().meta.id, "orchid-dark");
    }

    #[test]
    fn set_current_updates_active() {
        let mgr = ThemeManager::new(None).unwrap();
        mgr.set_current("orchid-light").unwrap();
        assert!(!mgr.current().meta.is_dark);
    }

    #[test]
    fn set_current_nord_dark() {
        let mgr = ThemeManager::new(None).unwrap();
        mgr.set_current("nord-dark").unwrap();
        assert_eq!(mgr.current().meta.id, "nord-dark");
        assert!(mgr.current().meta.is_dark);
        assert_eq!(
            mgr.current().tokens.color.surface_base,
            Color::rgb(0x2E, 0x34, 0x40)
        );
    }

    #[test]
    fn list_includes_all_bundled_themes() {
        let mgr = ThemeManager::new(None).unwrap();
        assert!(mgr.list().len() >= 9);
        let ids: Vec<_> = mgr.list().into_iter().map(|m| m.id).collect();
        for expected in [
            "orchid-dark",
            "orchid-light",
            "solarized-dark",
            "solarized-light",
            "nord-dark",
            "catppuccin-mocha",
            "catppuccin-latte",
            "high-contrast-dark",
            "high-contrast-light",
        ] {
            assert!(ids.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[test]
    fn json_loader_round_trip() {
        let source = bundled::solarized_dark_theme();
        let doc = ThemeDocument::from(&source);
        let json = serde_json::to_string_pretty(&doc).unwrap();

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("solarized.json"), &json).unwrap();

        let loaded = load_themes_from_dir(dir.path());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].meta.id, "solarized-dark");
        assert_eq!(loaded[0].tokens.color, source.tokens.color);

        let re_doc = ThemeDocument::from(&loaded[0]);
        let re_json = serde_json::to_string(&re_doc).unwrap();
        let parsed: ThemeDocument = serde_json::from_str(&re_json).unwrap();
        assert_eq!(parsed.meta.id, source.meta.id);
        assert_eq!(parsed.tokens.color.surface_base.0, source.tokens.color.surface_base);
    }

    #[test]
    fn unknown_theme_errors() {
        let mgr = ThemeManager::new(None).unwrap();
        let err = mgr.set_current("nope").unwrap_err();
        assert!(matches!(err, UiError::ThemeNotFound(_)));
    }
}
