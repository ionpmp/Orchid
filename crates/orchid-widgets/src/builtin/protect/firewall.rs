//! Outbound firewall rules owned by this widget.
//!
//! Rule names are deterministic (`Orchid Protect` + a hash of the path) so a
//! later unblock does not depend on the Windows UI language. Listing prefers
//! the JSON from `Get-NetFirewallRule`, which keeps property names stable on
//! a localized system.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Name prefix of every rule this widget creates.
pub const ORCHID_RULE_PREFIX: &str = "Orchid Protect ";

/// Why a program cannot be blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockRefusal {
    /// Not an `.exe` path.
    NotExe,
    /// A process the system cannot run without.
    Critical,
    /// Anything directly under `Windows\System32` or `SysWOW64`.
    SystemDir,
    /// Quote or newline in the path.
    UnsafePath,
}

/// A program row on the Network tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRow {
    /// File name shown in the list.
    pub name: String,
    /// Full path passed to the firewall.
    pub path: String,
    /// An Orchid Protect outbound block is in effect.
    pub blocked: bool,
}

/// A running program before it is merged with saved blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningApp {
    /// File name.
    pub name: String,
    /// Full executable path.
    pub path: String,
}

/// One firewall rule parsed from PowerShell JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirewallRule {
    /// Display name.
    pub name: String,
    /// Program path. Empty when the rule is not bound to a program.
    pub program: String,
    /// `true` when the action is Block.
    pub block: bool,
    /// `true` when the direction is Outbound.
    pub outbound: bool,
}

/// `None` when this path may receive an outbound block rule.
#[must_use]
pub fn block_refusal(path: &Path) -> Option<BlockRefusal> {
    let Some(name) = path.file_name() else {
        return Some(BlockRefusal::NotExe);
    };
    let name = name.to_string_lossy().to_ascii_lowercase();
    if !name.ends_with(".exe") {
        return Some(BlockRefusal::NotExe);
    }
    if CRITICAL.contains(&name.as_str()) {
        return Some(BlockRefusal::Critical);
    }
    let full = path.to_string_lossy();
    if full.contains('"') || full.contains('\n') || full.contains('\r') {
        return Some(BlockRefusal::UnsafePath);
    }
    let lower = full.to_ascii_lowercase();
    if lower.contains("\\windows\\system32\\")
        || lower.contains("\\windows\\syswow64\\")
        || lower.contains("/windows/system32/")
        || lower.contains("/windows/syswow64/")
    {
        return Some(BlockRefusal::SystemDir);
    }
    None
}

/// Stable rule name for `path`.
#[must_use]
pub fn rule_name(path: &Path) -> String {
    let norm = path.to_string_lossy().to_ascii_lowercase();
    let hash = fnv1a32(norm.as_bytes());
    let file = safe_exe_name(path).to_ascii_lowercase();
    format!("{ORCHID_RULE_PREFIX}{hash:08x} {file}")
}

/// Arguments for `netsh advfirewall firewall add rule`.
#[must_use]
pub fn netsh_add_args(name: &str, program: &str) -> Vec<String> {
    vec![
        "advfirewall".into(),
        "firewall".into(),
        "add".into(),
        "rule".into(),
        format!("name={name}"),
        "dir=out".into(),
        "action=block".into(),
        format!("program={program}"),
        "enable=yes".into(),
        "profile=any".into(),
    ]
}

/// Arguments for `netsh advfirewall firewall delete rule`.
#[must_use]
pub fn netsh_delete_args(name: &str) -> Vec<String> {
    vec![
        "advfirewall".into(),
        "firewall".into(),
        "delete".into(),
        "rule".into(),
        format!("name={name}"),
    ]
}

/// Parse `ConvertTo-Json` output from `Get-NetFirewallRule`.
///
/// An empty or unreadable body yields an empty list. Rules that are not
/// outbound blocks named with [`ORCHID_RULE_PREFIX`] are dropped.
#[must_use]
pub fn parse_firewall_json(text: &str) -> Vec<FirewallRule> {
    let text = text.trim().trim_start_matches('\u{feff}');
    if text.is_empty() {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let items: Vec<Value> = match value {
        Value::Array(items) => items,
        other @ Value::Object(_) => vec![other],
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| {
            let name = field_text(item, "Name");
            if !name.starts_with(ORCHID_RULE_PREFIX) {
                return None;
            }
            let action = field_text(item, "Action");
            let direction = field_text(item, "Direction");
            let block = is_block(&action);
            let outbound = is_outbound(&direction);
            if !block || !outbound {
                return None;
            }
            Some(FirewallRule {
                name,
                program: field_text(item, "Program"),
                block,
                outbound,
            })
        })
        .collect()
}

/// Running programs plus blocked programs that are not running right now.
///
/// Critical and system-directory programs are omitted. Comparison of paths
/// ignores ASCII case.
#[must_use]
pub fn merge_app_rows(running: &[RunningApp], blocked_paths: &[String]) -> Vec<AppRow> {
    let mut rows: Vec<AppRow> = Vec::new();
    for app in running {
        let path = PathBuf::from(&app.path);
        if block_refusal(&path).is_some() {
            continue;
        }
        let blocked = blocked_paths
            .iter()
            .any(|saved| saved.eq_ignore_ascii_case(&app.path));
        if rows
            .iter()
            .any(|row| row.path.eq_ignore_ascii_case(&app.path))
        {
            continue;
        }
        rows.push(AppRow {
            name: app.name.clone(),
            path: app.path.clone(),
            blocked,
        });
    }
    for saved in blocked_paths {
        if rows.iter().any(|row| row.path.eq_ignore_ascii_case(saved)) {
            continue;
        }
        let path = PathBuf::from(saved);
        if block_refusal(&path).is_some() {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| saved.clone());
        rows.push(AppRow {
            name,
            path: saved.clone(),
            blocked: true,
        });
    }
    rows.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
            .then_with(|| a.path.cmp(&b.path))
    });
    rows
}

const CRITICAL: &[&str] = &[
    "csrss.exe",
    "lsass.exe",
    "lsaiso.exe",
    "smss.exe",
    "wininit.exe",
    "services.exe",
    "winlogon.exe",
    "dwm.exe",
    "svchost.exe",
    "securityhealthservice.exe",
    "msmpeng.exe",
    "orchid.exe",
    "sihost.exe",
    "fontdrvhost.exe",
];

fn safe_exe_name(path: &Path) -> String {
    let raw = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app.exe".into());
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "app.exe".into()
    } else {
        cleaned
    }
}

fn fnv1a32(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn field_text(item: &Value, key: &str) -> String {
    let Some(value) = item.get(key).or_else(|| {
        item.as_object().and_then(|map| {
            map.iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .map(|(_, value)| value)
        })
    }) else {
        return String::new();
    };
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Object(map) => map
            .get("Value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

fn is_block(value: &str) -> bool {
    value.eq_ignore_ascii_case("block") || value == "4"
}

fn is_outbound(value: &str) -> bool {
    value.eq_ignore_ascii_case("outbound") || value == "2"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_system_and_accepts_a_normal_program() {
        assert_eq!(
            block_refusal(Path::new(r"C:\Windows\System32\svchost.exe")),
            Some(BlockRefusal::Critical)
        );
        assert_eq!(
            block_refusal(Path::new(r"C:\Windows\System32\notepad.exe")),
            Some(BlockRefusal::SystemDir)
        );
        assert_eq!(
            block_refusal(Path::new(r"C:\Games\Orchid.exe")),
            Some(BlockRefusal::Critical)
        );
        assert_eq!(
            block_refusal(Path::new(r"C:\Games\launcher.bat")),
            Some(BlockRefusal::NotExe)
        );
        assert_eq!(block_refusal(Path::new(r"D:\Apps\game.exe")), None);
    }

    #[test]
    fn rule_name_is_stable_and_prefixed() {
        let path = Path::new(r"D:\Apps\Game.exe");
        let name = rule_name(path);
        assert_eq!(name, rule_name(Path::new(r"d:\apps\game.exe")));
        assert!(name.starts_with(ORCHID_RULE_PREFIX));
        assert!(name.contains("game.exe"));
        let args = netsh_add_args(&name, r"D:\Apps\Game.exe");
        assert!(args.iter().any(|a| a == "action=block"));
        assert!(args.iter().any(|a| a == "dir=out"));
    }

    #[test]
    fn parses_a_single_object_and_an_array() {
        let one = r#"{"Name":"Orchid Protect abcd1234 game.exe","Program":"D:\\Apps\\game.exe","Action":"Block","Direction":"Outbound"}"#;
        let rules = parse_firewall_json(one);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, r"D:\Apps\game.exe");

        let many = r#"[
            {"Name":"Core Networking","Program":"*","Action":"Allow","Direction":"Outbound"},
            {"Name":"Orchid Protect 00ff00ff chrome.exe","Program":"C:\\Chrome\\chrome.exe","Action":4,"Direction":2}
        ]"#;
        let rules = parse_firewall_json(many);
        assert_eq!(rules.len(), 1);
        assert!(rules[0].name.contains("chrome.exe"));
        assert!(parse_firewall_json("not json").is_empty());
    }

    #[test]
    fn merge_hides_system_programs_and_keeps_a_blocked_app_that_is_not_running() {
        let running = vec![
            RunningApp {
                name: "svchost.exe".into(),
                path: r"C:\Windows\System32\svchost.exe".into(),
            },
            RunningApp {
                name: "game.exe".into(),
                path: r"D:\Apps\game.exe".into(),
            },
        ];
        let blocked = vec![r"D:\Apps\game.exe".into(), r"D:\Apps\old.exe".into()];
        let rows = merge_app_rows(&running, &blocked);
        assert_eq!(rows.len(), 2);
        assert!(rows
            .iter()
            .any(|r| r.path.ends_with("game.exe") && r.blocked));
        assert!(rows
            .iter()
            .any(|r| r.path.ends_with("old.exe") && r.blocked));
        assert!(rows.iter().all(|r| !r.path.contains("System32")));
    }
}
