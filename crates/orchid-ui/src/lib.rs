//! Slint-based UI layer for Orchid.
//!
//! The library currently ships the renderer-agnostic half of the workspace
//! dashboard:
//!
//! * [`widgets::terminal`] — terminal palette / clipboard / cell conversion
//!   helpers **plus** the [`orchid_widgets::Widget`] implementation
//!   ([`TerminalWidget`]) that will drive the dashboard's first concrete
//!   widget.
//! * [`widgets::view`] — the [`WidgetView`]/[`WidgetViewDispatcher`] bridge
//!   the Slint shell uses to fan [`WidgetSnapshot`]s out to the right Slint
//!   components.
//!
//! The Slint component tree itself (`ui/main.slint`) is still a stub and
//! the app-bootstrap / theme / window-controller / drag-state-machine work
//! lives in a follow-up UI-shell task. See the crate README for details.

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod widgets;

pub use widgets::terminal::{
    palette_from_flavor, snapshot_to_cells, terminal_descriptor, ArboardClipboard,
    RenderCell, StoredBackend, TerminalWidget, TerminalWidgetDeps, TerminalWidgetState,
    TerminalWidgetView, ThemeFlavor, TERMINAL_TYPE_ID,
};
pub use widgets::view::{SlintPayload, SlintTerminalCell, WidgetView, WidgetViewDispatcher};

/// Returns the crate version as declared in `Cargo.toml`.
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
