//! Windows window enumeration for App grouping.

use std::collections::HashSet;

/// Collect PIDs that own at least one top-level visible application window.
///
/// Mirrors Task Manager's "Apps" heuristic: visible, titled, non-tool windows
/// without an owner. Returns an empty set on non-Windows platforms.
#[must_use]
pub fn collect_app_pids() -> HashSet<u32> {
    #[cfg(windows)]
    {
        win::collect_app_pids()
    }
    #[cfg(not(windows))]
    {
        HashSet::new()
    }
}

#[cfg(windows)]
mod win {
    use std::collections::HashSet;

    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowThreadProcessId,
        IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WINDOW_EX_STYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    struct State {
        pids: HashSet<u32>,
    }

    pub fn collect_app_pids() -> HashSet<u32> {
        let mut state = State {
            pids: HashSet::new(),
        };
        // SAFETY: EnumWindows callback only touches the State behind LPARAM for
        // the duration of this call; no other threads share it.
        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(&mut state as *mut State as isize));
        }
        state.pids
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam.0 as *mut State);
        if !is_app_window(hwnd) {
            return BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != 0 {
            state.pids.insert(pid);
        }
        BOOL(1)
    }

    unsafe fn is_app_window(hwnd: HWND) -> bool {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        // Owned windows (dialogs, popups) are not independent Apps.
        if GetWindow(hwnd, GW_OWNER).ok().is_some_and(|o| !o.is_invalid()) {
            return false;
        }
        let ex = WINDOW_EX_STYLE(GetWindowLongW(hwnd, GWL_EXSTYLE) as u32);
        if ex.contains(WS_EX_TOOLWINDOW) || ex.contains(WS_EX_NOACTIVATE) {
            return false;
        }
        // Require a non-empty title so we skip invisible host / tray shells.
        GetWindowTextLengthW(hwnd) > 0
    }
}
