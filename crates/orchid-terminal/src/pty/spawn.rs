//! Spawn a child process inside a freshly-created PTY.

use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use portable_pty::native_pty_system;

use crate::backend::BackendSpec;
use crate::error::{Result, TerminalError};
use crate::pty::{PtyHandle, PtySize};

/// Spawn a child inside a PTY sized to `size`.
///
/// Returns an `Arc<PtyHandle>` that owns both master and child. The slave
/// end is dropped inside so EOF propagates cleanly when the child exits.
///
/// On Windows, the child is assigned to a Job Object with
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so the whole process tree dies when
/// Orchid exits (or the [`PtyHandle`] is dropped).
///
/// # Errors
///
/// * [`TerminalError::Pty`] if the OS cannot create a PTY.
/// * [`TerminalError::SpawnFailed`] if the child can't be spawned.
pub fn spawn(spec: &BackendSpec, size: PtySize) -> Result<Arc<PtyHandle>> {
    let system = native_pty_system();
    let pair = system
        .openpty(size.into())
        .map_err(|e| TerminalError::Pty(e.to_string()))?;
    let builder = spec.to_command()?;

    let child = pair
        .slave
        .spawn_command(builder)
        .map_err(|e| TerminalError::SpawnFailed(e.to_string()))?;

    // Drop the slave side so reads on the master observe EOF when the child
    // closes its stdio.
    drop(pair.slave);

    #[cfg(windows)]
    let job = match assign_kill_on_close_job(&*child) {
        Ok(job) => Some(job),
        Err(err) => {
            tracing::warn!(
                error = %err,
                "failed to assign PTY child to Job Object; relying on explicit kill"
            );
            None
        }
    };

    Ok(Arc::new(PtyHandle {
        child: Mutex::new(child),
        master: Mutex::new(pair.master),
        size: RwLock::new(size),
        started_at: chrono::Utc::now(),
        #[cfg(windows)]
        _job: job,
    }))
}

/// Create a Job Object with `KILL_ON_JOB_CLOSE` and assign `child` to it.
///
/// The returned handle must stay open for the lifetime of the session: when
/// it is closed (Orchid exit or [`PtyHandle`] drop), Windows terminates every
/// process still in the job.
#[cfg(windows)]
fn assign_kill_on_close_job(
    child: &dyn portable_pty::Child,
) -> std::result::Result<crate::pty::JobHandle, String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    // Prefer the child's process handle; fall back to opening by PID.
    let (process, owned): (HANDLE, bool) = if let Some(raw) = child.as_raw_handle() {
        (HANDLE(raw), false)
    } else if let Some(pid) = child.process_id() {
        (open_process_for_job(pid)?, true)
    } else {
        return Err("PTY child has neither process handle nor PID".into());
    };

    unsafe {
        let job = CreateJobObjectW(None, PCWSTR::null()).map_err(|e| e.to_string())?;

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if let Err(e) = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of_val(&info) as u32,
        ) {
            let _ = CloseHandle(job);
            if owned {
                let _ = CloseHandle(process);
            }
            return Err(e.to_string());
        }

        if let Err(e) = AssignProcessToJobObject(job, process) {
            let _ = CloseHandle(job);
            if owned {
                let _ = CloseHandle(process);
            }
            return Err(e.to_string());
        }

        // Only close handles we opened ourselves; portable-pty still owns
        // the child's process handle.
        if owned {
            let _ = CloseHandle(process);
        }

        Ok(crate::pty::JobHandle(job))
    }
}

#[cfg(windows)]
fn open_process_for_job(pid: u32) -> std::result::Result<windows::Win32::Foundation::HANDLE, String> {
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

    unsafe {
        OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid).map_err(|e| e.to_string())
    }
}
