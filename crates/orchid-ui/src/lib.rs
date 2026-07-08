//! Slint-based UI layer for Orchid.

#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

pub mod app;
mod commands;
pub mod error;
mod slint_generated;
mod autostart;
mod system_theme;
pub mod theme;
mod terminal_font_metrics;
mod terminal_raster;
pub mod widgets;
pub mod window;

pub use app::OrchidApp;
pub use error::{Result, UiError};
pub use theme::{
    Color, ColorTokens, DesignTokens, RadiusTokens, SpacingTokens, Theme, ThemeManager,
    ThemeMeta, TypographyTokens,
};
pub use widgets::terminal::{
    palette_from_flavor, palette_from_theme, snapshot_to_cells, terminal_descriptor,
    ArboardClipboard, RenderCell, StoredBackend, TerminalWidget, TerminalWidgetDeps,
    TerminalWidgetState, TerminalWidgetView, ThemeFlavor, TERMINAL_TYPE_ID,
};
pub use widgets::view::{SlintPayload, SlintTerminalCell, WidgetView, WidgetViewDispatcher};
pub use window::main_window::MainWindowController;
pub use window::startup::StartupWindowController;

/// Crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }
}
