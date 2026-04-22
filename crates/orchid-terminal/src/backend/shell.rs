//! Native-shell backends: PowerShell and cmd.

/// Standard environment variables we set for every spawned shell.
pub(crate) fn base_env() -> Vec<(&'static str, &'static str)> {
    vec![
        ("TERM", "xterm-256color"),
        ("COLORTERM", "truecolor"),
        ("ORCHID_TERMINAL", "1"),
    ]
}

/// Resolve the PowerShell executable, preferring `pwsh.exe` (PowerShell 7+)
/// and falling back to `powershell.exe` (Windows PowerShell 5.1).
#[must_use]
pub(crate) fn resolve_powershell() -> &'static str {
    if which("pwsh.exe") {
        "pwsh.exe"
    } else {
        "powershell.exe"
    }
}

/// `cmd.exe` is always available on Windows and doesn't need to be resolved.
#[must_use]
pub(crate) fn cmd_exe() -> &'static str {
    "cmd.exe"
}

/// Minimal, allocation-free `PATH` search.
fn which(program: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path.split(sep) {
        let candidate = std::path::Path::new(dir).join(program);
        if candidate.is_file() {
            return true;
        }
    }
    false
}
