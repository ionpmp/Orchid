//! Built-in widget payload variants.
//!
//! Each widget type has its own module defining a `*Payload` struct plus any
//! view-level support types. The variants are referenced from
//! [`crate::widget::snapshot::WidgetPayload`] and consumed exhaustively by
//! the UI renderer.

pub mod agent;
pub mod audio_player;
pub mod browser;
pub mod calculator;
pub mod calendar;
pub mod clock;
pub mod file_manager;
pub mod jyotish;
pub mod mail;
pub mod media;
pub mod moon;
pub mod notes;
pub mod optimize;
pub mod password;
pub mod processes;
pub mod protect;
pub mod recent_files;
pub mod rss;
pub mod search;
pub mod system;
pub mod video_player;
pub mod viewer;
pub mod weather;

pub use agent::{AgentLine, AgentPayload};
pub use audio_player::{
    AudioPlayerGroupRow, AudioPlayerLyricRow, AudioPlayerPayload, AudioPlayerPlaylistRow,
    AudioPlayerRootRow, AudioPlayerTrackRow,
};
pub use browser::{BrowserBookmarkRow, BrowserDownloadRow, BrowserPayload, BrowserTabRow};
pub use calculator::{CalcHistoryRow, CalculatorPayload};
pub use calendar::{CalendarDayCell, CalendarEventRow, CalendarPayload, CalendarUpcomingRow};
pub use clock::{ClockCityView, ClockPayload, ClockSearchHit};
pub use file_manager::{
    EntryPayload, FileManagerPayload, FmViewMode, ManagedFolderSidebarPayload, NetworkMountPayload,
    PanePayload, TabPayload, VisitHistoryItemPayload,
};
pub use jyotish::{
    JyotishAntarRow, JyotishCityEntry, JyotishDashaNow, JyotishDayChip, JyotishFactorRow,
    JyotishMonthCell, JyotishMonthSummary, JyotishPayload, JyotishPlanetRow, JyotishProfileCalCell,
    JyotishProfileEntry, JyotishRectifyCandidate, JyotishRectifyView, JyotishSearchHit,
    JyotishYearSummary,
};
pub use mail::{MailAccountRow, MailFolderRow, MailMessageRow, MailPayload};
pub use media::MediaPlayerPayload;
pub use moon::MoonPayload;
pub use notes::{NotesPayload, NotesTabRow};
pub use optimize::{OptimizePayload, OptimizeRow};
pub use password::{
    PasswordEntryDetailView, PasswordEntryView, PasswordGroupView, PasswordManagerPayload,
};
pub use processes::{
    ProcessGroup, ProcessRowView, ProcessSortColumn, ProcessesPayload, ProcessesTab,
    ServiceRowView, StartupRowView, UserRowView,
};
pub use protect::{ProtectDrive, ProtectPayload, ProtectRow};
pub use recent_files::{RecentFileItemView, RecentFilesPayload};
pub use rss::{RssItemView, RssPayload};
pub use search::{SearchCandidateView, UniversalSearchPayload};
pub use system::{IndicatorStatus, SystemIndicator, SystemIndicatorKind, SystemPayload};
pub use video_player::{
    VideoPlayerGroupRow, VideoPlayerItemRow, VideoPlayerPayload, VideoPlayerRootRow,
};
pub use viewer::ViewerPayload;
pub use weather::{
    WeatherCityEntry, WeatherForecastDay, WeatherPayload, WeatherSearchHit, WeatherStatusTag,
};
