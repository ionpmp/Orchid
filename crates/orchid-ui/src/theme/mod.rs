//! Theme manager + design-token types.
//!
//! Two themes are bundled — `orchid-dark` (default) and `orchid-light`.
//! Additional themes may be dropped as JSON files under
//! `paths.themes_dir`; the manager loads them on construction but this
//! task defers the actual loader to a future pass (only the bundled
//! pair is required for the startup window to look right).

pub mod tokens;

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::error::{Result, UiError};
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
    /// Build a manager seeded with the bundled dark + light themes.
    /// `extra_dir` is reserved for future external theme loaders.
    ///
    /// # Errors
    ///
    /// Currently infallible; the `Result` wrapper is kept so the future
    /// disk loader can propagate IO errors without an API break.
    pub fn new(extra_dir: Option<PathBuf>) -> Result<Self> {
        let dark = bundled_dark_theme();
        let light = bundled_light_theme();
        let current = Arc::new(dark.clone());
        Ok(Self {
            themes: RwLock::new(vec![dark, light]),
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

fn bundled_dark_theme() -> Theme {
    Theme {
        meta: ThemeMeta {
            id: "orchid-dark".into(),
            display_name: "Orchid Dark".into(),
            is_dark: true,
        },
        tokens: DesignTokens {
            color: ColorTokens {
                surface_base: Color::rgb(0x17, 0x18, 0x1E),
                surface_raised: Color::rgb(0x20, 0x22, 0x2A),
                text_primary: Color::rgb(0xEB, 0xEC, 0xF0),
                text_secondary: Color::rgb(0xAE, 0xB0, 0xBC),
                text_tertiary: Color::rgb(0x80, 0x84, 0x94),
                accent_brand: Color::rgb(0xC4, 0x9B, 0xE6),
                border_default: Color::rgba(0xFF, 0xFF, 0xFF, 0x14),
            },
            typography: TypographyTokens::default(),
            radius: RadiusTokens::default(),
            spacing: SpacingTokens::default(),
        },
    }
}

fn bundled_light_theme() -> Theme {
    Theme {
        meta: ThemeMeta {
            id: "orchid-light".into(),
            display_name: "Orchid Light".into(),
            is_dark: false,
        },
        tokens: DesignTokens {
            color: ColorTokens {
                surface_base: Color::rgb(0xF6, 0xF6, 0xFA),
                surface_raised: Color::rgb(0xFF, 0xFF, 0xFF),
                text_primary: Color::rgb(0x1A, 0x1B, 0x22),
                text_secondary: Color::rgb(0x49, 0x4B, 0x58),
                text_tertiary: Color::rgb(0x6D, 0x70, 0x7C),
                accent_brand: Color::rgb(0x7A, 0x4E, 0xA8),
                border_default: Color::rgba(0, 0, 0, 0x14),
            },
            typography: TypographyTokens::default(),
            radius: RadiusTokens::default(),
            spacing: SpacingTokens::default(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn unknown_theme_errors() {
        let mgr = ThemeManager::new(None).unwrap();
        let err = mgr.set_current("nope").unwrap_err();
        assert!(matches!(err, UiError::ThemeNotFound(_)));
    }
}
