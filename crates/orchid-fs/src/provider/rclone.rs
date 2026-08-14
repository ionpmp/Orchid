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
///
/// `ftps` uses rclone's `ftp` backend with explicit TLS. `scp` uses `sftp`
/// (OpenSSH `scp` is SFTP-based). Cloud backends (`s3`, `drive`, `onedrive`,
/// `dropbox`) prefer a named `rclone-remote`.
pub const RCLONE_SCHEMES: &[&str] = &[
    "sftp", "smb", "webdav", "ftp", "ftps", "scp", "s3", "drive", "onedrive", "dropbox",
];

/// rclone backend name for an Orchid URI scheme.
#[must_use]
pub fn rclone_backend(scheme: &str) -> &str {
    match scheme {
        "ftps" => "ftp",
        "scp" => "sftp",
        other => other,
    }
}

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
        self.resolve_mount_for_scheme(path, Some(self.scheme))
    }

    fn resolve_any_mount(&self, path: &FsPath) -> Result<ResolvedMount> {
        self.resolve_mount_for_scheme(path, None)
    }

    fn resolve_mount_for_scheme(
        &self,
        path: &FsPath,
        scheme_filter: Option<&str>,
    ) -> Result<ResolvedMount> {
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
            if let Some(want) = scheme_filter {
                if root_path.scheme() != want {
                    continue;
                }
            } else if !RCLONE_SCHEMES.contains(&root_path.scheme()) {
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
        self.rclone_remote_spec(&resolved)
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
                    redact_secrets(stderr.trim())
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
                    redact_secrets(stderr.trim())
                ),
            })
        }
    }

    fn rclone_remote_spec(&self, resolved: &ResolvedMount) -> Result<String> {
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
        let scheme = root_path.scheme();
        let backend = rclone_backend(scheme);
        if matches!(scheme, "drive" | "onedrive" | "dropbox") {
            return Err(FsError::InvalidPath {
                reason: format!(
                    "mount `{}` ({scheme}) needs rclone-remote (OAuth remote in rclone.conf)",
                    resolved.mount.name
                ),
            });
        }
        let body = root_path.as_str()[scheme.len() + 1..].trim_start_matches('/');
        if scheme == "s3" {
            let params = inline_backend_params(scheme, "", &resolved.mount)?;
            let subpath = join_rel(body, &resolved.relative_path);
            return Ok(format_inline_remote(backend, &params, &subpath));
        }
        let (host_part, root_tail) = body.split_once('/').unwrap_or((body, ""));
        let params = inline_backend_params(scheme, host_part, &resolved.mount)?;
        let subpath = join_rel(root_tail, &resolved.relative_path);
        Ok(format_inline_remote(backend, &params, &subpath))
    }

    fn try_remote_spec(&self, path: &FsPath) -> Result<String> {
        let resolved = self.resolve_any_mount(path)?;
        self.rclone_remote_spec(&resolved)
    }

    fn local_os_arg(path: &FsPath) -> Result<String> {
        let os = path.to_local()?;
        os.to_str()
            .map(str::to_string)
            .ok_or_else(|| FsError::InvalidPath {
                reason: format!("non-UTF8 local path: {}", os.display()),
            })
    }

    fn transfer_spec(&self, path: &FsPath) -> Result<String> {
        if path.is_local() {
            Self::local_os_arg(path)
        } else {
            self.try_remote_spec(path)
        }
    }

    /// One-way `rclone sync` (destination mirrors source). At least one side
    /// must be an rclone mount.
    pub async fn sync_paths(
        &self,
        from: &FsPath,
        to: &FsPath,
        progress: Option<&ProgressSink>,
    ) -> Result<()> {
        if from.is_local() && to.is_local() {
            return Err(FsError::InvalidPath {
                reason: "rclone sync needs a network mount on at least one side".into(),
            });
        }
        let src = self.transfer_spec(from)?;
        let dst = self.transfer_spec(to)?;
        let args = [
            "sync",
            src.as_str(),
            dst.as_str(),
            "--create-empty-src-dirs",
        ];
        self.run_rclone_with_progress(&args, progress, to).await
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
                reason: format!("rclone lsjson failed: {}", redact_secrets(stderr.trim())),
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
        let modified = row.mod_time.as_deref().and_then(parse_rclone_time);
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
        let remote = self.rclone_remote_spec(&resolved)?;
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
        let remote = self.rclone_remote_spec(&resolved)?;
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
                reason: format!("rclone cat failed: {}", redact_secrets(stderr.trim())),
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
                reason: format!("rclone rcat failed: {}", redact_secrets(stderr.trim())),
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
        let stdin = child.stdin.take().ok_or_else(|| FsError::InvalidPath {
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
        _recursive: bool,
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
        self.transfer_cross_scheme(registry, from, to, false, Some(options), progress)
            .await
    }

    async fn move_cross_scheme(
        &self,
        registry: &FsProviderRegistry,
        from: &FsPath,
        to: &FsPath,
        progress: Option<&ProgressSink>,
    ) -> Result<bool> {
        self.transfer_cross_scheme(registry, from, to, true, None, progress)
            .await
    }

    async fn sync_cross_scheme(
        &self,
        from: &FsPath,
        to: &FsPath,
        progress: Option<&ProgressSink>,
    ) -> Result<bool> {
        if from.is_local() && to.is_local() {
            return Ok(false);
        }
        if !from.is_local() && self.try_remote_spec(from).is_err() {
            return Ok(false);
        }
        if !to.is_local() && self.try_remote_spec(to).is_err() {
            return Ok(false);
        }
        self.sync_paths(from, to, progress).await?;
        Ok(true)
    }
}

impl RcloneProvider {
    async fn transfer_cross_scheme(
        &self,
        registry: &FsProviderRegistry,
        from: &FsPath,
        to: &FsPath,
        is_move: bool,
        options: Option<CopyOptions>,
        progress: Option<&ProgressSink>,
    ) -> Result<bool> {
        if from.is_local() && to.is_local() {
            return Ok(false);
        }
        let src_spec = match self.transfer_spec(from) {
            Ok(s) => s,
            Err(_) => return Ok(false),
        };
        let dst_spec = match self.transfer_spec(to) {
            Ok(s) => s,
            Err(_) => return Ok(false),
        };

        let overwrite = options.map(|o| o.overwrite).unwrap_or(false);
        if !overwrite || is_move {
            let dst = registry
                .for_path(to)
                .ok_or_else(|| FsError::ProviderNotMounted(to.to_string()))?;
            if dst.exists(to).await? {
                return Err(FsError::AlreadyExists(to.to_string()));
            }
        }

        let src_provider = registry
            .for_path(from)
            .ok_or_else(|| FsError::ProviderNotMounted(from.to_string()))?;
        let meta = src_provider.metadata(from).await?;
        let is_dir = matches!(meta.kind, FsEntryKind::Directory);
        let args = rclone_transfer_args(
            src_spec,
            dst_spec,
            is_dir,
            is_move,
            options.unwrap_or_default(),
        );
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        if is_dir {
            self.run_rclone_with_progress(&arg_refs, progress, to)
                .await?;
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

/// True when `scheme` is served by [`RcloneProvider`].
#[must_use]
pub fn is_rclone_scheme(scheme: &str) -> bool {
    RCLONE_SCHEMES.contains(&scheme)
}

/// One-way `rclone sync` when at least one path is a configured rclone mount.
///
/// # Errors
///
/// Missing provider, unresolvable mount, or rclone failure.
pub async fn rclone_sync(
    registry: &FsProviderRegistry,
    from: &FsPath,
    to: &FsPath,
    progress: Option<&ProgressSink>,
) -> Result<()> {
    if let Some(provider) = registry.for_path(from) {
        if provider.sync_cross_scheme(from, to, progress).await? {
            return Ok(());
        }
    }
    if let Some(provider) = registry.for_path(to) {
        if provider.sync_cross_scheme(from, to, progress).await? {
            return Ok(());
        }
    }
    Err(FsError::InvalidPath {
        reason: "rclone sync needs a network mount on at least one side".into(),
    })
}

fn join_rel(root_tail: &str, relative: &str) -> String {
    let root = root_tail.trim_start_matches('/');
    let rel = relative.trim_start_matches('/');
    if rel.is_empty() {
        root.to_string()
    } else if root.is_empty() {
        rel.to_string()
    } else {
        format!("{}/{rel}", root.trim_end_matches('/'))
    }
}

fn format_inline_remote(backend: &str, params: &[String], subpath: &str) -> String {
    let param_str = params.join(",");
    if subpath.is_empty() {
        format!(":{backend},{param_str}:")
    } else {
        format!(":{backend},{param_str}:{subpath}")
    }
}

fn inline_backend_params(
    scheme: &str,
    host_part: &str,
    mount: &orchid_storage::NetworkMountConfig,
) -> Result<Vec<String>> {
    let mut params = Vec::new();
    match scheme {
        "s3" => {
            let user = mount.user.as_deref().filter(|u| !u.is_empty());
            let pass = mount.password.as_deref().filter(|p| !p.is_empty());
            match (user, pass) {
                (Some(access), Some(secret)) => {
                    let plaintext = resolve_mount_secret(mount, secret)?;
                    params.push(format!("access_key_id={access}"));
                    params.push(format!("secret_access_key={plaintext}"));
                }
                _ => params.push("env_auth=true".into()),
            }
        }
        "ftps" => {
            params.push(format!("host={host_part}"));
            params.push("explicit_tls=true".into());
            push_user_pass(&mut params, mount)?;
        }
        _ => {
            params.push(format!("host={host_part}"));
            push_user_pass(&mut params, mount)?;
        }
    }
    Ok(params)
}

fn push_user_pass(
    params: &mut Vec<String>,
    mount: &orchid_storage::NetworkMountConfig,
) -> Result<()> {
    if let Some(user) = mount.user.as_deref().filter(|u| !u.is_empty()) {
        params.push(format!("user={user}"));
    }
    if let Some(pass) = mount.password.as_deref().filter(|p| !p.is_empty()) {
        let plaintext = resolve_mount_secret(mount, pass)?;
        params.push(format!("pass={plaintext}"));
    }
    Ok(())
}

fn resolve_mount_secret(mount: &orchid_storage::NetworkMountConfig, stored: &str) -> Result<String> {
    let plaintext =
        orchid_crypto::resolve_stored_secret(stored).map_err(|e| FsError::InvalidPath {
            reason: format!("mount `{}` password decrypt failed: {e}", mount.name),
        })?;
    if !orchid_crypto::is_protected(stored) && mount.rclone_remote.is_none() {
        tracing::warn!(
            mount = %mount.name,
            "network mount uses plaintext inline password; prefer rclone-remote \
             or let Orchid DPAPI-protect it on next config save \
             (password is still visible in rclone argv)"
        );
    }
    Ok(plaintext)
}

fn rclone_transfer_args(
    src: String,
    dst: String,
    is_dir: bool,
    is_move: bool,
    options: CopyOptions,
) -> Vec<String> {
    let mut args = if is_dir {
        vec![
            if is_move { "move" } else { "copy" }.into(),
            src,
            dst,
            "--create-empty-src-dirs".into(),
        ]
    } else {
        vec![
            if is_move { "moveto" } else { "copyto" }.into(),
            src,
            dst,
        ]
    };
    if options.resume {
        args.extend([
            "--retries".into(),
            "8".into(),
            "--low-level-retries".into(),
            "20".into(),
        ]);
    }
    args
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

/// Strip inline credentials from rclone stderr / connection strings before they
/// reach logs or UI error strings.
fn redact_secrets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Match `pass=` case-insensitively and redact until `,` / `:` / whitespace / end.
        let rest = &text[i..];
        let lower = rest.to_ascii_lowercase();
        if let Some((rel, key)) = ["secret_access_key=", "password=", "pass="]
            .into_iter()
            .filter_map(|key| lower.find(key).map(|rel| (rel, key)))
            .min_by_key(|(rel, _)| *rel)
        {
            out.push_str(&text[i..i + rel]);
            out.push_str(key);
            out.push_str("***");
            i += rel + key.len();
            while i < bytes.len() {
                let c = bytes[i] as char;
                if c == ',' || c == ':' || c.is_whitespace() {
                    break;
                }
                i += 1;
            }
            continue;
        }
        out.push_str(rest);
        break;
    }
    out
}

#[cfg(test)]
mod redact_tests {
    use super::redact_secrets;

    #[test]
    fn redacts_pass_in_connection_string() {
        let s = redact_secrets("rclone failed: :sftp,host=h,pass=s3cret:/path");
        assert!(!s.contains("s3cret"));
        assert!(s.contains("pass=***"));
    }

    #[test]
    fn leaves_unrelated_text() {
        assert_eq!(redact_secrets("permission denied"), "permission denied");
    }
}

/// Register one [`RcloneProvider`] per supported scheme.
pub fn register_rclone_providers(
    registry: &crate::provider::FsProviderRegistry,
    mounts: Arc<RwLock<Vec<orchid_storage::NetworkMountConfig>>>,
) -> Result<()> {
    for scheme in RCLONE_SCHEMES {
        registry.register(
            Arc::new(RcloneProvider::new(scheme, mounts.clone())) as Arc<dyn FsProvider>
        )?;
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
    FsPath::new(trimmed).ok().map(|p| p.as_str().to_string())
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
    fn backend_aliases() {
        assert_eq!(rclone_backend("ftps"), "ftp");
        assert_eq!(rclone_backend("scp"), "sftp");
        assert_eq!(rclone_backend("s3"), "s3");
        assert!(is_rclone_scheme("s3"));
        assert!(is_rclone_scheme("ftps"));
        assert!(!is_rclone_scheme("local"));
    }

    #[test]
    fn join_rel_composes() {
        assert_eq!(join_rel("home", "alice"), "home/alice");
        assert_eq!(join_rel("", "alice"), "alice");
        assert_eq!(join_rel("home", ""), "home");
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
