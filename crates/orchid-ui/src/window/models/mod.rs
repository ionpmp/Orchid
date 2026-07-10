//! Slint model builders for workspace widget frames.

mod media;
mod moon;
mod password;
mod recent;
mod rss;
mod search;
mod system;
mod viewer;
mod weather;

pub(crate) use media::{build_media_model, empty_media_model};
pub(crate) use moon::{build_moon_model, empty_moon_model};
pub(crate) use password::{build_password_model, empty_password_model, PasswordAddDialogOverlay};
pub(crate) use recent::{build_recent_files_model, empty_recent_files_model};
pub(crate) use rss::{build_rss_model, empty_rss_model};
pub(crate) use search::{build_search_model, empty_search_model};
pub(crate) use system::{build_system_model, empty_system_model};
pub(crate) use viewer::{build_viewer_model, empty_viewer_model};
pub(crate) use weather::{build_weather_model, empty_weather_model};
