//! Single-instance gate and argv / Explorer path forwarding.
//!
//! On Windows the first process holds a named mutex and listens on a named
//! pipe. Later launches send open paths to that pipe and exit. Other platforms
//! always run as primary (no forwarding).

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use tracing::info;

#[cfg(windows)]
mod win {
    use std::io::{Read, Write};
    use std::os::windows::io::{FromRawHandle, IntoRawHandle};
    use std::path::PathBuf;
    use std::sync::mpsc::SyncSender;
    use std::time::Duration;

    use serde::{Deserialize, Serialize};
    use tracing::{info, warn};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, INVALID_HANDLE_VALUE,
        WIN32_ERROR,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_READ,
        FILE_GENERIC_WRITE, FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows::Win32::System::Threading::CreateMutexW;

    pub(super) const MUTEX_NAME: &str = "Local\\Orchid.SingleInstance.v1";
    pub(super) const PIPE_NAME: &str = r"\\.\pipe\Orchid.Open.v1";
    /// Client already connected when `ConnectNamedPipe` returns.
    const ERROR_PIPE_CONNECTED: WIN32_ERROR = WIN32_ERROR(535);

    #[derive(Debug, Serialize, Deserialize)]
    struct OpenMessage {
        paths: Vec<String>,
    }

    pub(super) struct MutexGuard {
        handle: HANDLE,
    }

    // SAFETY: dropped only when the process exits; never shared across threads.
    unsafe impl Send for MutexGuard {}

    impl Drop for MutexGuard {
        fn drop(&mut self) {
            if !self.handle.is_invalid() {
                let _ = unsafe { CloseHandle(self.handle) };
            }
        }
    }

    /// Try to become the primary instance. `None` means another Orchid holds the mutex.
    pub(super) fn try_claim_primary() -> Option<MutexGuard> {
        let wide: Vec<u16> = MUTEX_NAME
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: null DACL; `wide` is NUL-terminated.
        let handle = unsafe { CreateMutexW(None, true, PCWSTR(wide.as_ptr())) }.ok()?;
        let err = unsafe { GetLastError() };
        if err == ERROR_ALREADY_EXISTS {
            let _ = unsafe { CloseHandle(handle) };
            return None;
        }
        Some(MutexGuard { handle })
    }

    pub(super) fn encode_message(paths: &[PathBuf]) -> Vec<u8> {
        let msg = OpenMessage {
            paths: paths
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        };
        let body = serde_json::to_vec(&msg).unwrap_or_else(|_| b"{\"paths\":[]}".to_vec());
        let mut out = Vec::with_capacity(4 + body.len());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    pub(super) fn decode_message(bytes: &[u8]) -> Option<Vec<PathBuf>> {
        let msg: OpenMessage = serde_json::from_slice(bytes).ok()?;
        Some(msg.paths.into_iter().map(PathBuf::from).collect())
    }

    fn read_frame(mut r: impl Read) -> Option<Vec<u8>> {
        let mut len_buf = [0u8; 4];
        r.read_exact(&mut len_buf).ok()?;
        let len = u32::from_le_bytes(len_buf) as usize;
        if len > 4 * 1024 * 1024 {
            return None;
        }
        let mut body = vec![0u8; len];
        r.read_exact(&mut body).ok()?;
        Some(body)
    }

    fn to_file(handle: HANDLE) -> std::fs::File {
        // SAFETY: caller transfers exclusive ownership of `handle`.
        unsafe { std::fs::File::from_raw_handle(handle.0) }
    }

    fn from_file(file: std::fs::File) -> HANDLE {
        HANDLE(file.into_raw_handle())
    }

    /// Connect to the primary pipe and send `paths` (may be empty).
    pub(super) fn forward_paths(paths: &[PathBuf]) -> Result<(), String> {
        let payload = encode_message(paths);
        let wide: Vec<u16> = PIPE_NAME
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let access = FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0;
        loop {
            // SAFETY: open the well-known Orchid open pipe.
            let handle = unsafe {
                CreateFileW(
                    PCWSTR(wide.as_ptr()),
                    access,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(FILE_ATTRIBUTE_NORMAL.0),
                    None,
                )
            };
            match handle {
                Ok(h) if h != INVALID_HANDLE_VALUE => {
                    let mut file = to_file(h);
                    file.write_all(&payload)
                        .map_err(|e| format!("write open pipe: {e}"))?;
                    let _ = file.flush();
                    return Ok(());
                }
                _ => {
                    if std::time::Instant::now() >= deadline {
                        return Err("primary Orchid open pipe not available".into());
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }

    pub(super) fn spawn_listener(tx: SyncSender<Vec<PathBuf>>) {
        std::thread::Builder::new()
            .name("orchid-open-ipc".into())
            .spawn(move || listener_loop(tx))
            .expect("spawn orchid-open-ipc");
    }

    fn listener_loop(tx: SyncSender<Vec<PathBuf>>) {
        let wide: Vec<u16> = PIPE_NAME
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        loop {
            // SAFETY: duplex byte pipe for open-path IPC.
            let pipe = unsafe {
                CreateNamedPipeW(
                    PCWSTR(wide.as_ptr()),
                    PIPE_ACCESS_DUPLEX,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                    PIPE_UNLIMITED_INSTANCES,
                    64 * 1024,
                    64 * 1024,
                    0,
                    None,
                )
            };
            if pipe.is_invalid() || pipe == INVALID_HANDLE_VALUE {
                warn!("CreateNamedPipeW failed; open IPC disabled");
                return;
            }
            // SAFETY: blocking wait for the next secondary instance.
            let connected = unsafe { ConnectNamedPipe(pipe, None) };
            if connected.is_err() {
                let err = unsafe { GetLastError() };
                if err != ERROR_PIPE_CONNECTED {
                    let _ = unsafe { CloseHandle(pipe) };
                    continue;
                }
            }
            let file = to_file(pipe);
            if let Some(body) = read_frame(&file) {
                if let Some(paths) = decode_message(&body) {
                    info!(count = paths.len(), "received open paths from secondary instance");
                    if tx.send(paths).is_err() {
                        let handle = from_file(file);
                        let _ = unsafe { CloseHandle(handle) };
                        return;
                    }
                }
            }
            let handle = from_file(file);
            let _ = unsafe { DisconnectNamedPipe(handle) };
            let _ = unsafe { CloseHandle(handle) };
        }
    }
}

/// Primary-instance token (holds the Windows mutex until drop).
pub struct PrimaryInstance {
    #[cfg(windows)]
    _mutex: win::MutexGuard,
    open_rx: Option<Receiver<Vec<PathBuf>>>,
}

/// Outcome of [`claim_instance`].
pub enum InstanceClaim {
    /// This process should run the UI.
    Primary(PrimaryInstance),
    /// Another Orchid is running; caller should forward paths and exit.
    Secondary,
}

/// Claim the single-instance lock. On non-Windows always returns [`InstanceClaim::Primary`].
#[must_use]
pub fn claim_instance() -> InstanceClaim {
    #[cfg(windows)]
    {
        match win::try_claim_primary() {
            Some(mutex) => {
                let (tx, rx) = mpsc::sync_channel(16);
                win::spawn_listener(tx);
                info!("single-instance: primary");
                InstanceClaim::Primary(PrimaryInstance {
                    _mutex: mutex,
                    open_rx: Some(rx),
                })
            }
            None => {
                info!("single-instance: secondary");
                InstanceClaim::Secondary
            }
        }
    }
    #[cfg(not(windows))]
    {
        let (_tx, rx) = mpsc::sync_channel(1);
        InstanceClaim::Primary(PrimaryInstance { open_rx: Some(rx) })
    }
}

/// Forward open paths to the primary instance (Windows). No-op elsewhere.
pub fn forward_open_paths(paths: &[PathBuf]) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::forward_paths(paths)
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
        Err("single-instance forwarding is Windows-only".into())
    }
}

impl PrimaryInstance {
    /// Take the receiver for secondary open requests (once).
    pub fn take_open_receiver(&mut self) -> Option<Receiver<Vec<PathBuf>>> {
        self.open_rx.take()
    }
}

#[cfg(all(test, windows))]
mod encode_tests {
    use super::win::{decode_message, encode_message};
    use std::path::PathBuf;

    #[test]
    fn empty_paths_roundtrip() {
        let enc = encode_message(&[]);
        let len = u32::from_le_bytes(enc[0..4].try_into().unwrap()) as usize;
        let decoded = decode_message(&enc[4..4 + len]).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn paths_roundtrip() {
        let paths = vec![PathBuf::from(r"C:\a.orchid")];
        let enc = encode_message(&paths);
        let len = u32::from_le_bytes(enc[0..4].try_into().unwrap()) as usize;
        assert_eq!(decode_message(&enc[4..4 + len]).unwrap(), paths);
    }
}
