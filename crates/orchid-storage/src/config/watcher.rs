//! Async, debounced configuration file watcher.
//!
//! [`ConfigWatcher`] watches the TOML config file for changes, reloads and
//! revalidates on each change, and broadcasts the resulting
//! [`OrchidConfig`] to every subscriber.
//!
//! Invalid or half-written files are skipped — the watcher keeps the last
//! good state and logs a warning via [`tracing`].

use std::path::PathBuf;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::config::loader::ConfigLoader;
use crate::config::schema::OrchidConfig;
use crate::error::Result;

/// Size of the broadcast channel's retained message queue.
///
/// Slow subscribers that fall more than this many updates behind will see a
/// [`broadcast::error::RecvError::Lagged`] error on their next `recv`.
const BROADCAST_BUFFER: usize = 16;

/// Debounce window applied to raw notify events before a reload attempt.
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(500);

/// Async file watcher that reloads a TOML config on change and broadcasts
/// the resulting [`OrchidConfig`].
///
/// Create one with [`ConfigWatcher::start`]; drop it or call
/// [`ConfigWatcher::stop`] to shut the background task down gracefully.
#[derive(Debug)]
pub struct ConfigWatcher {
    tx: broadcast::Sender<OrchidConfig>,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl ConfigWatcher {
    /// Begin watching `path`, spawning a background task on the current
    /// tokio runtime.
    ///
    /// The returned receiver yields an [`OrchidConfig`] every time the file
    /// changes and validates successfully.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Watcher`] if the underlying notify
    /// watcher cannot be created, and [`crate::StorageError::Io`] if the
    /// target directory cannot be watched.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orchid_storage::ConfigWatcher;
    /// # async fn run() -> orchid_storage::Result<()> {
    /// let (_watcher, mut rx) =
    ///     ConfigWatcher::start("config.toml".into()).await?;
    /// if let Ok(new_config) = rx.recv().await {
    ///     println!("reloaded: {:?}", new_config.appearance.theme);
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn start(
        path: PathBuf,
    ) -> Result<(Self, broadcast::Receiver<OrchidConfig>)> {
        let (tx, rx) = broadcast::channel(BROADCAST_BUFFER);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        // Bridge notify's sync callback into an async channel the background
        // task can `select!` over.
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<()>();

        let mut debouncer = new_debouncer(
            DEBOUNCE_WINDOW,
            None,
            move |res: notify_debouncer_full::DebounceEventResult| match res {
                Ok(events) => {
                    if !events.is_empty() {
                        let _ = event_tx.send(());
                    }
                }
                Err(errors) => {
                    for e in errors {
                        warn!(error = %e, "config watcher reported an error event");
                    }
                }
            },
        )?;

        // Watch the parent directory non-recursively: atomic saves rename a
        // sibling temp file over the target, so the target itself may be
        // recreated, which some backends miss if we watch the file directly.
        let watch_target = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        debouncer
            .watch(&watch_target, RecursiveMode::NonRecursive)?;

        let tx_for_task = tx.clone();
        let path_for_task = path.clone();
        let task = tokio::spawn(async move {
            // Keep the debouncer alive for the lifetime of the task.
            let _debouncer = debouncer;

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        debug!("config watcher shutting down");
                        break;
                    }
                    maybe = event_rx.recv() => {
                        match maybe {
                            Some(()) => handle_event(&path_for_task, &tx_for_task),
                            None => break, // channel closed -- debouncer dropped.
                        }
                    }
                }
            }
        });

        Ok((
            Self {
                tx,
                shutdown: Some(shutdown_tx),
                task: Some(task),
            },
            rx,
        ))
    }

    /// Subscribe an additional receiver to the config update stream.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn run() -> orchid_storage::Result<()> {
    /// use orchid_storage::ConfigWatcher;
    /// let (watcher, _rx) =
    ///     ConfigWatcher::start("config.toml".into()).await?;
    /// let _second_rx = watcher.subscribe();
    /// # Ok(()) }
    /// ```
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<OrchidConfig> {
        self.tx.subscribe()
    }

    /// Stop the background task, waiting for it to finish.
    ///
    /// # Errors
    ///
    /// Never errors in practice — the signature returns a [`Result`] so that
    /// future failure modes (e.g. draining a pending reload) can be surfaced
    /// without breaking callers.
    pub async fn stop(mut self) -> Result<()> {
        if let Some(sig) = self.shutdown.take() {
            let _ = sig.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
        Ok(())
    }
}

impl Drop for ConfigWatcher {
    fn drop(&mut self) {
        if let Some(sig) = self.shutdown.take() {
            let _ = sig.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Reload the config and broadcast the result. Logs and drops errors.
fn handle_event(path: &std::path::Path, tx: &broadcast::Sender<OrchidConfig>) {
    match ConfigLoader::reload(path) {
        Ok(cfg) => {
            debug!(?path, "reloaded configuration");
            // Best-effort broadcast. An empty receiver list is not an error.
            let _ = tx.send(cfg);
        }
        Err(e) => {
            warn!(error = %e, ?path, "failed to reload configuration -- keeping previous state");
        }
    }
}
