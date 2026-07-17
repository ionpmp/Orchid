//! Clock / world-clocks widget — local time plus configurable IANA zones.

pub mod config;

use std::str::FromStr;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{Local, Utc};
use chrono_tz::Tz;
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use tracing::warn;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{ClockCityView, ClockPayload, ClockSearchHit};
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, LocaleConfig, WidgetSize};

use super::weather::provider::{GeocodingHit, OpenMeteoProvider, WeatherProvider};

pub use config::{decode_config, ClockCity, ClockConfig};

/// Stable type id.
pub const TYPE_ID: &str = "clock";

fn job_interval(show_seconds: bool) -> Duration {
    if show_seconds {
        Duration::from_secs(1)
    } else {
        Duration::from_secs(15)
    }
}

static CLOCK_LIVE: LazyLock<DashMap<Uuid, Arc<ClockHandle>>> = LazyLock::new(DashMap::new);

#[derive(Clone, Default)]
struct UiState {
    picker_open: bool,
    search_query: String,
    search_results: Vec<GeocodingHit>,
    search_busy: bool,
    search_generation: u64,
    pending_notice: Option<&'static str>,
}

struct ClockHandle {
    instance_id: Uuid,
    config: Arc<RwLock<ClockConfig>>,
    ui: Arc<RwLock<UiState>>,
    provider: Arc<dyn WeatherProvider>,
    refresh: Mutex<PeriodicRefresh>,
    bus: Arc<orchid_core::EventBus>,
    orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
}

impl ClockHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn schedule_refresh(self: &Arc<Self>) {
        let show_seconds = self.config.read().show_seconds;
        let interval = job_interval(show_seconds);
        let mut refresh = self.refresh.lock();
        refresh.set_interval(interval);
        let handle = Arc::clone(self);
        refresh.start(move || {
            let handle = Arc::clone(&handle);
            async move {
                if handle.ui.read().picker_open {
                    return;
                }
                handle.publish();
            }
        });
    }

    fn stop_refresh(&self) {
        self.refresh.lock().stop();
    }

    fn set_picker_open(&self, open: bool) {
        let mut ui = self.ui.write();
        ui.picker_open = open;
        if !open {
            ui.search_query.clear();
            ui.search_results.clear();
            ui.search_busy = false;
        }
        drop(ui);
        self.publish();
    }

    fn remove_city(&self, index: usize) {
        // UI city list prepends the local row at index 0.
        if index == 0 {
            return;
        }
        let cfg_index = index - 1;
        {
            let mut cfg = self.config.write();
            if cfg_index < cfg.cities.len() {
                cfg.cities.remove(cfg_index);
            }
        }
        self.publish();
    }

    fn move_city(&self, index: usize, delta: i32) {
        if index == 0 || delta == 0 {
            return;
        }
        let cfg_index = index - 1;
        {
            let mut cfg = self.config.write();
            let len = cfg.cities.len();
            if cfg_index >= len {
                return;
            }
            let dest = cfg_index as i32 + delta;
            if dest < 0 || dest as usize >= len {
                return;
            }
            cfg.cities.swap(cfg_index, dest as usize);
        }
        self.publish();
    }

    fn add_city(&self, name: String, timezone: String) {
        let tz = timezone.trim().to_string();
        if tz.is_empty() || Tz::from_str(&tz).is_err() {
            return;
        }
        {
            let mut cfg = self.config.write();
            if let Some(existing) = cfg.cities.iter().position(|c| c.timezone == tz) {
                // Already present — bump it to the end so the user sees it.
                let city = cfg.cities.remove(existing);
                cfg.cities.push(city);
            } else {
                cfg.cities.push(ClockCity {
                    name: if name.trim().is_empty() {
                        tz.clone()
                    } else {
                        name.trim().to_string()
                    },
                    timezone: tz,
                });
            }
            cfg.normalize();
        }
        {
            let mut ui = self.ui.write();
            ui.picker_open = false;
            ui.search_query.clear();
            ui.search_results.clear();
            ui.search_busy = false;
        }
        self.publish();
    }

    fn search_cities(&self, query: String) {
        let generation = {
            let mut ui = self.ui.write();
            ui.search_query = query.clone();
            ui.search_generation = ui.search_generation.wrapping_add(1);
            ui.search_busy = !query.trim().is_empty();
            if query.trim().is_empty() {
                ui.search_results.clear();
                ui.search_busy = false;
            }
            ui.search_generation
        };
        self.publish();
        if query.trim().is_empty() {
            return;
        }

        let provider = self.provider.clone();
        let ui = self.ui.clone();
        let bus = self.bus.clone();
        let instance_id = self.instance_id;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(280)).await;
            if ui.read().search_generation != generation {
                return;
            }
            let result = provider.search_cities(&query).await;
            let mut slot = ui.write();
            if slot.search_generation != generation {
                return;
            }
            slot.search_busy = false;
            match result {
                Ok(hits) => {
                    // Prefer hits that include an IANA timezone.
                    slot.search_results = hits
                        .into_iter()
                        .filter(|h| {
                            h.timezone
                                .as_deref()
                                .is_some_and(|tz| !tz.is_empty() && Tz::from_str(tz).is_ok())
                        })
                        .collect();
                }
                Err(e) => {
                    warn!(%instance_id, error = %e, "clock geocoding failed");
                    slot.search_results.clear();
                    slot.pending_notice = Some("clock-error-geocoding");
                }
            }
            drop(slot);
            bus.publish(
                orchid_core::EventSource::Widget(instance_id),
                WidgetSnapshotUpdated { instance_id },
            );
        });
    }
}

/// Open / close the city picker overlay.
pub fn set_picker_open(instance_id: Uuid, open: bool) {
    if let Some(h) = CLOCK_LIVE.get(&instance_id) {
        if open {
            h.stop_refresh();
        }
        h.set_picker_open(open);
        if !open {
            Arc::clone(&h).schedule_refresh();
        }
    }
}

/// Remove a configured city.
pub fn remove_city(instance_id: Uuid, index: usize) {
    if let Some(h) = CLOCK_LIVE.get(&instance_id) {
        h.remove_city(index);
    }
}

pub fn move_city(instance_id: Uuid, index: usize, delta: i32) {
    if let Some(h) = CLOCK_LIVE.get(&instance_id) {
        h.move_city(index, delta);
    }
}

pub fn take_notice(instance_id: Uuid) -> Option<&'static str> {
    CLOCK_LIVE.get(&instance_id).and_then(|h| h.ui.write().pending_notice.take())
}

/// Update the city-search query and kick off geocoding.
pub fn search_cities(instance_id: Uuid, query: String) {
    if let Some(h) = CLOCK_LIVE.get(&instance_id) {
        h.search_cities(query);
    }
}

/// Add a city from a geocoding hit.
pub fn add_city(instance_id: Uuid, name: String, timezone: String) {
    if let Some(h) = CLOCK_LIVE.get(&instance_id) {
        h.add_city(name, timezone);
    }
}

/// Snapshot the live clock config for the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<ClockConfig> {
    CLOCK_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Apply a settings-dialog mutation to the live clock config.
pub fn update_config(instance_id: Uuid, mutate: impl FnOnce(&mut ClockConfig)) {
    let Some(h) = CLOCK_LIVE.get(&instance_id) else {
        return;
    };
    let before_seconds = h.config.read().show_seconds;
    {
        let mut cfg = h.config.write();
        mutate(&mut cfg);
        cfg.normalize();
    }
    let after_seconds = h.config.read().show_seconds;
    h.publish();
    if before_seconds != after_seconds && h.refresh.lock().is_running() {
        h.schedule_refresh();
    }
}

/// Clock widget implementation.
pub struct ClockWidget {
    instance_id: Uuid,
    handle: Arc<ClockHandle>,
}

impl std::fmt::Debug for ClockWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClockWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl ClockWidget {
    /// Construct a clock widget.
    pub fn new(
        instance_id: Uuid,
        config: ClockConfig,
        provider: Arc<dyn WeatherProvider>,
        bus: Arc<orchid_core::EventBus>,
        orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    ) -> Self {
        let mut config = config;
        config.normalize();
        let interval = job_interval(config.show_seconds);
        let handle = Arc::new(ClockHandle {
            instance_id,
            config: Arc::new(RwLock::new(config)),
            ui: Arc::new(RwLock::new(UiState::default())),
            provider,
            refresh: Mutex::new(PeriodicRefresh::new(interval)),
            bus,
            orchid_config,
        });
        CLOCK_LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
        }
    }
}

#[async_trait]
impl Widget for ClockWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }

    fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.schedule_refresh();
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.stop_refresh();
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.stop_refresh();
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.stop_refresh();
        CLOCK_LIVE.remove(&self.instance_id);
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let cfg = self.handle.config.read().clone();
        let ui = self.handle.ui.read().clone();
        let locale = self.handle.orchid_config.read().locale.clone();
        let payload = render_payload(&cfg, &ui, &locale);
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: String::new(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Clock(payload),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let cfg = decode_config(bytes)?;
        *self.handle.config.write() = cfg;
        Ok(())
    }

    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Small),
            max_size: None,
            preferred_size: Some(WidgetSize::Medium),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: true,
        }
    }
}

fn render_payload(cfg: &ClockConfig, ui: &UiState, locale: &LocaleConfig) -> ClockPayload {
    let now = Utc::now();
    let local_now = now.with_timezone(&Local);
    let local_date = local_now.date_naive();

    let time_fmt = time_format(locale, cfg.show_seconds);
    let date_fmt = locale
        .date_format
        .as_deref()
        .unwrap_or("%a, %b %d");

    let local_time = local_now.format(&time_fmt).to_string();
    let local_date_text = if cfg.show_dates {
        local_now.format(date_fmt).to_string()
    } else {
        String::new()
    };
    let local_timezone = local_tz_name();
    let local_offset_secs = offset_secs_local(&local_now);
    let local_offset = format_utc_offset(local_offset_secs);

    let mut cities = Vec::with_capacity(cfg.cities.len() + 1);
    cities.push(ClockCityView {
        name: String::new(), // UI fills with "Local" via i18n
        timezone: local_timezone.clone(),
        time_text: local_time.clone(),
        date_text: local_date_text.clone(),
        offset_text: if cfg.show_offsets {
            local_offset
        } else {
            String::new()
        },
        day_offset: 0,
        is_local: true,
    });

    for city in &cfg.cities {
        match Tz::from_str(&city.timezone) {
            Ok(tz) => {
                let zoned = now.with_timezone(&tz);
                let day_offset =
                    (zoned.date_naive() - local_date).num_days().clamp(-1, 1) as i8;
                cities.push(ClockCityView {
                    name: city.name.clone(),
                    timezone: city.timezone.clone(),
                    time_text: zoned.format(&time_fmt).to_string(),
                    date_text: if cfg.show_dates {
                        zoned.format(date_fmt).to_string()
                    } else {
                        String::new()
                    },
                    offset_text: if cfg.show_offsets {
                        format_relative_offset(local_offset_secs, offset_secs_zoned(&zoned))
                    } else {
                        String::new()
                    },
                    day_offset,
                    is_local: false,
                });
            }
            Err(_) => {
                cities.push(ClockCityView {
                    name: city.name.clone(),
                    timezone: city.timezone.clone(),
                    time_text: "—".into(),
                    date_text: String::new(),
                    offset_text: String::new(),
                    day_offset: 0,
                    is_local: false,
                });
            }
        }
    }

    let search_results = ui
        .search_results
        .iter()
        .filter_map(|h| {
            let timezone = h.timezone.clone().unwrap_or_default();
            if timezone.is_empty() {
                return None;
            }
            Some(ClockSearchHit {
                name: h.name.clone(),
                detail: h.detail.clone(),
                timezone,
            })
        })
        .collect();

    ClockPayload {
        local_time,
        local_date: local_date_text,
        local_timezone,
        cities,
        picker_open: ui.picker_open,
        search_query: ui.search_query.clone(),
        search_results,
        search_busy: ui.search_busy,
    }
}

fn time_format(locale: &LocaleConfig, show_seconds: bool) -> String {
    let base = locale.time_format.as_deref().unwrap_or("%H:%M");
    if !show_seconds {
        return base.to_string();
    }
    if base.contains("%S") {
        return base.to_string();
    }
    // Append seconds before am/pm markers when present.
    if let Some(idx) = base.find("%p").or_else(|| base.find("%P")) {
        let mut s = String::with_capacity(base.len() + 3);
        s.push_str(&base[..idx]);
        if !s.ends_with(':') && !s.ends_with(' ') {
            s.push(':');
        }
        if !s.ends_with("%S") && !s.ends_with("%S ") {
            // insert :%S before am/pm
            if s.ends_with(':') {
                s.push_str("%S ");
            } else {
                s.push_str(":%S ");
            }
        }
        s.push_str(&base[idx..]);
        return s;
    }
    format!("{base}:%S")
}

fn format_utc_offset(secs: i32) -> String {
    let hours = secs / 3600;
    let mins = (secs.abs() % 3600) / 60;
    if mins == 0 {
        format!("UTC{hours:+}")
    } else {
        format!("UTC{hours:+}:{mins:02}")
    }
}

fn format_relative_offset(local_secs: i32, zone_secs: i32) -> String {
    let diff = zone_secs - local_secs;
    if diff == 0 { return "\u{00b1}0".into(); }
    let hours = diff / 3600;
    let mins = (diff.abs() % 3600) / 60;
    if mins == 0 { return format!("{hours:+}h"); }
    format!("{hours:+}:{mins:02}")
}

fn offset_secs_local(dt: &chrono::DateTime<Local>) -> i32 {
    (dt.naive_local() - dt.naive_utc()).num_seconds() as i32
}

fn offset_secs_zoned(dt: &chrono::DateTime<Tz>) -> i32 {
    (dt.naive_local() - dt.naive_utc()).num_seconds() as i32
}

fn local_tz_name() -> String {
    iana_time_zone::get_timezone().unwrap_or_default()
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor(http_client: reqwest::Client) -> WidgetDescriptor {
    let provider: Arc<dyn WeatherProvider> = Arc::new(OpenMeteoProvider::new(http_client));
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, state_bytes| {
        let cfg = match state_bytes {
            Some(bytes) => decode_config(bytes).unwrap_or_default(),
            None => ClockConfig::default(),
        };
        Ok(Box::new(ClockWidget::new(
            ctx.instance_id,
            cfg,
            provider.clone(),
            ctx.bus.clone(),
            ctx.config.clone(),
        )) as Box<dyn Widget>)
    });

    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-clock-name",
        description_key: "widget-clock-desc",
        icon_name: "clock",
        category: WidgetCategory::Information,
        default_size: WidgetSize::Medium,
        min_size: Some(WidgetSize::Small),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cities_parse_as_iana() {
        for city in &ClockConfig::default().cities {
            assert!(
                Tz::from_str(&city.timezone).is_ok(),
                "invalid tz {}",
                city.timezone
            );
        }
    }

    #[test]
    fn time_format_adds_seconds() {
        let locale = LocaleConfig {
            time_format: Some("%H:%M".into()),
            ..LocaleConfig::default()
        };
        assert_eq!(time_format(&locale, true), "%H:%M:%S");
        assert_eq!(time_format(&locale, false), "%H:%M");
    }

    #[test]
    fn format_relative_offset_same_zone() {
        assert_eq!(format_relative_offset(7 * 3600, 7 * 3600), "\u{00b1}0");
        assert_eq!(format_relative_offset(0, 5 * 3600), "+5h");
        assert_eq!(format_relative_offset(0, -2 * 3600 - 30 * 60), "-2:30");
    }

    #[test]
    fn format_utc_offset_hours_and_minutes() {
        assert_eq!(format_utc_offset(7 * 3600), "UTC+7");
        assert_eq!(format_utc_offset(-5 * 3600), "UTC-5");
        assert_eq!(format_utc_offset(5 * 3600 + 30 * 60), "UTC+5:30");
    }
}
