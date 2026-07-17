//! Payload for the clock / world-clocks widget.

/// One configured city (or the local row) ready for the UI.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct ClockCityView {
    pub name: String,
    pub timezone: String,
    pub time_text: String,
    pub date_text: String,
    pub offset_text: String,
    /// `-1` yesterday, `0` same calendar day as local, `1` tomorrow.
    pub day_offset: i8,
    pub is_local: bool,
}

/// One geocoding hit for the city picker.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct ClockSearchHit {
    pub name: String,
    pub detail: String,
    pub timezone: String,
}

/// Render-ready clock payload.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct ClockPayload {
    pub local_time: String,
    pub local_date: String,
    pub local_timezone: String,
    pub cities: Vec<ClockCityView>,
    pub picker_open: bool,
    pub search_query: String,
    pub search_results: Vec<ClockSearchHit>,
    pub search_busy: bool,
}
