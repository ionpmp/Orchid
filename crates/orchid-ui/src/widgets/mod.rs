//! Widget integrations for the Orchid UI layer.
//!
//! Exposes:
//!
//! * [`terminal`] — terminal-specific palette, clipboard, cell-render
//!   helpers and a [`Widget`](orchid_widgets::Widget) implementation.
//! * [`view`] — the renderer-agnostic [`WidgetView`] trait and
//!   [`WidgetViewDispatcher`] that route per-type payloads to the
//!   eventual Slint components.

pub mod terminal;
pub mod view;

pub use view::{SlintPayload, SlintTerminalCell, WidgetView, WidgetViewDispatcher};
