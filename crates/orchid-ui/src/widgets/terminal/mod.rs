//! Terminal widget support for the Orchid UI layer.
//!
//! This module ships the **renderer-agnostic** glue between the `orchid-terminal`
//! library and the wider UI:
//!
//! * [`palette`] — map Orchid theme flavours to an
//!   [`orchid_terminal::TerminalPalette`].
//! * [`render`] — turn a [`orchid_terminal::GridSnapshot`] into a flat
//!   `Vec<Vec<RenderCell>>` of resolved RGBA cells, ready to feed a renderer.
//! * [`clipboard`] — [`arboard`]-backed implementation of
//!   [`orchid_crypto::SecureClipboard`], used by both the terminal's
//!   copy/paste hotkeys and the future password-manager widget.
//!
//! The actual Slint-side view (`ui/widgets/terminal.slint`), the controller
//! that binds it to a live [`orchid_terminal::TerminalSession`], and the
//! bootstrap wiring into `orchid-app` are scheduled together with the broader
//! UI shell (theme global, startup window, app bootstrap) in a follow-up
//! task. They need the shared Theme infrastructure which is not yet
//! present in the workspace; see the crate README.

pub mod clipboard;
pub mod palette;
pub mod render;
pub mod view;
pub mod widget;

pub use clipboard::ArboardClipboard;
pub use palette::{palette_from_flavor, palette_from_theme, ThemeFlavor};
pub use render::{snapshot_to_cells, RenderCell};
pub use view::TerminalWidgetView;
pub use widget::{
    add_tab, close_tab, switch_tab, terminal_descriptor, StoredBackend, TerminalWidget,
    TerminalWidgetDeps, TerminalWidgetState, TERMINAL_TYPE_ID,
};
