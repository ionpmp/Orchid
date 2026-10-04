//! Persisted choices for the protection widget.

use serde::{Deserialize, Serialize};

use super::catalog::CleanerId;
use super::firewall::block_refusal;
use super::wipe::clamp_passes;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectConfig {
    /// Cleaner ids the user wants included.
    pub enabled: Vec<String>,
    /// Executables currently blocked from outbound traffic.
    pub blocked: Vec<String>,
    /// Free-space overwrite passes (1 or 3).
    pub passes: u8,
    /// Mount point chosen for a free-space wipe, such as `C:\`.
    pub drive: String,
}

impl Default for ProtectConfig {
    fn default() -> Self {
        Self {
            enabled: CleanerId::ALL
                .iter()
                .copied()
                .filter(|id| id.default_on())
                .map(CleanerId::as_str)
                .map(str::to_string)
                .collect(),
            blocked: Vec::new(),
            passes: 1,
            drive: String::new(),
        }
    }
}

impl ProtectConfig {
    pub fn normalize(&mut self) {
        self.passes = clamp_passes(self.passes);
        self.enabled.retain(|id| CleanerId::parse(id).is_some());
        self.blocked
            .retain(|path| block_refusal(std::path::Path::new(path)).is_none());
    }

    pub fn is_enabled(&self, id: CleanerId) -> bool {
        self.enabled.iter().any(|row| row == id.as_str())
    }

    pub fn set_enabled(&mut self, id: CleanerId, on: bool) {
        self.enabled.retain(|row| row != id.as_str());
        if on {
            self.enabled.push(id.as_str().to_string());
        }
    }
}
