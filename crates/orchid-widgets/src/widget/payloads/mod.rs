//! Built-in widget payload variants.
//!
//! Each widget type has its own module defining a `*Payload` struct plus any
//! view-level support types. The variants are referenced from
//! [`crate::widget::snapshot::WidgetPayload`] and consumed exhaustively by
//! the UI renderer.

pub mod file_manager;
pub mod media;
pub mod moon;
pub mod password;
pub mod rss;
pub mod search;
pub mod system;
pub mod viewer;
pub mod weather;

pub use file_manager::{
    EntryPayload, FileManagerPayload, FmViewMode, PanePayload, TabPayload,
};
pub use media::MediaPlayerPayload;
pub use moon::MoonPayload;
pub use password::{PasswordEntryDetailView, PasswordEntryView, PasswordManagerPayload};
pub use rss::{RssItemView, RssPayload};
pub use search::{SearchCandidateView, UniversalSearchPayload};
pub use system::{IndicatorStatus, SystemIndicator, SystemPayload};
pub use viewer::ViewerPayload;
pub use weather::{WeatherForecastDay, WeatherPayload, WeatherStatusTag};
