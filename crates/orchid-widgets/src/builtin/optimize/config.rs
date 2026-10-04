//! Which tab the Optimize widget last showed.

use serde::{Deserialize, Serialize};

use super::catalog::TAB_COUNT;

/// Persisted Optimize widget state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizeConfig {
    /// Selected tab, `0..TAB_COUNT`.
    pub tab: u8,
}

impl Default for OptimizeConfig {
    fn default() -> Self {
        Self { tab: 0 }
    }
}

impl OptimizeConfig {
    /// Clamp the tab into range.
    pub fn normalize(&mut self) {
        if self.tab >= TAB_COUNT {
            self.tab = 0;
        }
    }
}
