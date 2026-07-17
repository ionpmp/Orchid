//! Payload for the calculator widget.

#![allow(missing_docs)]

/// One history row for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalcHistoryRow {
    pub expression: String,
    pub result: String,
}

/// Render payload for the calculator widget.
#[derive(Debug, Clone)]
pub struct CalculatorPayload {
    /// `0` = Standard, `1` = Scientific.
    pub mode: i32,
    /// `0` = DEG, `1` = RAD, `2` = GRAD.
    pub angle: i32,
    pub second: bool,
    /// Raw engine display (digit string or error i18n key).
    pub display: String,
    /// Expression / pending-op line.
    pub expression: String,
    pub memory_set: bool,
    /// Fluent key when display is an error; `None` for normal numbers.
    pub error_key: Option<&'static str>,
    pub history: Vec<CalcHistoryRow>,
    pub show_history: bool,
}
