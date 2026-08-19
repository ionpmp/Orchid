//! Payload for the Jyotish (Vedic panchanga) widget.

/// One configured location chip / picker row.
#[derive(Debug, Clone, PartialEq)]
pub struct JyotishCityEntry {
    /// Display name (resolved place label for the synthetic Current row).
    pub name: String,
    /// Whether this is the active location.
    pub active: bool,
    /// Synthetic pinned "Current location" row (not removable).
    pub is_current: bool,
}

/// One birth-profile chip / picker row.
#[derive(Debug, Clone, PartialEq)]
pub struct JyotishProfileEntry {
    /// Display name.
    pub name: String,
    /// Whether this is the active profile.
    pub active: bool,
    /// Whether the profile has a birth date set.
    pub has_birth_data: bool,
}

/// One geocoding search result shown in the location picker.
#[derive(Debug, Clone, PartialEq)]
pub struct JyotishSearchHit {
    /// Place name.
    pub name: String,
    /// Secondary line (region, country).
    pub detail: String,
    /// WGS84 latitude.
    pub latitude: f64,
    /// WGS84 longitude.
    pub longitude: f64,
    /// IANA timezone id when the geocoder provides one.
    pub timezone: String,
}

/// One cell in the birth-profile date picker (day, month, or year view).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct JyotishProfileCalCell {
    /// Day of month, month number (1..=12), year, or day-grid index payload.
    pub value: i32,
    pub is_selected: bool,
    pub is_today: bool,
    /// Adjacent-month day, or a year outside the supported civil range.
    pub is_outside: bool,
}

/// One graha (planet) row for the sidereal table.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct JyotishPlanetRow {
    /// Fluent key for the graha name (`jyotish-graha-*`).
    pub graha_key: &'static str,
    /// Fluent key for the rashi (`jyotish-rashi-*`).
    pub rashi_key: &'static str,
    /// Degrees within the rashi, e.g. `"12°34'"`.
    pub degree_text: String,
    /// Retrograde marker when applicable.
    pub is_retrograde: bool,
}

/// One scored factor row for the day tab (delta + intensity).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct JyotishFactorRow {
    /// Fluent influence key explaining the factor.
    pub label_key: &'static str,
    /// Signed point contribution.
    pub delta: i8,
    /// 0..=100 intensity.
    pub strength: u8,
    /// 0=favorable, 1=unfavorable, 2=neutral.
    pub valence: u8,
}

/// One day chip in the 7-day strip.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct JyotishDayChip {
    pub weekday_key: &'static str,
    pub day_num: u8,
    /// 0=green, 1=yellow, 2=red.
    pub color: u8,
    pub offset: i32,
    pub is_selected: bool,
}

/// One cell in the month grid.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct JyotishMonthCell {
    pub day: u8,
    pub color: u8,
    pub is_today: bool,
    /// Matches the Day-tab selected offset.
    pub is_selected: bool,
    pub offset: i32,
}

/// One month row in the year view.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct JyotishMonthSummary {
    pub month_key: &'static str,
    pub green: u16,
    pub yellow: u16,
    pub red: u16,
    pub month_offset: i32,
}

/// One year row in the life retrospective.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct JyotishYearSummary {
    pub year: i32,
    pub green: u16,
    pub yellow: u16,
    pub red: u16,
    pub dasha_key: &'static str,
    pub year_offset: i32,
    /// Selected for antar expansion on the Life tab.
    pub is_selected: bool,
    /// Civil year equals today.
    pub is_current: bool,
}

/// Current Vimshottari stack for the Day “now” strip.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(missing_docs)]
pub struct JyotishDashaNow {
    pub maha_key: &'static str,
    pub antar_key: &'static str,
    pub pratyantar_key: &'static str,
    pub maha_range: String,
    pub antar_range: String,
    pub pratyantar_range: String,
}

/// One antar-daśā row under an expanded Life year.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct JyotishAntarRow {
    pub lord_key: &'static str,
    pub from_text: String,
    pub to_text: String,
    pub is_current: bool,
}

/// One ranked birth-time candidate for the rectify results step.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct JyotishRectifyCandidate {
    pub range: String,
    pub rashi_key: &'static str,
    pub confidence_pct: u8,
    pub quiz_score: i32,
    pub event_score: i32,
    pub total_score: i32,
    pub is_top: bool,
}

/// Rectification wizard state for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct JyotishRectifyView {
    /// 0=hidden, 1=window, 2=quiz, 3=events, 4=results.
    pub step: u8,
    pub question_idx: u8,
    pub question_total: u8,
    pub question_key: &'static str,
    pub option_keys: Vec<&'static str>,
    /// (kind ftl key, year).
    pub events: Vec<(&'static str, i32)>,
    pub event_kind_keys: Vec<&'static str>,
    pub candidates: Vec<JyotishRectifyCandidate>,
    /// Wizard can navigate backward.
    pub can_go_back: bool,
    /// Closed UI still has a resumable draft session.
    pub has_draft: bool,
    /// Fluent key for the last event validation error (empty if none).
    pub error_key: &'static str,
    /// Show “narrow around best” on results.
    pub can_refine: bool,
    pub event_year_min: i32,
    pub event_year_max: i32,
}

impl Default for JyotishRectifyView {
    fn default() -> Self {
        Self {
            step: 0,
            question_idx: 0,
            question_total: 8,
            question_key: "",
            option_keys: Vec::new(),
            events: Vec::new(),
            event_kind_keys: Vec::new(),
            candidates: Vec::new(),
            can_go_back: false,
            has_draft: false,
            error_key: "",
            can_refine: false,
            event_year_min: 1900,
            event_year_max: 2100,
        }
    }
}

/// Render-ready Jyotish payload.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct JyotishPayload {
    pub date_text: String,
    pub location_name: String,
    /// All configured locations (for chips / picker list).
    pub cities: Vec<JyotishCityEntry>,
    /// Index of the active location in [`Self::cities`].
    pub active_city_index: usize,
    /// Location-picker overlay visibility.
    pub picker_open: bool,
    /// Current location-search query.
    pub search_query: String,
    /// Geocoding hits for the current query.
    pub search_results: Vec<JyotishSearchHit>,
    /// True while a geocoding request is in flight.
    pub search_busy: bool,
    /// True while resolving Current location (GPS/IP).
    pub current_locating: bool,
    /// True when the last Current-location resolve failed (and no cache).
    pub current_failed: bool,
    /// Birth profiles (chips / picker list).
    pub profiles: Vec<JyotishProfileEntry>,
    /// Index of the active profile in [`Self::profiles`].
    pub active_profile_index: usize,
    /// Profile-picker overlay visibility.
    pub profile_picker_open: bool,
    /// Birth-place search query inside the profile picker.
    pub profile_search_query: String,
    /// Geocoding hits for the birth-place search.
    pub profile_search_results: Vec<JyotishSearchHit>,
    /// True while a birth-place geocoding request is in flight.
    pub profile_search_busy: bool,
    /// True when the add/edit profile form is shown.
    pub profile_editing: bool,
    /// Profile index being edited (`-1` = creating a new profile).
    pub profile_edit_index: i32,
    pub profile_edit_name: String,
    /// `0` unspecified, `1` female, `2` male.
    pub profile_edit_gender: u8,
    pub profile_edit_date: String,
    pub profile_edit_time: String,
    pub profile_edit_offset: i32,
    pub profile_edit_place_name: String,
    /// Formatted draft birth-place coordinates (empty when unset).
    pub profile_edit_place_coords: String,
    /// Month key for the draft date calendar (`jyotish-month-N`).
    pub profile_cal_month_key: &'static str,
    pub profile_cal_year: i32,
    /// `0` days, `1` months, `2` twelve-year block.
    pub profile_cal_mode: u8,
    pub profile_cal_cells: Vec<JyotishProfileCalCell>,
    pub ayanamsa_key: &'static str,
    pub ayanamsa_deg_text: String,
    pub day_offset: i32,
    pub is_today: bool,

    pub tithi_key: &'static str,
    pub paksha_key: &'static str,
    pub tithi_end_text: Option<String>,
    pub nakshatra_key: &'static str,
    pub pada: u8,
    pub nakshatra_end_text: Option<String>,
    pub yoga_key: &'static str,
    pub yoga_end_text: Option<String>,
    pub karana_key: &'static str,
    pub karana_end_text: Option<String>,
    pub vara_key: &'static str,

    pub sunrise_time: Option<String>,
    pub sunset_time: Option<String>,
    /// Formatted "HH:MM–HH:MM" local ranges when sunrise/sunset known.
    pub rahukalam_text: Option<String>,
    pub yamagandam_text: Option<String>,
    pub gulika_text: Option<String>,
    /// True when `calculated_at` falls inside Rahu Kalam.
    pub in_rahukalam: bool,

    pub planets: Vec<JyotishPlanetRow>,
    pub show_planets: bool,
    pub is_loading: bool,

    pub active_tab: u8,
    /// Primary traffic-light color (now when viewing today, else day).
    pub score_color: u8,
    /// Instantaneous score color at the selected sample time.
    pub now_score_color: u8,
    /// Whole-day (local noon) score color.
    pub day_score_color: u8,
    /// Primary numeric score 0..=100.
    pub score_value: u8,
    /// Ranked factor contributions for the transparent score breakdown.
    pub factors: Vec<JyotishFactorRow>,
    /// Natal layers active for this payload.
    pub personal_mode: bool,
    pub headline_key: &'static str,
    /// Day-tab "what's happening" prose key (`jyotish-summary-…`).
    pub summary_key: &'static str,
    pub influence_keys: Vec<&'static str>,
    pub advice_keys: Vec<&'static str>,
    pub week_strip: Vec<JyotishDayChip>,
    pub month_key: &'static str,
    pub month_year: i32,
    pub month_cells: Vec<JyotishMonthCell>,
    pub month_first_weekday: u8,
    pub month_green: u16,
    pub month_yellow: u16,
    pub month_red: u16,
    pub year_value: i32,
    pub year_months: Vec<JyotishMonthSummary>,
    pub life_years: Vec<JyotishYearSummary>,
    /// Absolute year expanded on the Life tab (`0` = none).
    pub life_detail_year: i32,
    pub life_antars: Vec<JyotishAntarRow>,
    /// Present when birth data yields a Vimshottari stack for the selected day.
    pub has_dasha_now: bool,
    pub dasha_now: JyotishDashaNow,
    /// Soft gochara note for the active year/month context (empty when unused).
    pub gochara_note_key: &'static str,
    pub has_birth_data: bool,
    pub rectify: JyotishRectifyView,
}
