//! Process grouping heuristics (Apps / Background / Windows).

use crate::widget::payloads::ProcessGroup;

/// Classify a process into a Task Manager–style group.
///
/// `has_visible_window` should be `true` when the PID owns a top-level visible
/// application window (see [`super::windows::collect_app_pids`]).
#[must_use]
pub fn classify_process(
    name: &str,
    path: &str,
    session_id: Option<u32>,
    has_visible_window: bool,
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

    // Primary signal: owning a visible top-level window ⇒ Apps.
    if has_visible_window {
        return ProcessGroup::Apps;
    }

    ProcessGroup::Background
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
        "explorer.exe",
    ];
    if SYSTEM_NAMES.iter().any(|n| name == *n) {
        return true;
    }
    path.contains("\\windows\\system32\\")
        || path.contains("\\windows\\syswow64\\")
        || path.contains("\\windows\\systemapps\\")
        || path.ends_with("\\windows\\explorer.exe")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_svchost_as_windows() {
        assert_eq!(
            classify_process(
                "svchost.exe",
                r"C:\Windows\System32\svchost.exe",
                Some(0),
                false
            ),
            ProcessGroup::Windows
        );
    }

    #[test]
    fn visible_window_marks_app() {
        assert_eq!(
            classify_process(
                "Code.exe",
                r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe",
                Some(1),
                true,
            ),
            ProcessGroup::Apps
        );
    }

    #[test]
    fn no_window_is_background() {
        assert_eq!(
            classify_process(
                "node.exe",
                r"C:\Users\me\AppData\Roaming\nvm\node.exe",
                Some(1),
                false,
            ),
            ProcessGroup::Background
        );
    }

    #[test]
    fn explorer_stays_windows_even_with_window() {
        assert_eq!(
            classify_process(
                "explorer.exe",
                r"C:\Windows\explorer.exe",
                Some(1),
                true,
            ),
            ProcessGroup::Windows
        );
    }
}
