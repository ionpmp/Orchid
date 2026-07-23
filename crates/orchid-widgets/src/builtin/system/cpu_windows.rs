//! Windows CPU sampling via `GetSystemTimes` / `NtQuerySystemInformation`.
//!
//! Avoids PDH `\Processor(*)\% Idle Time`, which on some Windows 11 hosts
//! returns a near-constant value through `sysinfo` and makes the System
//! widget look stuck (e.g. forever "71%").

use std::mem::{size_of, MaybeUninit};

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
use windows::Win32::System::Threading::GetSystemTimes;

/// Previous cumulative times used to compute a usage delta.
#[derive(Debug, Clone)]
pub(super) struct WindowsCpuState {
    total: CpuTimes,
    per_core: Vec<CpuTimes>,
}

#[derive(Debug, Clone, Copy)]
struct CpuTimes {
    idle: u64,
    /// Kernel time **includes** idle on Windows.
    kernel: u64,
    user: u64,
}

impl CpuTimes {
    fn usage_since(self, prev: Self) -> f32 {
        let idle = self.idle.saturating_sub(prev.idle);
        let kernel = self.kernel.saturating_sub(prev.kernel);
        let user = self.user.saturating_sub(prev.user);
        let total = kernel.saturating_add(user);
        if total == 0 {
            return 0.0;
        }
        let busy = total.saturating_sub(idle);
        ((busy as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32
    }
}

/// Take a baseline sample so the next [`sample_cpu`] call can compute a delta.
#[must_use]
pub(super) fn baseline() -> Option<WindowsCpuState> {
    let total = read_total_times()?;
    let per_core = read_per_core_times().unwrap_or_default();
    Some(WindowsCpuState { total, per_core })
}

/// Sample total + per-core CPU %, updating `state` in place.
///
/// Returns `(0.0, [])` until a prior baseline exists.
#[must_use]
pub(super) fn sample_cpu(state: &mut Option<WindowsCpuState>) -> (f32, Vec<f32>) {
    let Some(now_total) = read_total_times() else {
        return (0.0, Vec::new());
    };
    let now_cores = read_per_core_times().unwrap_or_default();

    let Some(prev) = state.as_ref() else {
        *state = Some(WindowsCpuState {
            total: now_total,
            per_core: now_cores,
        });
        return (0.0, Vec::new());
    };

    let total = now_total.usage_since(prev.total);
    let per_core: Vec<f32> = now_cores
        .iter()
        .enumerate()
        .map(|(i, core)| {
            prev.per_core
                .get(i)
                .map(|p| core.usage_since(*p))
                .unwrap_or(0.0)
        })
        .collect();

    *state = Some(WindowsCpuState {
        total: now_total,
        per_core: now_cores,
    });
    (total, per_core)
}

fn filetime_to_u64(ft: FILETIME) -> u64 {
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

fn read_total_times() -> Option<CpuTimes> {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()?;
    }
    Some(CpuTimes {
        idle: filetime_to_u64(idle),
        kernel: filetime_to_u64(kernel),
        user: filetime_to_u64(user),
    })
}

/// `SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION` (ntdll).
#[repr(C)]
#[derive(Clone, Copy)]
struct SystemProcessorPerformanceInformation {
    idle_time: i64,
    kernel_time: i64,
    user_time: i64,
    dpc_time: i64,
    interrupt_time: i64,
    interrupt_count: u32,
}

const SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION_CLASS: u32 = 8;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQuerySystemInformation(
        system_information_class: u32,
        system_information: *mut core::ffi::c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

fn logical_processor_count() -> usize {
    let mut info = SYSTEM_INFO::default();
    unsafe { GetSystemInfo(&mut info) };
    info.dwNumberOfProcessors.max(1) as usize
}

fn read_per_core_times() -> Option<Vec<CpuTimes>> {
    let n = logical_processor_count();
    let mut buf = vec![MaybeUninit::<SystemProcessorPerformanceInformation>::uninit(); n];
    let bytes = (size_of::<SystemProcessorPerformanceInformation>() * n) as u32;
    let mut ret_len = 0u32;
    let status = unsafe {
        NtQuerySystemInformation(
            SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION_CLASS,
            buf.as_mut_ptr().cast(),
            bytes,
            &mut ret_len,
        )
    };
    if status < 0 {
        return None;
    }
    let infos = unsafe { assume_init_slice(&buf) };
    Some(
        infos
            .iter()
            .map(|i| CpuTimes {
                idle: i.idle_time as u64,
                kernel: i.kernel_time as u64,
                user: i.user_time as u64,
            })
            .collect(),
    )
}

unsafe fn assume_init_slice(
    buf: &[MaybeUninit<SystemProcessorPerformanceInformation>],
) -> &[SystemProcessorPerformanceInformation] {
    // SAFETY: NtQuerySystemInformation filled `buf` on success.
    unsafe { &*(buf as *const [_] as *const [SystemProcessorPerformanceInformation]) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn consecutive_samples_stay_in_range_and_can_change() {
        let mut state = baseline();
        assert!(state.is_some());
        thread::sleep(Duration::from_millis(300));
        let (a, cores_a) = sample_cpu(&mut state);
        thread::sleep(Duration::from_millis(300));
        let (b, cores_b) = sample_cpu(&mut state);
        assert!((0.0..=100.0).contains(&a), "a={a}");
        assert!((0.0..=100.0).contains(&b), "b={b}");
        assert!(!cores_a.is_empty());
        assert_eq!(cores_a.len(), cores_b.len());
        for c in cores_a.iter().chain(cores_b.iter()) {
            assert!((0.0..=100.0).contains(c), "core={c}");
        }
    }
}
