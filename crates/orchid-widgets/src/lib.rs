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
pub mod recent_files;
pub mod registry;
pub mod visibility;
pub mod widget;
pub mod workspace;

pub use error::{Result, WidgetError};
pub use events::*;
pub use group::{GroupManager, WidgetGroup};
pub use layout::{
    free_placement_from_pixel_bounds, position_from_content_top_left, LayoutEngine, LayoutMode,
    LayoutOptions, LayoutSnapshot, PixelBounds, PlacedWidget, ViewportSize,
};
pub use registry::WidgetRegistry;
pub use widget::descriptor::{WidgetCategory, WidgetDescriptor, WidgetFactory};
pub use widget::instance::{SharedInstance, WidgetInstanceRuntime};
pub use widget::lifecycle::LifecycleController;
pub use widget::snapshot::{
    TerminalDividerPayload, TerminalPanePayload, TerminalPayload, TerminalPayloadCell,
    TerminalTabPayload, WidgetPayload, WidgetSnapshot, WidgetStatus,
};
pub use widget::WidgetSnapshotCache;
pub use widget::{PeriodicRefresh, Widget, WidgetCapabilities, WidgetContext};

pub use builtin::{register_all, register_core};
pub use widget::payloads::{
    CalcHistoryRow, CalculatorPayload, ClockCityView, ClockPayload, ClockSearchHit, EntryPayload,
    NotesPayload, NotesTabRow,
    FileManagerPayload, FmViewMode, IndicatorStatus, ManagedFolderSidebarPayload,
    JyotishPayload, JyotishPlanetRow, MediaPlayerPayload, MoonPayload, NetworkMountPayload,
    PanePayload, PasswordEntryDetailView,
    PasswordEntryView, PasswordManagerPayload, ProcessGroup, ProcessRowView, ProcessSortColumn,
    ProcessesPayload, ProcessesTab, RecentFileItemView, RecentFilesPayload, RssItemView, RssPayload,
    SearchCandidateView, ServiceRowView, StartupRowView, SystemIndicator, SystemIndicatorKind,
    SystemPayload, TabPayload, UniversalSearchPayload, UserRowView, ViewerPayload, WeatherCityEntry,
    WeatherForecastDay, WeatherPayload, WeatherSearchHit, WeatherStatusTag,
};
pub use workspace::{WorkspaceManager, MAX_WORKSPACES};

pub use commands::build_command_set;
pub use manager::operations::CreateWidgetRequest;
pub use manager::{WidgetManager, WidgetManagerOptions};
pub use recent_files::{RecentFileEntry, RecentFilesStore, RecentFilesUpdated};
pub use visibility::visible_instance_ids;

/// Crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
