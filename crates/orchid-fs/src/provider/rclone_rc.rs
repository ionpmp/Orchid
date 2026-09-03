//! Persistent `rclone rcd` and a tiny localhost HTTP client for RC calls.
//!
//! List / stat go through one warm daemon so FM navigation does not spawn a
//! new rclone process (and re-parse config) on every folder. If `rcd` is
//! missing or fails to start, callers fall back to the CLI.

use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::error::{FsError, Result};

struct RcDaemon {
    port: u16,
    child: Child,
}

static DAEMON: OnceLock<Mutex<Option<RcDaemon>>> = OnceLock::new();

/// `operations/list` via RC. `None` means the daemon is down — use the CLI.
pub async fn list_json(rclone_bin: &str, remote: &str) -> Option<Result<Vec<u8>>> {
    let port = ensure_daemon(rclone_bin).await.ok()?;
    match rc_post(
        port,
        "operations/list",
        &json!({ "fs": remote, "remote": "" }),
    )
    .await
    {
        Ok(body) => {
            let list = body
                .get("list")
                .cloned()
                .unwrap_or(Value::Array(Vec::new()));
            Some(serde_json::to_vec(&list).map_err(|e| FsError::InvalidPath {
                reason: format!("rclone rc list encode: {e}"),
            }))
        }
        Err(FsError::Io(_)) => None,
        Err(e) => Some(Err(e)),
    }
}

/// `operations/stat` via RC. `None` (outer) means use the CLI.
/// Inner `None` means the path does not exist.
pub async fn stat_json(rclone_bin: &str, remote: &str) -> Option<Result<Option<Vec<u8>>>> {
    let port = ensure_daemon(rclone_bin).await.ok()?;
    match rc_post(
        port,
        "operations/stat",
        &json!({ "fs": remote, "remote": "" }),
    )
    .await
    {
        Ok(body) => {
            let item = body.get("item").cloned().unwrap_or(Value::Null);
            if item.is_null() || item.as_object().is_some_and(serde_json::Map::is_empty) {
                return Some(Ok(None));
            }
            Some(
                serde_json::to_vec(&item)
                    .map(Some)
                    .map_err(|e| FsError::InvalidPath {
                        reason: format!("rclone rc stat encode: {e}"),
                    }),
            )
        }
        Err(FsError::Io(_)) => None,
        Err(e) => Some(Err(e)),
    }
}

async fn ensure_daemon(rclone_bin: &str) -> Result<u16> {
    let slot = DAEMON.get_or_init(|| Mutex::new(None));
    let mut guard = slot.lock().await;
    if let Some(daemon) = guard.as_mut() {
        match daemon.child.try_wait() {
            Ok(None) => return Ok(daemon.port),
            Ok(Some(_)) | Err(_) => {
                *guard = None;
            }
        }
    }
    let daemon = spawn_rcd(rclone_bin).await?;
    let port = daemon.port;
    *guard = Some(daemon);
    Ok(port)
}

async fn spawn_rcd(rclone_bin: &str) -> Result<RcDaemon> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(FsError::Io)?;
    let port = listener.local_addr().map_err(FsError::Io)?.port();
    drop(listener);

    let mut child = Command::new(rclone_bin)
        .args([
            "rcd",
            &format!("--rc-addr=127.0.0.1:{port}"),
            "--rc-no-auth",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(false)
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FsError::InvalidPath {
                    reason: format!("`{rclone_bin}` not found"),
                }
            } else {
                FsError::Io(e)
            }
        })?;

    for _ in 0..40 {
        if rc_post(port, "rc/noop", &json!({})).await.is_ok() {
            return Ok(RcDaemon { port, child });
        }
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = child.kill().await;
    Err(FsError::InvalidPath {
        reason: "rclone rcd did not become ready".into(),
    })
}

async fn rc_post(port: u16, path: &str, body: &Value) -> Result<Value> {
    let payload = serde_json::to_vec(body).map_err(|e| FsError::InvalidPath {
        reason: format!("rclone rc body: {e}"),
    })?;
    let header = format!(
        "POST /{path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(FsError::Io)?;
    stream
        .write_all(header.as_bytes())
        .await
        .map_err(FsError::Io)?;
    stream.write_all(&payload).await.map_err(FsError::Io)?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.map_err(FsError::Io)?;
    let (status, json_body) = parse_http_json(&raw)?;
    if !(200..300).contains(&status) {
        let err = json_body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("rc request failed");
        return Err(FsError::InvalidPath {
            reason: format!("rclone rc {path}: {err}"),
        });
    }
    Ok(json_body)
}

fn parse_http_json(raw: &[u8]) -> Result<(u16, Value)> {
    let text = std::str::from_utf8(raw).map_err(|e| FsError::InvalidPath {
        reason: format!("rclone rc response utf8: {e}"),
    })?;
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| FsError::InvalidPath {
            reason: "rclone rc response missing header terminator".into(),
        })?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let json_body = if body.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(body).map_err(|e| FsError::InvalidPath {
            reason: format!("rclone rc json: {e}"),
        })?
    };
    Ok((status, json_body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_http_json_reads_status_and_object() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"list\":[]}";
        let (status, value) = parse_http_json(raw).unwrap();
        assert_eq!(status, 200);
        assert_eq!(value["list"], json!([]));
    }

    #[test]
    fn parse_http_json_empty_body() {
        let raw = b"HTTP/1.1 200 OK\r\n\r\n";
        let (status, value) = parse_http_json(raw).unwrap();
        assert_eq!(status, 200);
        assert_eq!(value, json!({}));
    }
}
