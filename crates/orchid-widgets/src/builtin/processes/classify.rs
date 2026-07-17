//! Process grouping heuristics (Apps / Background / Windows).

use std::path::Path;

use crate::widget::payloads::ProcessGroup;

/// Classify a process into a Task Manager–style group.
#[must_use]
pub fn classify_process(
    name: &str,
    path: &str,
    session_id: Option<u32>,
    parent_pid: Option<u32>,
) -> ProcessGroup {
    let name_l = name.to_ascii_lowercase();
    let path_l = path.to_ascii_lowercase();

    if is_windows_process(&name_l, &path_l) {
        return ProcessGroup::Windows;
    }

    // Session 0 is the services / system session on Windows.
    if session_id == Some(0) {
        return ProcessGroup::Windows;
    }

    // Orphaned / root-like background helpers without a user session.
    if session_id.is_none() && parent_pid.is_some_and(|p| p <= 4) {
        return ProcessGroup::Background;
    }

    // Heuristic: processes under Program Files / user profile with a non-system
    // name are treated as Apps; everything else Background.
    if looks_like_app(&path_l, &name_l) {
        ProcessGroup::Apps
    } else {
        ProcessGroup::Background
    }
}

fn is_windows_process(name: &str, path: &str) -> bool {
    const SYSTEM_NAMES: &[&str] = &[
        "system",
        "registry",
        "smss.exe",
        "csrss.exe",
        "wininit.exe",
        "services.exe",
        "lsass.exe",
        "svchost.exe",
        "winlogon.exe",
        "fontdrvhost.exe",
        "dwm.exe",
        "sihost.exe",
        "taskhostw.exe",
        "runtimebroker.exe",
        "searchhost.exe",
        "startmenuexperiencehost.exe",
        "shellexperiencehost.exe",
        "textinputhost.exe",
        "ctfmon.exe",
        "conhost.exe",
        "dllhost.exe",
        "wudfhost.exe",
        "spoolsv.exe",
        "memory compression",
        "secure system",
    ];
    if SYSTEM_NAMES.iter().any(|n| name == *n) {
        return true;
    }
    path.contains("\\windows\\system32\\")
        || path.contains("\\windows\\syswow64\\")
        || path.contains("\\windows\\systemapps\\")
        || path.ends_with("\\windows\\explorer.exe")
}

fn looks_like_app(path: &str, name: &str) -> bool {
    if path.is_empty() {
        // No path — treat non-system names as apps when they look like GUIs.
        return name.ends_with(".exe") && !name.starts_with("service");
    }
    let p = Path::new(path);
    let s = path;
    s.contains("\\program files")
        || s.contains("\\program files (x86)")
        || s.contains("\\users\\")
        || s.contains("\\appdata\\")
        || p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            && !is_windows_process(name, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_svchost_as_windows() {
        assert_eq!(
            classify_process("svchost.exe", r"C:\Windows\System32\svchost.exe", Some(0), Some(800)),
            ProcessGroup::Windows
        );
    }

    #[test]
    fn classifies_user_app() {
        assert_eq!(
            classify_process(
                "Code.exe",
                r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe",
                Some(1),
                Some(1000),
            ),
            ProcessGroup::Apps
        );
    }
}
