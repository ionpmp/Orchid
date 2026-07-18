//! `sysinfo`-backed process list provider.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use chrono::Utc;
use parking_lot::Mutex;
use sysinfo::{
    Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users,
};

use super::classify::classify_process;
use super::types::{ProcessSample, ProcessesSnapshot};
use super::windows::collect_app_pids;

/// Owns a long-lived [`System`] handle for process sampling.
pub struct ProcessesProvider {
    system: Mutex<System>,
    users: Mutex<Users>,
    last_refresh: Mutex<Option<Instant>>,
    /// Previous total I/O counters per pid for rate calculation.
    prev_io: Mutex<HashMap<u32, (u64, u64, Instant)>>,
    /// Cached Task Manager "Apps" PID set (EnumWindows is relatively expensive).
    app_pids: Mutex<(Instant, HashSet<u32>)>,
    /// Sample counter for cheap vs full refreshes.
    sample_n: AtomicU32,
}

impl std::fmt::Debug for ProcessesProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessesProvider").finish_non_exhaustive()
    }
}

impl Default for ProcessesProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessesProvider {
    /// New provider with a baseline process sample (CPU not yet meaningful).
    #[must_use]
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            process_refresh_kind(true),
        );
        let users = Users::new_with_refreshed_list();
        Self {
            system: Mutex::new(system),
            users: Mutex::new(users),
            last_refresh: Mutex::new(Some(Instant::now())),
            prev_io: Mutex::new(HashMap::new()),
            app_pids: Mutex::new((Instant::now(), HashSet::new())),
            sample_n: AtomicU32::new(0),
        }
    }

    /// Refresh and produce a process snapshot.
    pub fn refresh(&self) -> ProcessesSnapshot {
        let captured_at = Utc::now();
        let now = Instant::now();
        let n = self.sample_n.fetch_add(1, Ordering::Relaxed);
        // Disk/IO + user list every 3rd sample keeps the hot path lighter.
        let full = n.is_multiple_of(3);

        // Do not sleep here: the widget interval already spaces samples.
        // A blocking sleep on the worker made every tick feel like a hitch.
        *self.last_refresh.lock() = Some(now);

        if full {
            let mut users = self.users.lock();
            users.refresh_list();
        }

        {
            let mut system = self.system.lock();
            system.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                process_refresh_kind(full),
            );
        }

        let app_pids = self.app_pids_cached();
        let system = self.system.lock();
        let users = self.users.lock();
        let mut prev_io = self.prev_io.lock();
        let mut alive = HashSet::new();
        let mut processes = Vec::with_capacity(system.processes().len());

        for (pid, proc_) in system.processes() {
            let pid_u = pid.as_u32();
            alive.insert(pid_u);
            let name = proc_.name().to_string_lossy().into_owned();
            let path = proc_
                .exe()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let status = format!("{:?}", proc_.status());
            let cpu_percent = proc_.cpu_usage();
            let memory_bytes = proc_.memory();
            let disk = proc_.disk_usage();
            let io_read_bytes = disk.total_read_bytes;
            let io_write_bytes = disk.total_written_bytes;
            let (io_read_bps, io_write_bps) = match prev_io.get(&pid_u) {
                Some(&(prev_r, prev_w, at)) => {
                    let secs = at.elapsed().as_secs_f64().max(0.001);
                    let r = io_read_bytes.saturating_sub(prev_r) as f64 / secs;
                    let w = io_write_bytes.saturating_sub(prev_w) as f64 / secs;
                    (r as u64, w as u64)
                }
                None => (0, 0),
            };
            prev_io.insert(pid_u, (io_read_bytes, io_write_bytes, now));

            let user = proc_
                .user_id()
                .and_then(|uid| users.get_user_by_id(uid))
                .map(|u| u.name().to_string())
                .unwrap_or_default();
            let parent_pid = proc_.parent().map(|p| p.as_u32());
            let session_id = proc_.session_id().map(|s| s.as_u32());
            let group = classify_process(&name, &path, session_id, app_pids.contains(&pid_u));

            processes.push(ProcessSample {
                pid: pid_u,
                name,
                status,
                cpu_percent,
                memory_bytes,
                io_read_bytes,
                io_write_bytes,
                io_read_bps,
                io_write_bps,
                user,
                path,
                parent_pid,
                session_id,
                group,
            });
        }

        prev_io.retain(|pid, _| alive.contains(pid));
        drop(prev_io);
        drop(users);
        drop(system);

        ProcessesSnapshot {
            processes,
            captured_at,
        }
    }

    fn app_pids_cached(&self) -> HashSet<u32> {
        const TTL: Duration = Duration::from_secs(8);
        let mut guard = self.app_pids.lock();
        if guard.0.elapsed() < TTL && !guard.1.is_empty() {
            return guard.1.clone();
        }
        let pids = collect_app_pids();
        *guard = (Instant::now(), pids.clone());
        pids
    }

    /// Kill a single process by PID. Returns `true` on success.
    pub fn kill(&self, pid: u32) -> bool {
        let system = self.system.lock();
        system
            .process(Pid::from_u32(pid))
            .map(|p| p.kill())
            .unwrap_or(false)
    }

    /// Collect descendant PIDs (including `root`) from the last snapshot graph.
    pub fn collect_tree_pids(&self, root: u32) -> Vec<u32> {
        let system = self.system.lock();
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (pid, proc_) in system.processes() {
            if let Some(parent) = proc_.parent() {
                children
                    .entry(parent.as_u32())
                    .or_default()
                    .push(pid.as_u32());
            }
        }
        let mut out = Vec::new();
        let mut stack = vec![root];
        let mut seen = HashSet::new();
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            out.push(pid);
            if let Some(kids) = children.get(&pid) {
                stack.extend(kids.iter().copied());
            }
        }
        // Kill children before parents.
        out.reverse();
        out
    }

    /// Kill a process and its descendants. Returns how many kills succeeded.
    pub fn kill_tree(&self, root: u32) -> (usize, usize) {
        let pids = self.collect_tree_pids(root);
        let total = pids.len();
        let mut ok = 0usize;
        for pid in pids {
            if self.kill(pid) {
                ok += 1;
            }
        }
        (ok, total)
    }
}

fn process_refresh_kind(with_disk: bool) -> ProcessRefreshKind {
    let mut kind = ProcessRefreshKind::new()
        .with_cpu()
        .with_memory()
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_user(UpdateKind::OnlyIfNotSet);
    if with_disk {
        kind = kind.with_disk_usage();
    }
    kind
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_tree_includes_root() {
        let provider = ProcessesProvider::new();
        let me = sysinfo::get_current_pid().expect("pid").as_u32();
        let tree = provider.collect_tree_pids(me);
        assert!(tree.contains(&me));
    }
}
