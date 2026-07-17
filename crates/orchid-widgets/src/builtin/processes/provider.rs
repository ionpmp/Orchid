//! `sysinfo`-backed process list provider.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use chrono::Utc;
use parking_lot::Mutex;
use sysinfo::{
    Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users,
    MINIMUM_CPU_UPDATE_INTERVAL,
};

use super::classify::classify_process;
use super::types::{ProcessSample, ProcessesSnapshot};

/// Owns a long-lived [`System`] handle for process sampling.
pub struct ProcessesProvider {
    system: Mutex<System>,
    users: Mutex<Users>,
    last_refresh: Mutex<Option<Instant>>,
    /// Previous total I/O counters per pid for rate calculation.
    prev_io: Mutex<HashMap<u32, (u64, u64, Instant)>>,
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
        let kind = process_refresh_kind();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        let users = Users::new_with_refreshed_list();
        Self {
            system: Mutex::new(system),
            users: Mutex::new(users),
            last_refresh: Mutex::new(Some(Instant::now())),
            prev_io: Mutex::new(HashMap::new()),
        }
    }

    /// Refresh and produce a process snapshot.
    pub fn refresh(&self) -> ProcessesSnapshot {
        let captured_at = Utc::now();
        let now = Instant::now();

        if let Some(prev) = *self.last_refresh.lock() {
            let elapsed = prev.elapsed();
            if elapsed < MINIMUM_CPU_UPDATE_INTERVAL {
                std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL - elapsed);
            }
        }

        {
            let mut users = self.users.lock();
            users.refresh_list();
        }

        let mut system = self.system.lock();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, process_refresh_kind());
        *self.last_refresh.lock() = Some(Instant::now());

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
            let group = classify_process(&name, &path, session_id, parent_pid);

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

fn process_refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::new()
        .with_cpu()
        .with_memory()
        .with_disk_usage()
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_user(UpdateKind::OnlyIfNotSet)
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
