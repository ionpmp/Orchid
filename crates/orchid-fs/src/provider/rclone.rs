//! Network filesystem access via the `rclone` CLI.

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::entry::{ExtendedAttributes, FsEntry, FsEntryKind, FsMetadata};
use crate::error::{FsError, Result};
use crate::operations::copy::CopyOptions;
use crate::operations::progress::{OperationProgress, ProgressSink};
use crate::path::FsPath;
use crate::provider::{FsCapabilities, FsProvider, FsProviderRegistry, ProviderId};

/// Schemes served by [`RcloneProvider`].
pub const RCLONE_SCHEMES: &[&str] = &["sftp", "smb", "webdav", "ftp"];

/// Network provider backed by `rclone lsjson` / `rclone cat`.
pub struct RcloneProvider {
    id: ProviderId,
    scheme: &'static str,
    mounts: Arc<RwLock<Vec<orchid_storage::NetworkMountConfig>>>,
    rclone_bin: String,
}

impl RcloneProvider {
    /// Build a provider for one URL scheme.
    #[must_use]
    pub fn new(
        scheme: &'static str,
        mounts: Arc<RwLock<Vec<orchid_storage::NetworkMountConfig>>>,
    ) -> Self {
        Self {
            id: ProviderId::new(format!("rclone-{scheme}")),
            scheme,
            mounts,
            rclone_bin: std::env::var("RCLONE_BIN").unwrap_or_else(|_| "rclone".into()),
        }
    }

    fn resolve_mount(&self, path: &FsPath) -> Result<ResolvedMount> {
        let path_key = path.as_str();
        let mounts = self.mounts.read();
        let mut winner: Option<(usize, String, usize)> = None;
        for (i, mount) in mounts.iter().enumerate() {
            if !mount.enabled {
                continue;
            }
            let Some(root) = normalize_mount_uri(&mount.uri) else {
                continue;
            };
            let Ok(root_path) = FsPath::new(&root) else {
                continue;
            };
            if root_path.scheme() != self.scheme {
                continue;
            }
            if path_key == root {
                if winner.as_ref().map(|(_, _, score)| *score).unwrap_or(0) < root.len() {
                    winner = Some((i, String::new(), root.len()));
                }
            } else if let Some(rest) = path_key.strip_prefix(&format!("{root}/")) {
                let rel = rest.trim_start_matches('/').to_string();
                if winner.as_ref().map(|(_, _, score)| *score).unwrap_or(0) < root.len() {
                    winner = Some((i, rel, root.len()));
                }
            }
        }
        let Some((idx, rel, _)) = winner else {
            return Err(FsError::ProviderNotMounted(path.as_str().to_string()));
        };
        let mount = mounts
            .get(idx)
            .cloned()
            .ok_or_else(|| FsError::ProviderNotMounted(path.as_str().to_string()))?;
        Ok(ResolvedMount {
            mount,
            relative_path: rel,
        })
    }

    async fn remote_for_path(&self, path: &FsPath) -> Result<String> {
        let resolved = self.resolve_mount(path)?;
        self.rclone_remote_spec(&resolved).await
    }

    async fn run_rclone(&self, args: &[&str]) -> Result<()> {
        let output = self.spawn_rclone(args).await?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(FsError::InvalidPath {
                reason: format!(
                    "rclone {} failed: {}",
                    args.first().copied().unwrap_or(""),
                    stderr.trim()
                ),
            })
        }
    }

    async fn spawn_rclone(&self, args: &[&str]) -> Result<std::process::Output> {
        Command::new(&self.rclone_bin)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    FsError::InvalidPath {
                        reason: format!(
                            "`{}` not found; install rclone and ensure it is on PATH (or set RCLONE_BIN)",
                            self.rclone_bin
                        ),
                    }
                } else {
                    FsError::Io(e)
                }
            })
    }

    /// Run `rclone copy` (or similar long transfers) and stream `--stats-one-line`
    /// progress to the sink when provided.
    async fn run_rclone_with_progress(
        &self,
        args: &[&str],
        progress: Option<&ProgressSink>,
        dest_path: &FsPath,
    ) -> Result<()> {
        let mut cmd_args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        if cmd_args.first().map(String::as_str) == Some("copy") {
            cmd_args.push("--stats-one-line".into());
            cmd_args.push("--stats".into());
            cmd_args.push("500ms".into());
        }

        let arg_refs: Vec<&str> = cmd_args.iter().map(String::as_str).collect();
        let mut child = Command::new(&self.rclone_bin)
            .args(&arg_refs)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    FsError::InvalidPath {
                        reason: format!(
                            "`{}` not found; install rclone and ensure it is on PATH (or set RCLONE_BIN)",
                            self.rclone_bin
                        ),
                    }
                } else {
                    FsError::Io(e)
                }
            })?;

        if let (Some(sink), Some(stderr)) = (progress.cloned(), child.stderr.take()) {
            let dest = dest_path.clone();
            let mut reader = BufReader::new(stderr).lines();
            let progress_task = tokio::spawn(async move {
                while let Ok(Some(line)) = reader.next_line().await {
                    if let Some(pct) = parse_rclone_stats_percent(&line) {
                        sink.send(OperationProgress {
                            total_bytes: 100,
                            processed_bytes: pct,
                            current_path: dest.clone(),
                            items_processed: 0,
                            items_total: 0,
                        });
                    }
                }
            });
            let status = child.wait().await.map_err(FsError::Io)?;
            progress_task.abort();
            let _ = progress_task.await;
            if status.success() {
                return Ok(());
            }
            return Err(FsError::InvalidPath {
                reason: format!(
                    "rclone {} failed: exit {}",
                    args.first().copied().unwrap_or(""),
                    status.code().unwrap_or(-1)
                ),
            });
        }

        let output = child.wait_with_output().await.map_err(FsError::Io)?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(FsError::InvalidPath {
                reason: format!(
                    "rclone {} failed: {}",
                    args.first().copied().unwrap_or(""),
                    stderr.trim()
                ),
            })
        }
    }

    async fn rclone_remote_spec(&self, resolved: &ResolvedMount) -> Result<String> {
        if let Some(remote) = resolved.mount.rclone_remote.as_deref() {
            let tail = resolved.relative_path.trim_start_matches('/');
            return if tail.is_empty() {
                Ok(format!("{remote}:"))
            } else {
                Ok(format!("{remote}:{tail}"))
            };
        }
        let Some(root) = normalize_mount_uri(&resolved.mount.uri) else {
            return Err(FsError::InvalidPath {
                reason: format!("invalid mount uri: {}", resolved.mount.uri),
            });
        };
        let Ok(root_path) = FsPath::new(&root) else {
            return Err(FsError::InvalidPath {
                reason: format!("invalid mount uri: {}", resolved.mount.uri),
            });
        };
        let body = root_path.as_str()[root_path.scheme().len() + 1..].trim_start_matches('/');
        let (host_part, root_tail) = body.split_once('/').unwrap_or((body, ""));
        let mut params = vec![format!("host={host_part}")];
        if let Some(user) = resolved.mount.user.as_deref().filter(|u| !u.is_empty()) {
            params.push(format!("user={user}"));
        }
        if let Some(pass) = resolved.mount.password.as_deref().filter(|p| !p.is_empty()) {
            if resolved.mount.rclone_remote.is_none() {
                tracing::warn!(
                    mount = %resolved.mount.name,
                    "network mount uses inline password; prefer rclone-remote \
                     (password is plaintext in config.toml and visible in rclone argv)"
                );
            }
            params.push(format!("pass={pass}"));
        }
        let subpath = if !resolved.relative_path.is_empty() {
            if root_tail.is_empty() {
                resolved.relative_path.trim_start_matches('/').to_string()
            } else {
                format!(
                    "{}/{}",
                    root_tail.trim_end_matches('/'),
                    resolved.relative_path.trim_start_matches('/')
                )
            }
        } else {
            root_tail.trim_start_matches('/').to_string()
        };
        let param_str = params.join(",");
        if subpath.is_empty() {
            Ok(format!(":{},{}:", self.scheme, param_str))
        } else {
            Ok(format!(":{},{}:{subpath}", self.scheme, param_str))
        }
    }

    async fn run_lsjson(&self, remote: &str) -> Result<Vec<RcloneEntry>> {
        let output = Command::new(&self.rclone_bin)
            .args(["lsjson", remote])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    FsError::InvalidPath {
                        reason: format!(
                            "`{}` not found; install rclone and ensure it is on PATH (or set RCLONE_BIN)",
                            self.rclone_bin
                        ),
                    }
                } else {
                    FsError::Io(e)
                }
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FsError::InvalidPath {
                reason: format!("rclone lsjson failed: {}", stderr.trim()),
            });
        }
        if output.stdout.is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_slice(&output.stdout).map_err(|e| FsError::InvalidPath {
            reason: format!("rclone lsjson parse error: {e}"),
        })
    }

    fn entry_from_rclone(&self, parent: &FsPath, row: &RcloneEntry) -> Result<FsEntry> {
        let name = row.name.clone();
        let child = parent.join(&name);
        let modified = row
            .mod_time
            .as_deref()
            .and_then(parse_rclone_time);
        Ok(FsEntry {
            name: name.clone(),
            path: child,
            metadata: FsMetadata {
                kind: if row.is_dir {
                    FsEntryKind::Directory
                } else {
                    FsEntryKind::File
                },
                size: row.size.unwrap_or(0),
                created: None,
                modified,
                accessed: None,
                readonly: false,
                hidden: name_starts_hidden(&name),
                system: false,
                mime: row.mime_type.clone(),
                extended: ExtendedAttributes::default(),
            },
        })
    }
}

struct ResolvedMount {
    mount: orchid_storage::NetworkMountConfig,
    relative_path: String,
}

#[derive(Debug, Deserialize)]
struct RcloneEntry {
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "IsDir", default)]
    is_dir: bool,
    #[serde(rename = "Size", default)]
    size: Option<u64>,
    #[serde(rename = "ModTime", default)]
    mod_time: Option<String>,
    #[serde(rename = "MimeType", default)]
    mime_type: Option<String>,
}

#[async_trait]
impl FsProvider for RcloneProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn scheme(&self) -> &'static str {
        self.scheme
    }

    async fn list(&self, path: &FsPath) -> Result<Vec<FsEntry>> {
        let resolved = self.resolve_mount(path)?;
        let remote = self.rclone_remote_spec(&resolved).await?;
        let rows = self.run_lsjson(&remote).await?;
        rows.iter()
            .map(|row| self.entry_from_rclone(path, row))
            .collect()
    }

    async fn metadata(&self, path: &FsPath) -> Result<FsMetadata> {
        let parent = path
            .parent()
            .ok_or_else(|| FsError::NotFound(path.as_str().to_string()))?;
        let name = path
            .file_name()
            .ok_or_else(|| FsError::NotFound(path.as_str().to_string()))?;
        let entries = self.list(&parent).await?;
        entries
            .into_iter()
            .find(|e| e.name == name)
            .map(|e| e.metadata)
            .ok_or_else(|| FsError::NotFound(path.as_str().to_string()))
    }

    async fn exists(&self, path: &FsPath) -> Result<bool> {
        match self.metadata(path).await {
            Ok(_) => Ok(true),
            Err(FsError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn read(&self, path: &FsPath) -> Result<Vec<u8>> {
        let resolved = self.resolve_mount(path)?;
        let remote = self.rclone_remote_spec(&resolved).await?;
        let output = Command::new(&self.rclone_bin)
            .args(["cat", &remote])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(FsError::Io)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FsError::InvalidPath {
                reason: format!("rclone cat failed: {}", stderr.trim()),
            });
        }
        Ok(output.stdout)
    }

    async fn read_stream(
        &self,
        path: &FsPath,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
        let bytes = self.read(path).await?;
        Ok(Box::new(std::io::Cursor::new(bytes)))
    }

    async fn write(&self, path: &FsPath, bytes: &[u8]) -> Result<()> {
        let remote = self.remote_for_path(path).await?;
        let mut child = Command::new(&self.rclone_bin)
            .args(["rcat", &remote])
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    FsError::InvalidPath {
                        reason: format!(
                            "`{}` not found; install rclone and ensure it is on PATH (or set RCLONE_BIN)",
                            self.rclone_bin
                        ),
                    }
                } else {
                    FsError::Io(e)
                }
            })?;
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(bytes).await.map_err(FsError::Io)?;
        }
        let output = child.wait_with_output().await.map_err(FsError::Io)?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(FsError::InvalidPath {
                reason: format!("rclone rcat failed: {}", stderr.trim()),
            })
        }
    }

    async fn write_stream(
        &self,
        path: &FsPath,
    ) -> Result<Box<dyn tokio::io::AsyncWrite + Unpin + Send>> {
        let remote = self.remote_for_path(path).await?;
        let mut child = Command::new(&self.rclone_bin)
            .args(["rcat", &remote])
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(FsError::Io)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| FsError::InvalidPath {
                reason: "rclone rcat stdin unavailable".into(),
            })?;
        Ok(Box::new(RcloneWriteHandle { child, stdin }))
    }

    async fn create_dir(&self, path: &FsPath, _recursive: bool) -> Result<()> {
        let remote = self.remote_for_path(path).await?;
        self.run_rclone(&["mkdir", &remote]).await
    }

    async fn rename(&self, from: &FsPath, to: &FsPath) -> Result<()> {
        let src = self.remote_for_path(from).await?;
        let dst = self.remote_for_path(to).await?;
        self.run_rclone(&["moveto", &src, &dst]).await
    }

    async fn remove(&self, path: &FsPath, recursive: bool) -> Result<()> {
        let remote = self.remote_for_path(path).await?;
        if recursive {
            return self.run_rclone(&["purge", &remote]).await;
        }
        match self.metadata(path).await {
            Ok(meta) if matches!(meta.kind, FsEntryKind::Directory) => {
                self.run_rclone(&["rmdir", &remote]).await
            }
            Ok(_) => self.run_rclone(&["deletefile", &remote]).await,
            Err(e) => Err(e),
        }
    }

    async fn watch(
        &self,
        _path: &FsPath,
    ) -> Result<Option<Box<dyn crate::provider::FsWatcherHandle>>> {
        Ok(None)
    }

    fn capabilities(&self) -> FsCapabilities {
        FsCapabilities {
            supports_rename: true,
            supports_symlinks: false,
            supports_permissions: false,
            supports_extended_attrs: false,
            supports_native_watch: false,
            case_sensitive: true,
            supports_random_write: false,
        }
    }

    async fn copy_cross_scheme(
        &self,
        registry: &FsProviderRegistry,
        from: &FsPath,
        to: &FsPath,
        options: CopyOptions,
        progress: Option<&ProgressSink>,
    ) -> Result<bool> {
        let (_local_path, remote_path, local_is_src) = match (from.is_local(), to.is_local()) {
            (true, false) if to.scheme() == self.scheme && self.resolve_mount(to).is_ok() => {
                (from, to, true)
            }
            (false, true) if from.scheme() == self.scheme && self.resolve_mount(from).is_ok() => {
                (from, to, false)
            }
            _ => return Ok(false),
        };

        if !options.overwrite {
            let dst = registry
                .for_path(to)
                .ok_or_else(|| FsError::ProviderNotMounted(to.to_string()))?;
            if dst.exists(to).await? {
                return Err(FsError::AlreadyExists(to.to_string()));
            }
        }

        let meta = if local_is_src {
            registry
                .for_path(from)
                .ok_or_else(|| FsError::ProviderNotMounted(from.to_string()))?
                .metadata(from)
                .await?
        } else {
            self.metadata(from).await?
        };

        let local_os = if local_is_src {
            from.to_local()?
        } else {
            to.to_local()?
        };
        let local_arg = local_os
            .to_str()
            .ok_or_else(|| FsError::InvalidPath {
                reason: format!("non-UTF8 local path: {}", local_os.display()),
            })?;
        let remote_spec = self.remote_for_path(remote_path).await?;

        let is_dir = matches!(meta.kind, FsEntryKind::Directory);
        let args: Vec<String> = if is_dir {
            if local_is_src {
                vec![
                    "copy".into(),
                    local_arg.into(),
                    remote_spec,
                    "--create-empty-src-dirs".into(),
                ]
            } else {
                vec![
                    "copy".into(),
                    remote_spec,
                    local_arg.into(),
                    "--create-empty-src-dirs".into(),
                ]
            }
        } else if local_is_src {
            vec!["copyto".into(), local_arg.into(), remote_spec]
        } else {
            vec!["copyto".into(), remote_spec, local_arg.into()]
        };

        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        if is_dir {
            self.run_rclone_with_progress(&arg_refs, progress, to).await?;
        } else {
            self.run_rclone(&arg_refs).await?;
            if let Some(p) = progress {
                let size = meta.size;
                p.send(OperationProgress {
                    total_bytes: size,
                    processed_bytes: size,
                    current_path: to.clone(),
                    items_processed: 1,
                    items_total: 1,
                });
            }
        }
        Ok(true)
    }

    async fn move_cross_scheme(
        &self,
        registry: &FsProviderRegistry,
        from: &FsPath,
        to: &FsPath,
        progress: Option<&ProgressSink>,
    ) -> Result<bool> {
        let (_local_path, remote_path, local_is_src) = match (from.is_local(), to.is_local()) {
            (true, false) if to.scheme() == self.scheme && self.resolve_mount(to).is_ok() => {
                (from, to, true)
            }
            (false, true) if from.scheme() == self.scheme && self.resolve_mount(from).is_ok() => {
                (from, to, false)
            }
            _ => return Ok(false),
        };

        let dst = registry
            .for_path(to)
            .ok_or_else(|| FsError::ProviderNotMounted(to.to_string()))?;
        if dst.exists(to).await? {
            return Err(FsError::AlreadyExists(to.to_string()));
        }

        let meta = if local_is_src {
            registry
                .for_path(from)
                .ok_or_else(|| FsError::ProviderNotMounted(from.to_string()))?
                .metadata(from)
                .await?
        } else {
            self.metadata(from).await?
        };

        let local_os = if local_is_src {
            from.to_local()?
        } else {
            to.to_local()?
        };
        let local_arg = local_os
            .to_str()
            .ok_or_else(|| FsError::InvalidPath {
                reason: format!("non-UTF8 local path: {}", local_os.display()),
            })?;
        let remote_spec = self.remote_for_path(remote_path).await?;

        let is_dir = matches!(meta.kind, FsEntryKind::Directory);
        let args: Vec<String> = if is_dir {
            if local_is_src {
                vec![
                    "move".into(),
                    local_arg.into(),
                    remote_spec,
                    "--create-empty-src-dirs".into(),
                ]
            } else {
                vec![
                    "move".into(),
                    remote_spec,
                    local_arg.into(),
                    "--create-empty-src-dirs".into(),
                ]
            }
        } else if local_is_src {
            vec!["moveto".into(), local_arg.into(), remote_spec]
        } else {
            vec!["moveto".into(), remote_spec, local_arg.into()]
        };

        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        if is_dir {
            self.run_rclone_with_progress(&arg_refs, progress, to).await?;
        } else {
            self.run_rclone(&arg_refs).await?;
            if let Some(p) = progress {
                let size = meta.size;
                p.send(OperationProgress {
                    total_bytes: size,
                    processed_bytes: size,
                    current_path: to.clone(),
                    items_processed: 1,
                    items_total: 1,
                });
            }
        }
        Ok(true)
    }
}

struct RcloneWriteHandle {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
}

impl tokio::io::AsyncWrite for RcloneWriteHandle {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.stdin).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdin).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}

impl Drop for RcloneWriteHandle {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Parse `Transferred: …, 42%, …` from `rclone --stats-one-line` stderr.
fn parse_rclone_stats_percent(line: &str) -> Option<u64> {
    if !line.contains("Transferred:") {
        return None;
    }
    line.split(',')
        .map(str::trim)
        .find(|part| part.ends_with('%'))
        .and_then(|part| part.trim_end_matches('%').trim().parse().ok())
}

/// Register one [`RcloneProvider`] per supported scheme.
pub fn register_rclone_providers(
    registry: &crate::provider::FsProviderRegistry,
    mounts: Arc<RwLock<Vec<orchid_storage::NetworkMountConfig>>>,
) -> Result<()> {
    for scheme in RCLONE_SCHEMES {
        registry.register(Arc::new(RcloneProvider::new(scheme, mounts.clone()))
            as Arc<dyn FsProvider>)?;
    }
    Ok(())
}

/// Convert `sftp://host/path` or `sftp:host/path` into canonical Orchid path syntax.
#[must_use]
pub fn normalize_mount_uri(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with("://") {
        return None;
    }
    if let Some(colon) = trimmed.find("://") {
        let scheme = &trimmed[..colon];
        let rest = trimmed[colon + 3..].trim_start_matches('/');
        let (auth_host, path) = split_auth_host_path(rest);
        let host = auth_host
            .split('@')
            .next_back()
            .unwrap_or(auth_host)
            .split(':')
            .next()
            .unwrap_or(auth_host);
        let candidate = if path.is_empty() {
            format!("{scheme}:{host}")
        } else {
            format!("{scheme}:{host}/{path}")
        };
        return FsPath::new(&candidate).ok().map(|p| p.as_str().to_string());
    }
    FsPath::new(trimmed)
        .ok()
        .map(|p| p.as_str().to_string())
}

fn split_auth_host_path(rest: &str) -> (&str, &str) {
    if let Some(slash) = rest.find('/') {
        (&rest[..slash], rest[slash + 1..].trim_start_matches('/'))
    } else {
        (rest, "")
    }
}

fn parse_rclone_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn name_starts_hidden(name: &str) -> bool {
    Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_sftp_url() {
        assert_eq!(
            normalize_mount_uri("sftp://user@myserver/home/alice"),
            Some("sftp:myserver/home/alice".into())
        );
    }

    #[test]
    fn normalize_sftp_colon_form() {
        assert_eq!(
            normalize_mount_uri("sftp:myserver/home/alice"),
            Some("sftp:myserver/home/alice".into())
        );
    }

    #[test]
    fn parse_stats_percent() {
        assert_eq!(
            parse_rclone_stats_percent(
                "Transferred:   	          1.234 GiB / 5.678 GiB, 22%, 1.234 MiB/s, ETA 4m12s"
            ),
            Some(22)
        );
    }
}
