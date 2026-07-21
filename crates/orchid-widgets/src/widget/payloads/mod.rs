//! Built-in widget payload variants.
//!
//! Each widget type has its own module defining a `*Payload` struct plus any
//! view-level support types. The variants are referenced from
//! [`crate::widget::snapshot::WidgetPayload`] and consumed exhaustively by
//! the UI renderer.

pub mod calculator;
pub mod clock;
pub mod notes;
pub mod file_manager;
pub mod media;
pub mod moon;
pub mod password;
pub mod processes;
pub mod recent_files;
pub mod rss;
pub mod search;
pub mod system;
pub mod viewer;
pub mod weather;

pub use calculator::{CalcHistoryRow, CalculatorPayload};
pub use clock::{ClockCityView, ClockPayload, ClockSearchHit};
pub use notes::{NotesPayload, NotesTabRow};
pub use file_manager::{
    EntryPayload, FileManagerPayload, FmViewMode, ManagedFolderSidebarPayload, NetworkMountPayload,
    PanePayload, TabPayload,
};
pub use media::MediaPlayerPayload;
pub use moon::MoonPayload;
pub use password::{PasswordEntryDetailView, PasswordEntryView, PasswordManagerPayload};
pub use processes::{
    ProcessGroup, ProcessRowView, ProcessSortColumn, ProcessesPayload, ProcessesTab, ServiceRowView,
    StartupRowView, UserRowView,
};
pub use recent_files::{RecentFileItemView, RecentFilesPayload};
pub use rss::{RssItemView, RssPayload};
pub use search::{SearchCandidateView, UniversalSearchPayload};
pub use system::{IndicatorStatus, SystemIndicator, SystemIndicatorKind, SystemPayload};
pub use viewer::ViewerPayload;
pub use weather::{
    WeatherCityEntry, WeatherForecastDay, WeatherPayload, WeatherSearchHit, WeatherStatusTag,
};
