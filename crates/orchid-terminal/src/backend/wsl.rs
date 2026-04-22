//! WSL backend helpers.

use std::process::Command;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::error::{Result, TerminalError};

/// Cached result of `wsl.exe -l --quiet`. The CLI call is slow (~200 ms) and
/// the list rarely changes, so we reuse a snapshot for 30 seconds.
static DISTRO_CACHE: Mutex<Option<(Instant, Vec<String>)>> = Mutex::new(None);

/// Validate that a distro name is non-empty.
pub(crate) fn validate_distro(distro: &str) -> Result<()> {
    if distro.trim().is_empty() {
        return Err(TerminalError::BackendUnavailable(
            "WSL distro not specified".into(),
        ));
    }
    Ok(())
}

/// List available WSL distributions. Returns an empty list when WSL is not
/// installed (best-effort — we don't distinguish "not installed" from
/// "install in a weird state").
///
/// # Errors
///
/// Propagates I/O errors from spawning `wsl.exe`.
///
/// # Examples
///
/// ```no_run
/// let distros = orchid_terminal::backend::wsl::list_wsl_distros()
///     .unwrap_or_default();
/// for d in distros {
///     println!("{d}");
/// }
/// ```
pub fn list_wsl_distros() -> Result<Vec<String>> {
    {
        let cache = DISTRO_CACHE.lock();
        if let Some((when, list)) = cache.as_ref() {
            if when.elapsed() < Duration::from_secs(30) {
                return Ok(list.clone());
            }
        }
    }
    let output = match Command::new("wsl.exe").args(["-l", "--quiet"]).output() {
        Ok(o) => o,
        Err(_) => {
            // wsl.exe missing or inaccessible: behave as if no distros.
            let mut cache = DISTRO_CACHE.lock();
            *cache = Some((Instant::now(), Vec::new()));
            return Ok(Vec::new());
        }
    };
    // Windows encodes wsl.exe output as UTF-16LE.
    let text = match utf16le_to_string(&output.stdout) {
        Some(s) => s,
        None => String::from_utf8_lossy(&output.stdout).into_owned(),
    };
    let list: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let mut cache = DISTRO_CACHE.lock();
    *cache = Some((Instant::now(), list.clone()));
    Ok(list)
}

fn utf16le_to_string(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 || bytes.len() % 2 != 0 {
        return None;
    }
    // Skip BOM if present.
    let start = if bytes[0] == 0xFF && bytes[1] == 0xFE { 2 } else { 0 };
    let mut units: Vec<u16> = Vec::with_capacity((bytes.len() - start) / 2);
    for pair in bytes[start..].chunks_exact(2) {
        units.push(u16::from_le_bytes([pair[0], pair[1]]));
    }
    String::from_utf16(&units).ok()
}
