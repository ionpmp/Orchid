//! Persistent `rclone rcd` and a tiny localhost HTTP client for RC calls.
//!
//! List / stat go through one warm daemon so FM navigation does not spawn a
//! new rclone process (and re-parse config) on every folder. If `rcd` is
//! missing or fails to start, callers fall back to the CLI.
//!
//! The HTTP client keeps a single `TcpStream` against `127.0.0.1` and reuses
//! it across calls (`Connection: keep-alive`), so folder navigation does not
//! pay a fresh TCP handshake per list/stat.

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
    /// Persistent HTTP/1.1 connection; cleared on I/O errors so the next
    /// call opens a fresh one.
    stream: Option<TcpStream>,
}

static DAEMON: OnceLock<Mutex<Option<RcDaemon>>> = OnceLock::new();

/// `operations/list` via RC. `None` means the daemon is down — use the CLI.
///
/// Rows are deserialized directly out of the socket buffer into `T`, so the
/// listing is materialized exactly once.
pub async fn list<T: DeserializeOwned>(rclone_bin: &str, remote: &str) -> Option<Result<Vec<T>>> {
    match rc_call::<ListEnvelope<T>>(
        rclone_bin,
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
    match rc_call::<StatEnvelope<T>>(
        rclone_bin,
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

async fn rc_call<T: DeserializeOwned>(rclone_bin: &str, path: &str, body: &Value) -> Result<T> {
    let slot = DAEMON.get_or_init(|| Mutex::new(None));
    let mut guard = slot.lock().await;
    ensure_daemon_locked(&mut guard, rclone_bin).await?;
    let daemon = guard.as_mut().expect("daemon just ensured");

    match rc_post_on(daemon, path, body).await {
        Ok(v) => Ok(v),
        Err(FsError::Io(_)) => {
            // Drop the broken keep-alive socket and retry once on a fresh
            // connection before declaring the daemon unusable.
            daemon.stream = None;
            rc_post_on(daemon, path, body).await
        }
        Err(e) => Err(e),
    }
}

async fn ensure_daemon_locked(guard: &mut Option<RcDaemon>, rclone_bin: &str) -> Result<()> {
    if let Some(daemon) = guard.as_mut() {
        match daemon.child.try_wait() {
            Ok(None) => return Ok(()),
            Ok(Some(_)) | Err(_) => {
                *guard = None;
            }
        }
    }
    *guard = Some(spawn_rcd(rclone_bin).await?);
    Ok(())
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

    let mut daemon = RcDaemon {
        port,
        child,
        stream: None,
    };
    for _ in 0..40 {
        if rc_post_on::<serde::de::IgnoredAny>(&mut daemon, "rc/noop", &json!({}))
            .await
            .is_ok()
        {
            return Ok(daemon);
        }
        if matches!(daemon.child.try_wait(), Ok(Some(_))) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = daemon.child.kill().await;
    Err(FsError::InvalidPath {
        reason: "rclone rcd did not become ready".into(),
    })
}

async fn rc_post_on<T: DeserializeOwned>(
    daemon: &mut RcDaemon,
    path: &str,
    body: &Value,
) -> Result<T> {
    let payload = serde_json::to_vec(body).map_err(|e| FsError::InvalidPath {
        reason: format!("rclone rc body: {e}"),
    })?;
    let header = format!(
        "POST /{path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
        daemon.port,
        payload.len()
    );

    if daemon.stream.is_none() {
        let stream = TcpStream::connect(("127.0.0.1", daemon.port))
            .await
            .map_err(FsError::Io)?;
        daemon.stream = Some(stream);
    }
    let stream = daemon.stream.as_mut().expect("stream just ensured");

    stream
        .write_all(header.as_bytes())
        .await
        .map_err(FsError::Io)?;
    stream.write_all(&payload).await.map_err(FsError::Io)?;

    let (status, body) = read_http_response(stream).await?;
    if !(200..300).contains(&status) {
        let err = serde_json::from_slice::<ErrorEnvelope>(&body)
            .ok()
            .and_then(|e| e.error)
            .unwrap_or_else(|| "rc request failed".to_string());
        return Err(FsError::InvalidPath {
            reason: format!("rclone rc {path}: {err}"),
        });
    }
    // An empty body is still valid for calls like `rc/noop`.
    let body = if body.iter().all(u8::is_ascii_whitespace) {
        b"{}".to_vec()
    } else {
        body
    };
    serde_json::from_slice(&body).map_err(|e| FsError::InvalidPath {
        reason: format!("rclone rc json: {e}"),
    })
}

/// Read one HTTP/1.1 response from a keep-alive stream.
///
/// Uses `Content-Length` so we stop at the end of this response and leave
/// the connection open for the next call. Falls back to reading until EOF
/// when the length is missing (then the stream is spent).
async fn read_http_response(stream: &mut TcpStream) -> Result<(u16, Vec<u8>)> {
    let mut buf = Vec::with_capacity(4096);
    loop {
        if let Some(split) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let status = status_from_head(&buf[..split]);
            let headers = &buf[..split];
            let content_length = content_length_from_head(headers);
            let mut body = buf.split_off(split + 4);
            match content_length {
                Some(len) => {
                    while body.len() < len {
                        let mut chunk = [0u8; 8192];
                        let n = stream.read(&mut chunk).await.map_err(FsError::Io)?;
                        if n == 0 {
                            break;
                        }
                        body.extend_from_slice(&chunk[..n]);
                    }
                    if body.len() > len {
                        // Pipelined leftover — should not happen with our
                        // request/response cadence; truncate to the response.
                        body.truncate(len);
                    } else if body.len() < len {
                        return Err(FsError::Io(std::io::Error::new(
                            std::io::ErrorKind::UnexpectedEof,
                            "rclone rc short body",
                        )));
                    }
                    return Ok((status, body));
                }
                None => {
                    // No length: drain until the peer closes. The keep-alive
                    // socket is spent after this.
                    loop {
                        let mut chunk = [0u8; 8192];
                        let n = stream.read(&mut chunk).await.map_err(FsError::Io)?;
                        if n == 0 {
                            break;
                        }
                        body.extend_from_slice(&chunk[..n]);
                    }
                    return Ok((status, body));
                }
            }
        }
        let mut chunk = [0u8; 1024];
        let n = stream.read(&mut chunk).await.map_err(FsError::Io)?;
        if n == 0 {
            return Err(FsError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "rclone rc response missing header terminator",
            )));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

fn status_from_head(head: &[u8]) -> u16 {
    std::str::from_utf8(head)
        .ok()
        .and_then(|head| head.lines().next())
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

fn content_length_from_head(head: &[u8]) -> Option<usize> {
    let text = std::str::from_utf8(head).ok()?;
    for line in text.lines().skip(1) {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            return value.trim().parse().ok();
        }
    }
    None
}

/// Split a raw HTTP/1.1 response into its status code and body bytes.
///
/// Kept for unit tests that feed a complete buffer rather than a stream.
fn split_http_response(raw: &[u8]) -> Result<(u16, &[u8])> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| FsError::InvalidPath {
            reason: "rclone rc response missing header terminator".into(),
        })?;
    let (head, body) = raw.split_at(split);
    let status = status_from_head(head);
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
    fn content_length_is_parsed_case_insensitively() {
        let head = b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\n";
        assert_eq!(content_length_from_head(head), Some(12));
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
