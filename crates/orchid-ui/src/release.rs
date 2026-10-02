//! GitHub release comparison and the anonymous app-start event.
//!
//! Update checks read the public releases API and never download a binary.
//! Telemetry records version, OS family, and language. The event leaves the
//! machine only when the endpoint is `https`.

use std::cmp::Ordering;
use std::io::Write;
use std::path::Path;

use serde::Serialize;

/// Latest-release document for `ionpmp/Orchid`.
pub const RELEASES_LATEST: &str = "https://api.github.com/repos/ionpmp/Orchid/releases/latest";

/// What a release lookup means for this build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseOffer {
    /// `candidate` is newer than the running version.
    Newer {
        /// Tag as published, without a leading `v`.
        version: String,
        /// Release page, only when it stays on this repository.
        page: Option<String>,
    },
    /// The running version is the latest published release.
    Current,
    /// GitHub has no published release.
    None,
}

/// Where an opt-in telemetry event may go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelemetryTarget {
    /// Record on disk only.
    LocalOnly,
    /// POST the JSON here. Redirects are not followed.
    Https(String),
    /// The endpoint is set but is not an `https` URL with a host.
    Rejected,
}

/// Anonymous app-start payload. No paths, names, or hostnames.
#[derive(Debug, Serialize)]
pub struct TelemetryEvent<'a> {
    /// Stable event name.
    pub event: &'a str,
    /// `CARGO_PKG_VERSION` of the running build.
    pub version: &'a str,
    /// `windows`, `macos`, `linux`, or `other`.
    pub os: &'a str,
    /// Configured BCP 47 language tag.
    pub language: &'a str,
}

/// `true` when `candidate` is a newer semver than `current`.
#[must_use]
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let Some((cand_nums, cand_pre)) = version_key(candidate) else {
        return false;
    };
    let Some((cur_nums, cur_pre)) = version_key(current) else {
        return false;
    };
    let len = cand_nums.len().max(cur_nums.len());
    for i in 0..len {
        let a = cand_nums.get(i).copied().unwrap_or(0);
        let b = cur_nums.get(i).copied().unwrap_or(0);
        match a.cmp(&b) {
            Ordering::Greater => return true,
            Ordering::Less => return false,
            Ordering::Equal => {}
        }
    }
    match (cand_pre, cur_pre) {
        (None, Some(_)) => true,
        (Some(a), Some(b)) => pre_cmp(&a, &b) == Ordering::Greater,
        _ => false,
    }
}

/// Interpret a GitHub releases response. Non-200/404 statuses are errors.
pub fn interpret_release(status: u16, body: &str, current: &str) -> Result<ReleaseOffer, ()> {
    if status == 404 {
        return Ok(ReleaseOffer::None);
    }
    if status != 200 {
        return Err(());
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| ())?;
    let Some(tag) = value.get("tag_name").and_then(|v| v.as_str()) else {
        return Err(());
    };
    let version = tag.trim().trim_start_matches(['v', 'V']).to_string();
    if version.is_empty() || !is_newer(&version, current) {
        return Ok(ReleaseOffer::Current);
    }
    let page = value
        .get("html_url")
        .and_then(|v| v.as_str())
        .and_then(trusted_release_page);
    Ok(ReleaseOffer::Newer { version, page })
}

/// Classify a telemetry endpoint. Empty means local only.
#[must_use]
pub fn telemetry_target(endpoint: &str) -> TelemetryTarget {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return TelemetryTarget::LocalOnly;
    }
    let Ok(url) = url::Url::parse(endpoint) else {
        return TelemetryTarget::Rejected;
    };
    if url.scheme() == "https" && url.host_str().is_some() {
        TelemetryTarget::Https(endpoint.to_string())
    } else {
        TelemetryTarget::Rejected
    }
}

/// OS family label included in the telemetry event.
#[must_use]
pub fn os_family() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "other"
    }
}

/// One JSON object, with no trailing newline.
pub fn telemetry_json(event: &TelemetryEvent<'_>) -> Result<String, ()> {
    serde_json::to_string(event).map_err(|_| ())
}

/// Append one JSON line, keeping the file from growing without bound.
pub fn append_telemetry_line(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.metadata().is_ok_and(|m| m.len() > 256 * 1024) {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let kept: Vec<&str> = text.lines().rev().take(50).collect();
        let mut out = String::new();
        for kept_line in kept.into_iter().rev() {
            out.push_str(kept_line);
            out.push('\n');
        }
        out.push_str(line);
        out.push('\n');
        return std::fs::write(path, out);
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")
}

fn trusted_release_page(url: &str) -> Option<String> {
    let url = url.trim();
    if url.starts_with("https://github.com/ionpmp/Orchid/") {
        Some(url.to_string())
    } else {
        None
    }
}

fn version_key(raw: &str) -> Option<(Vec<u64>, Option<String>)> {
    let raw = raw.trim();
    let raw = raw
        .strip_prefix('v')
        .or_else(|| raw.strip_prefix('V'))
        .unwrap_or(raw);
    if raw.is_empty() {
        return None;
    }
    let (core, pre) = match raw.split_once('-') {
        Some((core, pre)) => {
            let pre = pre.split_once('+').map(|(p, _)| p).unwrap_or(pre);
            (core, Some(pre.to_string()))
        }
        None => {
            let core = raw.split_once('+').map(|(c, _)| c).unwrap_or(raw);
            (core, None)
        }
    };
    if core.is_empty() {
        return None;
    }
    let mut nums = Vec::new();
    for part in core.split('.') {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        nums.push(part.parse::<u64>().ok()?);
    }
    Some((nums, pre))
}

fn pre_cmp(a: &str, b: &str) -> Ordering {
    let a: Vec<&str> = a.split('.').collect();
    let b: Vec<&str> = b.split('.').collect();
    let len = a.len().max(b.len());
    for i in 0..len {
        match (a.get(i), b.get(i)) {
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(nx), Ok(ny)) => nx.cmp(&ny),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => (*x).cmp(*y),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (None, None) => break,
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_compares_semver() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("0.1.0", "0.1.0-alpha"));
        assert!(is_newer("0.1.0-alpha.2", "0.1.0-alpha.1"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0-alpha", "0.1.0"));
        assert!(!is_newer("1.0.0+build", "1.0.0"));
        assert!(!is_newer("not-a-version", "0.1.0"));
    }

    #[test]
    fn release_json_maps_to_an_offer() {
        let body = r#"{"tag_name":"v0.9.0","html_url":"https://github.com/ionpmp/Orchid/releases/tag/v0.9.0"}"#;
        assert_eq!(
            interpret_release(200, body, "0.1.0").unwrap(),
            ReleaseOffer::Newer {
                version: "0.9.0".into(),
                page: Some("https://github.com/ionpmp/Orchid/releases/tag/v0.9.0".into()),
            }
        );
        let same = r#"{"tag_name":"v0.1.0","html_url":"https://github.com/ionpmp/Orchid/releases/tag/v0.1.0"}"#;
        assert_eq!(
            interpret_release(200, same, "0.1.0").unwrap(),
            ReleaseOffer::Current
        );
        assert_eq!(
            interpret_release(404, "", "0.1.0").unwrap(),
            ReleaseOffer::None
        );
        let foreign = r#"{"tag_name":"v9.0.0","html_url":"https://evil.example/x"}"#;
        assert_eq!(
            interpret_release(200, foreign, "0.1.0").unwrap(),
            ReleaseOffer::Newer {
                version: "9.0.0".into(),
                page: None,
            }
        );
        assert!(interpret_release(500, "", "0.1.0").is_err());
    }

    #[test]
    fn telemetry_target_requires_https() {
        assert_eq!(telemetry_target("  "), TelemetryTarget::LocalOnly);
        assert_eq!(
            telemetry_target("https://example.com/collect"),
            TelemetryTarget::Https("https://example.com/collect".into())
        );
        assert_eq!(
            telemetry_target("http://example.com/collect"),
            TelemetryTarget::Rejected
        );
        assert_eq!(telemetry_target("not a url"), TelemetryTarget::Rejected);
    }

    #[test]
    fn journal_appends_one_json_line() {
        let dir = std::env::temp_dir().join(format!("orchid-tel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("telemetry.jsonl");
        let event = TelemetryEvent {
            event: "app-start",
            version: "0.1.0",
            os: "windows",
            language: "en-US",
        };
        let line = telemetry_json(&event).unwrap();
        append_telemetry_line(&path, &line).unwrap();
        append_telemetry_line(&path, &line).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("\"event\":\"app-start\""));
        assert!(!text.contains("C:\\"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
