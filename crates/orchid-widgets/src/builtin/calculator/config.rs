//! Persistent config for the calculator widget.

#![allow(missing_docs)]

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use super::engine::{AngleMode, CalcMode, HistoryEntry};

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct PersistedHistoryEntry {
    pub expression: String,
    pub result: String,
    pub value: f64,
}

impl From<&HistoryEntry> for PersistedHistoryEntry {
    fn from(h: &HistoryEntry) -> Self {
        Self { expression: h.expression.clone(), result: h.result.clone(), value: h.value }
    }
}

impl From<PersistedHistoryEntry> for HistoryEntry {
    fn from(h: PersistedHistoryEntry) -> Self {
        Self { expression: h.expression, result: h.result, value: h.value }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct CalculatorConfig {
    pub mode: u8,
    pub angle_mode: u8,
    pub show_history: bool,
    #[serde(default)]
    pub memory: f64,
    #[serde(default)]
    pub memory_set: bool,
    #[serde(default)]
    pub history: Vec<PersistedHistoryEntry>,
}

impl Default for CalculatorConfig {
    fn default() -> Self {
        Self {
            mode: CalcMode::Standard as u8,
            angle_mode: AngleMode::Degrees as u8,
            show_history: true,
            memory: 0.0,
            memory_set: false,
            history: Vec::new(),
        }
    }
}

impl CalculatorConfig {
    #[must_use]
    pub fn calc_mode(&self) -> CalcMode { CalcMode::from_index(i32::from(self.mode)) }
    #[must_use]
    pub fn angle(&self) -> AngleMode { AngleMode::from_index(i32::from(self.angle_mode)) }
}
