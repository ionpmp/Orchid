//! View-mode presets.

use super::config::ViewMode;

/// Render-hint bundle for a view mode.
#[derive(Debug, Clone, Copy)]
#[allow(missing_docs)]
pub struct ViewModeConfig {
    pub mode: ViewMode,
    pub item_height: f32,
    pub item_width: f32,
    pub show_thumbnails: bool,
    pub columns: Option<u16>,
}

/// Presets for each mode; density is advisory and scales row heights.
#[must_use]
pub fn config_for_mode(mode: ViewMode, density_scale: f32) -> ViewModeConfig {
    let scale = density_scale.max(0.5);
    match mode {
        ViewMode::Icons => ViewModeConfig {
            mode,
            item_height: 144.0 * scale,
            item_width: 120.0 * scale,
            show_thumbnails: true,
            columns: Some(4),
        },
        ViewMode::List => ViewModeConfig {
            mode,
            item_height: 36.0 * scale,
            item_width: f32::INFINITY,
            show_thumbnails: true,
            columns: None,
        },
        ViewMode::Details => ViewModeConfig {
            mode,
            item_height: 28.0 * scale,
            item_width: f32::INFINITY,
            show_thumbnails: false,
            columns: None,
        },
        ViewMode::Gallery => ViewModeConfig {
            mode,
            item_height: 288.0 * scale,
            item_width: 256.0 * scale,
            show_thumbnails: true,
            columns: Some(3),
        },
    }
}
