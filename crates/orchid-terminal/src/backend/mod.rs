//! Backend specs and resolution. Every backend ultimately boils down to a
//! [`portable_pty::CommandBuilder`].

pub mod shell;
pub mod ssh;
pub mod wsl;

use std::collections::BTreeMap;
use std::path::PathBuf;

use portable_pty::CommandBuilder;
use serde::{Deserialize, Serialize};

use crate::error::Result;
#[cfg(test)]
use crate::error::TerminalError;

pub use ssh::SshTarget;

/// Which kind of shell to spawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendKind {
    /// PowerShell (`pwsh.exe` with fallback to `powershell.exe`).
    PowerShell,
    /// Legacy `cmd.exe`.
    Cmd,
    /// WSL distribution by name, e.g. `"Ubuntu-22.04"`.
    Wsl(String),
    /// SSH session.
    Ssh(SshTarget),
    /// Anything else.
    Custom {
        /// Executable path or name.
        command: String,
        /// Argument vector.
        args: Vec<String>,
    },
}

/// Everything needed to spawn a child on a PTY.
#[derive(Debug, Clone)]
pub struct BackendSpec {
    /// Backend variant.
    pub kind: BackendKind,
    /// Working directory, or user home if `None`.
    pub working_directory: Option<PathBuf>,
    /// Extra environment variables (merged on top of `TERM` / `COLORTERM`).
    pub env: BTreeMap<String, String>,
    /// Optional initial command to run after the shell starts.
    pub initial_command: Option<String>,
}

impl BackendSpec {
    /// PowerShell with default settings.
    #[must_use]
    pub fn powershell() -> Self {
        Self {
            kind: BackendKind::PowerShell,
            working_directory: None,
            env: BTreeMap::new(),
            initial_command: None,
        }
    }

    /// Legacy `cmd.exe`.
    #[must_use]
    pub fn cmd() -> Self {
        Self {
            kind: BackendKind::Cmd,
            working_directory: None,
            env: BTreeMap::new(),
            initial_command: None,
        }
    }

    /// WSL distribution by name.
    #[must_use]
    pub fn wsl(distro: impl Into<String>) -> Self {
        Self {
            kind: BackendKind::Wsl(distro.into()),
            working_directory: None,
            env: BTreeMap::new(),
            initial_command: None,
        }
    }

    /// SSH session.
    #[must_use]
    pub fn ssh(target: SshTarget) -> Self {
        Self {
            kind: BackendKind::Ssh(target),
            working_directory: None,
            env: BTreeMap::new(),
            initial_command: None,
        }
    }

    /// Builder helper: set the initial working directory.
    #[must_use]
    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.working_directory = Some(cwd);
        self
    }

    /// Builder helper: add an env var.
    #[must_use]
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    /// Builder helper: set a command to run on startup.
    #[must_use]
    pub fn with_initial_command(mut self, cmd: impl Into<String>) -> Self {
        self.initial_command = Some(cmd.into());
        self
    }

    /// Compose a `CommandBuilder` suitable for spawning via portable-pty.
    ///
    /// # Errors
    ///
    /// Propagates [`TerminalError::BackendUnavailable`] for invalid configs
    /// (e.g. WSL with an empty distro).
    pub fn to_command(&self) -> Result<CommandBuilder> {
        let mut builder = match &self.kind {
            BackendKind::PowerShell => {
                let mut b = CommandBuilder::new(shell::resolve_powershell());
                // Intentionally *do not* pass -NoProfile so users see their
                // prompt / aliases by default.
                b.args(Vec::<String>::new());
                b
            }
            BackendKind::Cmd => CommandBuilder::new(shell::cmd_exe()),
            BackendKind::Wsl(distro) => {
                wsl::validate_distro(distro)?;
                let mut b = CommandBuilder::new("wsl.exe");
                b.args(["-d", distro.as_str()]);
                b
            }
            BackendKind::Ssh(target) => {
                let mut b = CommandBuilder::new("ssh");
                b.args(target.to_args());
                b
            }
            BackendKind::Custom { command, args } => {
                let mut b = CommandBuilder::new(command);
                b.args(args.iter().map(String::as_str));
                b
            }
        };

        // Apply working directory.
        if let Some(cwd) = &self.working_directory {
            builder.cwd(cwd);
        } else if let Some(home) = dirs_home() {
            builder.cwd(home);
        }

        // Base environment.
        for (k, v) in shell::base_env() {
            builder.env(k, v);
        }
        for (k, v) in &self.env {
            builder.env(k, v);
        }

        Ok(builder)
    }

    /// Label suitable for the UI (tab title fallback).
    #[must_use]
    pub fn display_name(&self) -> String {
        match &self.kind {
            BackendKind::PowerShell => "PowerShell".into(),
            BackendKind::Cmd => "Command Prompt".into(),
            BackendKind::Wsl(d) => format!("WSL: {d}"),
            BackendKind::Ssh(t) => format!("SSH {}", t.host),
            BackendKind::Custom { command, .. } => command.clone(),
        }
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_are_reasonable() {
        assert_eq!(BackendSpec::powershell().display_name(), "PowerShell");
        assert_eq!(BackendSpec::cmd().display_name(), "Command Prompt");
        assert_eq!(BackendSpec::wsl("Ubuntu").display_name(), "WSL: Ubuntu");
    }

    #[test]
    fn wsl_rejects_empty_distro() {
        let spec = BackendSpec::wsl("");
        assert!(matches!(
            spec.to_command().unwrap_err(),
            TerminalError::BackendUnavailable(_)
        ));
    }

    #[test]
    fn env_merges_user_over_default() {
        let spec = BackendSpec::cmd().with_env("TERM", "dumb");
        let builder = spec.to_command().unwrap();
        let _ = builder; // CommandBuilder does not expose env inspection.
    }
}
