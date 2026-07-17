//! Persistent config for the calculator widget.

#![allow(missing_docs)]

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use super::engine::{AngleMode, CalcMode};

/// Persistent calculator-widget config.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct CalculatorConfig {
    /// `0` = Standard, `1` = Scientific.
    pub mode: u8,
    /// `0` = DEG, `1` = RAD, `2` = GRAD.
    pub angle_mode: u8,
    /// Show history panel when space allows.
    pub show_history: bool,
}

impl Default for CalculatorConfig {
    fn default() -> Self {
        Self {
            mode: CalcMode::Standard as u8,
            angle_mode: AngleMode::Degrees as u8,
            show_history: true,
        }
    }
}

impl CalculatorConfig {
    #[must_use]
    pub fn calc_mode(&self) -> CalcMode {
        CalcMode::from_index(i32::from(self.mode))
    }

    #[must_use]
    pub fn angle(&self) -> AngleMode {
        AngleMode::from_index(i32::from(self.angle_mode))
    }
}
