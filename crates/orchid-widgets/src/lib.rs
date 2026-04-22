//! Widget framework for Orchid (work in progress).

#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

pub mod builtin;
pub mod commands;
pub mod error;
pub mod events;
pub mod group;
pub mod layout;
pub mod manager;
pub mod registry;
pub mod widget;
pub mod workspace;

pub use error::{Result, WidgetError};
pub use events::*;
pub use group::{GroupManager, WidgetGroup};
pub use layout::{
    LayoutEngine, LayoutMode, LayoutOptions, LayoutSnapshot, PixelBounds, PlacedWidget,
    ViewportSize,
};
pub use registry::WidgetRegistry;
pub use widget::descriptor::{WidgetCategory, WidgetDescriptor, WidgetFactory};
pub use widget::instance::{SharedInstance, WidgetInstanceRuntime};
pub use widget::lifecycle::LifecycleController;
pub use widget::snapshot::{
    TerminalPayload, TerminalPayloadCell, WidgetPayload, WidgetSnapshot, WidgetStatus,
};
pub use widget::{PeriodicRefresh, Widget, WidgetCapabilities, WidgetContext};

pub use builtin::{register_all, register_core};
pub use widget::payloads::{
    IndicatorStatus, MediaPlayerPayload, MoonPayload, PasswordEntryDetailView,
    PasswordEntryView, PasswordManagerPayload, RssItemView, RssPayload,
    SearchCandidateView, SystemIndicator, SystemPayload, UniversalSearchPayload,
    WeatherForecastDay, WeatherPayload, WeatherStatusTag,
};
pub use workspace::{WorkspaceManager, MAX_WORKSPACES};

pub use commands::build_command_set;
pub use manager::operations::CreateWidgetRequest;
pub use manager::{WidgetManager, WidgetManagerOptions};

/// Crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
