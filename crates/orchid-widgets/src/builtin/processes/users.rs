//! Windows Terminal Services session listing.

use crate::widget::payloads::UserRowView;

#[cfg(windows)]
#[allow(missing_docs)]
mod win {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{FALSE, HANDLE};
    use windows::Win32::System::RemoteDesktop::{
        WTSDisconnectSession, WTSEnumerateSessionsW, WTSFreeMemory, WTSLogoffSession,
        WTSQuerySessionInformationW, WTSUserName, WTS_CONNECTSTATE_CLASS,
        WTS_CURRENT_SERVER_HANDLE, WTS_SESSION_INFOW,
    };

    use super::UserRowView;
    use crate::builtin::processes::types::ProcessSample;

    pub fn list_sessions(processes: &[ProcessSample]) -> Result<Vec<UserRowView>, String> {
        unsafe {
            let mut info: *mut WTS_SESSION_INFOW = std::ptr::null_mut();
            let mut count = 0u32;
            WTSEnumerateSessionsW(WTS_CURRENT_SERVER_HANDLE, 0, 1, &mut info, &mut count)
                .map_err(|e| format!("WTSEnumerateSessions: {e}"))?;
            if info.is_null() {
                return Ok(Vec::new());
            }
            let mut out = Vec::with_capacity(count as usize);
            for i in 0..count as usize {
                let entry = &*info.add(i);
                let session_id = entry.SessionId;
                let state = state_label(entry.State);
                let mut user_name = query_user_name(session_id).unwrap_or_default();
                if user_name.is_empty() && !entry.pWinStationName.is_null() {
                    user_name = entry.pWinStationName.to_string().unwrap_or_default();
                }
                let (process_count, memory_bytes) = processes
                    .iter()
                    .filter(|p| p.session_id == Some(session_id))
                    .fold((0u32, 0u64), |acc, p| {
                        (acc.0 + 1, acc.1.saturating_add(p.memory_bytes))
                    });
                out.push(UserRowView {
                    session_id,
                    user_name,
                    state,
                    process_count,
                    memory_bytes,
                    memory_text: String::new(), // filled by locale layer
                });
            }
            WTSFreeMemory(info.cast());
            out.sort_by_key(|u| u.session_id);
            Ok(out)
        }
    }

    pub fn disconnect_session(session_id: u32) -> Result<(), String> {
        unsafe {
            WTSDisconnectSession(WTS_CURRENT_SERVER_HANDLE, session_id, FALSE)
                .map_err(|e| format!("WTSDisconnectSession: {e}"))
        }
    }

    pub fn sign_out_session(session_id: u32) -> Result<(), String> {
        unsafe {
            WTSLogoffSession(WTS_CURRENT_SERVER_HANDLE, session_id, FALSE)
                .map_err(|e| format!("WTSLogoffSession: {e}"))
        }
    }

    unsafe fn query_user_name(session_id: u32) -> Option<String> {
        let mut buf = PWSTR::null();
        let mut bytes = 0u32;
        WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            session_id,
            WTSUserName,
            &mut buf,
            &mut bytes,
        )
        .ok()?;
        if buf.is_null() {
            return None;
        }
        let s = buf.to_string().ok();
        WTSFreeMemory(buf.0 as _);
        s
    }

    fn state_label(state: WTS_CONNECTSTATE_CLASS) -> String {
        // Compare `.0` — windows crate constants trigger `non_upper_case_globals` in patterns.
        match state.0 {
            0 => "Active".into(),
            1 => "Connected".into(),
            2 => "Connect query".into(),
            3 => "Shadow".into(),
            4 => "Disconnected".into(),
            5 => "Idle".into(),
            6 => "Listen".into(),
            7 => "Reset".into(),
            8 => "Down".into(),
            9 => "Init".into(),
            other => format!("Unknown ({other})"),
        }
    }

    #[allow(dead_code)]
    fn _handle() -> HANDLE {
        HANDLE::default()
    }
}

#[cfg(windows)]
pub use win::{disconnect_session, list_sessions, sign_out_session};

#[cfg(not(windows))]
pub fn list_sessions(
    processes: &[super::types::ProcessSample],
) -> Result<Vec<UserRowView>, String> {
    let mut by_session: std::collections::BTreeMap<u32, (u32, u64, String)> =
        std::collections::BTreeMap::new();
    for p in processes {
        if let Some(sid) = p.session_id {
            let e = by_session.entry(sid).or_insert((0, 0, p.user.clone()));
            e.0 += 1;
            e.1 = e.1.saturating_add(p.memory_bytes);
            if e.2.is_empty() {
                e.2 = p.user.clone();
            }
        }
    }
    Ok(by_session
        .into_iter()
        .map(|(session_id, (process_count, memory_bytes, user_name))| UserRowView {
            session_id,
            user_name,
            state: String::new(),
            process_count,
            memory_bytes,
            memory_text: String::new(),
        })
        .collect())
}

#[cfg(not(windows))]
pub fn disconnect_session(_session_id: u32) -> Result<(), String> {
    Err("user sessions are only supported on Windows".into())
}

#[cfg(not(windows))]
pub fn sign_out_session(_session_id: u32) -> Result<(), String> {
    Err("user sessions are only supported on Windows".into())
}
