//! Windows Service Control Manager integration.

use crate::widget::payloads::ServiceRowView;

#[cfg(windows)]
#[allow(missing_docs)]
mod win {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_MORE_DATA;
    use windows::Win32::System::Services::{
        CloseServiceHandle, ControlService, EnumServicesStatusExW, OpenSCManagerW, OpenServiceW,
        QueryServiceConfigW, StartServiceW, ENUM_SERVICE_STATUS_PROCESSW, QUERY_SERVICE_CONFIGW,
        SC_ENUM_PROCESS_INFO, SC_HANDLE, SC_MANAGER_CONNECT, SC_MANAGER_ENUMERATE_SERVICE,
        SERVICE_AUTO_START, SERVICE_BOOT_START, SERVICE_CONTROL_STOP, SERVICE_CONTINUE_PENDING,
        SERVICE_DEMAND_START, SERVICE_DISABLED, SERVICE_PAUSE_PENDING, SERVICE_PAUSED,
        SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START,
        SERVICE_START_PENDING, SERVICE_START_TYPE, SERVICE_STATE_ALL, SERVICE_STATUS,
        SERVICE_STATUS_CURRENT_STATE, SERVICE_STOP, SERVICE_STOP_PENDING, SERVICE_STOPPED,
        SERVICE_SYSTEM_START, SERVICE_WIN32,
    };

    use super::ServiceRowView;

    struct ScHandle(SC_HANDLE);
    impl Drop for ScHandle {
        fn drop(&mut self) {
            let _ = unsafe { CloseServiceHandle(self.0) };
        }
    }

    pub fn list_services() -> Result<Vec<ServiceRowView>, String> {
        unsafe {
            let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE)
                .map_err(|e| format!("OpenSCManager: {e}"))?;
            let scm = ScHandle(scm);

            let mut bytes_needed = 0u32;
            let mut services_returned = 0u32;
            let mut resume = 0u32;
            let first = EnumServicesStatusExW(
                scm.0,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                None,
                &mut bytes_needed,
                &mut services_returned,
                Some(&mut resume),
                PCWSTR::null(),
            );
            if let Err(e) = first {
                if e.code() != ERROR_MORE_DATA.to_hresult() {
                    return Err(format!("EnumServicesStatusEx: {e}"));
                }
            }
            if bytes_needed == 0 {
                return Ok(Vec::new());
            }

            let mut buf = vec![0u8; bytes_needed as usize];
            resume = 0;
            EnumServicesStatusExW(
                scm.0,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                Some(buf.as_mut_slice()),
                &mut bytes_needed,
                &mut services_returned,
                Some(&mut resume),
                PCWSTR::null(),
            )
            .map_err(|e| format!("EnumServicesStatusEx: {e}"))?;

            let count = services_returned as usize;
            let entries = buf.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>();
            let mut out = Vec::with_capacity(count);
            for i in 0..count {
                let entry = &*entries.add(i);
                let name = pwstr_to_string(entry.lpServiceName);
                let display_name = pwstr_to_string(entry.lpDisplayName);
                let state = entry.ServiceStatusProcess.dwCurrentState;
                let pid = entry.ServiceStatusProcess.dwProcessId;
                let status = state_label(state);
                let start_type = query_start_type(scm.0, &name).unwrap_or_else(|| "—".into());
                let can_start = state == SERVICE_STOPPED;
                let can_stop = state == SERVICE_RUNNING || state == SERVICE_PAUSED;
                out.push(ServiceRowView {
                    name,
                    display_name,
                    status,
                    status_code: state.0,
                    start_type,
                    pid,
                    can_start,
                    can_stop,
                });
            }
            out.sort_by(|a, b| {
                a.display_name
                    .to_ascii_lowercase()
                    .cmp(&b.display_name.to_ascii_lowercase())
            });
            Ok(out)
        }
    }

    pub fn start_service(name: &str) -> Result<(), String> {
        with_service(name, SERVICE_START | SERVICE_QUERY_STATUS, |svc| unsafe {
            StartServiceW(svc, None).map_err(|e| format!("StartService: {e}"))?;
            wait_for_state(svc, SERVICE_RUNNING, std::time::Duration::from_secs(15))
        })
    }

    pub fn stop_service(name: &str) -> Result<(), String> {
        with_service(name, SERVICE_STOP | SERVICE_QUERY_STATUS, |svc| unsafe {
            let mut status = SERVICE_STATUS::default();
            let state = query_state(svc)?;
            if state == SERVICE_STOPPED {
                return Ok(());
            }
            ControlService(svc, SERVICE_CONTROL_STOP, &mut status)
                .map_err(|e| format!("ControlService stop: {e}"))?;
            wait_for_state(svc, SERVICE_STOPPED, std::time::Duration::from_secs(15))
        })
    }

    pub fn restart_service(name: &str) -> Result<(), String> {
        // Best-effort stop (already-stopped is fine), then start and wait.
        let _ = stop_service(name);
        start_service(name)
    }

    fn with_service<F>(name: &str, access: u32, f: F) -> Result<(), String>
    where
        F: FnOnce(SC_HANDLE) -> Result<(), String>,
    {
        unsafe {
            let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT)
                .map_err(|e| format!("OpenSCManager: {e}"))?;
            let scm = ScHandle(scm);
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let svc = OpenServiceW(scm.0, PCWSTR(wide.as_ptr()), access)
                .map_err(|e| format!("OpenService({name}): {e}"))?;
            let svc = ScHandle(svc);
            f(svc.0)
        }
    }

    unsafe fn query_state(svc: SC_HANDLE) -> Result<SERVICE_STATUS_CURRENT_STATE, String> {
        use windows::Win32::System::Services::QueryServiceStatus;
        let mut status = SERVICE_STATUS::default();
        QueryServiceStatus(svc, &mut status).map_err(|e| format!("QueryServiceStatus: {e}"))?;
        Ok(status.dwCurrentState)
    }

    unsafe fn wait_for_state(
        svc: SC_HANDLE,
        wanted: SERVICE_STATUS_CURRENT_STATE,
        timeout: std::time::Duration,
    ) -> Result<(), String> {
        let start = std::time::Instant::now();
        loop {
            let state = query_state(svc)?;
            if state == wanted {
                return Ok(());
            }
            if start.elapsed() >= timeout {
                return Err(format!(
                    "timed out waiting for service state {} (last {})",
                    wanted.0, state.0
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    }

    unsafe fn query_start_type(scm: SC_HANDLE, name: &str) -> Option<String> {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let svc = OpenServiceW(scm, PCWSTR(wide.as_ptr()), SERVICE_QUERY_CONFIG).ok()?;
        let svc = ScHandle(svc);
        let mut needed = 0u32;
        let _ = QueryServiceConfigW(svc.0, None, 0, &mut needed);
        if needed == 0 {
            return None;
        }
        let mut buf = vec![0u8; needed as usize];
        QueryServiceConfigW(
            svc.0,
            Some(buf.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>()),
            needed,
            &mut needed,
        )
        .ok()?;
        let cfg = &*buf.as_ptr().cast::<QUERY_SERVICE_CONFIGW>();
        Some(start_type_label(cfg.dwStartType))
    }

    fn state_label(state: SERVICE_STATUS_CURRENT_STATE) -> String {
        match state.0 {
            x if x == SERVICE_STOPPED.0 => "Stopped".into(),
            x if x == SERVICE_START_PENDING.0 => "Start pending".into(),
            x if x == SERVICE_STOP_PENDING.0 => "Stop pending".into(),
            x if x == SERVICE_RUNNING.0 => "Running".into(),
            x if x == SERVICE_CONTINUE_PENDING.0 => "Continue pending".into(),
            x if x == SERVICE_PAUSE_PENDING.0 => "Pause pending".into(),
            x if x == SERVICE_PAUSED.0 => "Paused".into(),
            other => format!("Unknown ({other})"),
        }
    }

    fn start_type_label(t: SERVICE_START_TYPE) -> String {
        match t.0 {
            x if x == SERVICE_BOOT_START.0 => "Boot".into(),
            x if x == SERVICE_SYSTEM_START.0 => "System".into(),
            x if x == SERVICE_AUTO_START.0 => "Automatic".into(),
            x if x == SERVICE_DEMAND_START.0 => "Manual".into(),
            x if x == SERVICE_DISABLED.0 => "Disabled".into(),
            other => format!("Other ({other})"),
        }
    }

    unsafe fn pwstr_to_string(p: windows::core::PWSTR) -> String {
        if p.is_null() {
            return String::new();
        }
        p.to_string().unwrap_or_default()
    }
}

#[cfg(windows)]
pub use win::{list_services, restart_service, start_service, stop_service};

#[cfg(not(windows))]
pub fn list_services() -> Result<Vec<ServiceRowView>, String> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub fn start_service(_name: &str) -> Result<(), String> {
    Err("services are only supported on Windows".into())
}

#[cfg(not(windows))]
pub fn stop_service(_name: &str) -> Result<(), String> {
    Err("services are only supported on Windows".into())
}

#[cfg(not(windows))]
pub fn restart_service(_name: &str) -> Result<(), String> {
    Err("services are only supported on Windows".into())
}
