//! Built-in widget implementations that ship with Orchid.
//!
//! Each submodule contains one concrete widget (a [`crate::Widget`] impl),
//! a provider / data-source, payload conversion, and a helper returning the
//! corresponding [`crate::WidgetDescriptor`].
//!
//! The [`register_all`] / [`register_core`] helpers wire every built-in
//! descriptor into a [`crate::WidgetRegistry`] in one call. Consumers that
//! need additional wiring (search sources, custom media providers) call
//! the per-widget descriptor builders directly.

pub mod calculator;
pub mod calendar;
pub mod clock;
pub mod file_manager;
pub mod jyotish;
pub mod media;
pub mod moon;
pub mod notes;
pub mod password;
pub mod processes;
pub mod recent_files;
pub mod rss;
pub mod search;
pub mod system;
pub mod viewer;
pub mod weather;

use std::sync::Arc;

use crate::error::Result;
use crate::registry::WidgetRegistry;

/// Register every built-in widget that works without user-injected
/// dependencies. Handy for tests / samples. Production bootstraps should
/// prefer [`register_all`] so the password widget and a real search
/// aggregator land in the same registry.
///
/// # Errors
///
/// Propagates duplicate-registration errors from the registry.
pub fn register_core(registry: &WidgetRegistry, http: reqwest::Client) -> Result<()> {
    registry.register(weather::descriptor(http.clone()))?;
    registry.register(moon::descriptor())?;
    registry.register(jyotish::descriptor(http.clone()))?;
    registry.register(clock::descriptor(http.clone()))?;
    registry.register(system::descriptor())?;
    registry.register(processes::descriptor())?;
    registry.register(calculator::descriptor())?;
    registry.register(notes::descriptor())?;
    registry.register(calendar::descriptor())?;
    registry.register(rss::descriptor(http))?;
    registry.register(search::descriptor_stub())?;
    registry.register(media::descriptor())?;
    Ok(())
}

/// Register every built-in widget including the password manager and a
/// real search aggregator.
///
/// # Errors
///
/// Propagates duplicate-registration errors.
#[allow(clippy::too_many_arguments)]
pub fn register_all(
    registry: &WidgetRegistry,
    http: reqwest::Client,
    search_aggregator: Arc<search::SearchAggregator>,
    password_vault: Arc<orchid_crypto::PasswordVault>,
    clipboard: Arc<dyn orchid_crypto::SecureClipboard>,
) -> Result<()> {
    registry.register(weather::descriptor(http.clone()))?;
    registry.register(moon::descriptor())?;
    registry.register(jyotish::descriptor(http.clone()))?;
    registry.register(clock::descriptor(http.clone()))?;
    registry.register(system::descriptor())?;
    registry.register(processes::descriptor())?;
    registry.register(calculator::descriptor())?;
    registry.register(notes::descriptor())?;
    registry.register(calendar::descriptor())?;
    registry.register(rss::descriptor(http))?;
    registry.register(search::descriptor(search_aggregator))?;
    registry.register(media::descriptor())?;
    registry.register(password::descriptor(password_vault, clipboard))?;
    Ok(())
}
