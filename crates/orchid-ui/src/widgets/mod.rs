//! Widget integrations for the Orchid UI layer.
//!
//! Exposes:
//!
//! * [`terminal`] — terminal-specific palette, clipboard, cell-render
//!   helpers and a [`Widget`](orchid_widgets::Widget) implementation.
//! * [`view`] — the Slint-free [`WidgetView`] trait and
//!   [`WidgetViewDispatcher`]. The desktop shell maps `WidgetPayload`
//!   in `window/main_window/` and does not call the dispatcher each frame.

pub mod terminal;
pub mod view;

pub use view::{SlintPayload, SlintTerminalCell, WidgetView, WidgetViewDispatcher};
