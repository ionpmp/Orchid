//! Persistent `rclone rcd` and a tiny localhost HTTP client for RC calls.
//!
//! List / stat go through one warm daemon so FM navigation does not spawn a
//! new rclone process (and re-parse config) on every folder. If `rcd` is
//! missing or fails to start, callers fall back to the CLI.

use std::sync::OnceLock;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::error::{FsError, Result};

/// `operations/list` response envelope, deserialized straight into the
/// caller's row type.
#[derive(Deserialize)]
struct ListEnvelope<T> {
    #[serde(default = "Vec::new")]
    list: Vec<T>,
}

/// `operations/stat` response envelope.
#[derive(Deserialize)]
struct StatEnvelope<T> {
    #[serde(default = "Option::default")]
    item: Option<T>,
}

/// Error body returned by `rcd` on a non-2xx status.
#[derive(Deserialize)]
struct ErrorEnvelope {
    #[serde(default = "Option::default")]
    error: Option<String>,
}

struct RcDaemon {
    port: u16,
    child: Child,
}

static DAEMON: OnceLock<Mutex<Option<RcDaemon>>> = OnceLock::new();

/// `operations/list` via RC. `None` means the daemon is down — use the CLI.
///
/// Rows are deserialized directly out of the socket buffer into `T`, so the
/// listing is materialized exactly once.
pub async fn list<T: DeserializeOwned>(rclone_bin: &str, remote: &str) -> Option<Result<Vec<T>>> {
    let port = ensure_daemon(rclone_bin).await.ok()?;
    match rc_post::<ListEnvelope<T>>(
        port,
        "operations/list",
        &json!({ "fs": remote, "remote": "" }),
    )
    .await
    {
        Ok(envelope) => Some(Ok(envelope.list)),
        Err(FsError::Io(_)) => None,
        Err(e) => Some(Err(e)),
    }
}

/// `operations/stat` via RC. `None` (outer) means use the CLI.
/// Inner `None` means the path does not exist.
pub async fn stat<T: DeserializeOwned>(
    rclone_bin: &str,
    remote: &str,
) -> Option<Result<Option<T>>> {
    let port = ensure_daemon(rclone_bin).await.ok()?;
    match rc_post::<StatEnvelope<T>>(
        port,
        "operations/stat",
        &json!({ "fs": remote, "remote": "" }),
    )
    .await
    {
        Ok(envelope) => Some(Ok(envelope.item)),
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
        if rc_post::<serde::de::IgnoredAny>(port, "rc/noop", &json!({}))
            .await
            .is_ok()
        {
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

async fn rc_post<T: DeserializeOwned>(port: u16, path: &str, body: &Value) -> Result<T> {
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
    let (status, body) = split_http_response(&raw)?;
    if !(200..300).contains(&status) {
        let err = serde_json::from_slice::<ErrorEnvelope>(body)
            .ok()
            .and_then(|e| e.error)
            .unwrap_or_else(|| "rc request failed".to_string());
        return Err(FsError::InvalidPath {
            reason: format!("rclone rc {path}: {err}"),
        });
    }
    // An empty body is still valid for calls like `rc/noop`; feed serde an
    // empty object so `T` gets a chance to default.
    let body = if body.iter().all(u8::is_ascii_whitespace) {
        b"{}".as_slice()
    } else {
        body
    };
    serde_json::from_slice(body).map_err(|e| FsError::InvalidPath {
        reason: format!("rclone rc json: {e}"),
    })
}

/// Split a raw HTTP/1.1 response into its status code and body bytes.
///
/// Only the status line is decoded as text; the body is left as borrowed
/// bytes so the JSON payload is parsed once, in place.
fn split_http_response(raw: &[u8]) -> Result<(u16, &[u8])> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| FsError::InvalidPath {
            reason: "rclone rc response missing header terminator".into(),
        })?;
    let (head, body) = raw.split_at(split);
    let status = std::str::from_utf8(head)
        .ok()
        .and_then(|head| head.lines().next())
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    Ok((status, &body[4..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_http_response_reads_status_and_body() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"list\":[]}";
        let (status, body) = split_http_response(raw).unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, b"{\"list\":[]}");
    }

    #[test]
    fn split_http_response_empty_body() {
        let raw = b"HTTP/1.1 200 OK\r\n\r\n";
        let (status, body) = split_http_response(raw).unwrap();
        assert_eq!(status, 200);
        assert!(body.is_empty());
    }

    #[test]
    fn list_envelope_deserializes_rows_and_defaults() {
        let env: ListEnvelope<Value> =
            serde_json::from_slice(br#"{"list":[{"Name":"a"}]}"#).unwrap();
        assert_eq!(env.list.len(), 1);
        let empty: ListEnvelope<Value> = serde_json::from_slice(br#"{}"#).unwrap();
        assert!(empty.list.is_empty());
    }

    #[test]
    fn stat_envelope_reports_missing_item() {
        let env: StatEnvelope<Value> = serde_json::from_slice(br#"{"item":null}"#).unwrap();
        assert!(env.item.is_none());
        let env: StatEnvelope<Value> = serde_json::from_slice(br#"{}"#).unwrap();
        assert!(env.item.is_none());
    }
}
