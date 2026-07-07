//! Orchid desktop application entry point.

#![warn(clippy::all)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use anyhow::{Context, Result};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use orchid_storage::OrchidPaths;
use orchid_ui::OrchidApp;

fn init_tracing() -> Result<()> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,orchid=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .try_init()
        .map_err(|e| anyhow::anyhow!("failed to initialise tracing subscriber: {e}"))?;

    Ok(())
}

fn main() -> Result<()> {
    init_tracing()?;

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Orchid starting");

    let paths = OrchidPaths::resolve().context("failed to resolve Orchid paths")?;

    // Small multi-thread runtime for async bootstrap work. The Slint
    // event loop itself runs on the main thread, outside this runtime.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .context("failed to build tokio runtime")?;

    let app = runtime
        .block_on(OrchidApp::bootstrap(paths))
        .context("bootstrap failed")?;

    // `slint::spawn_local` and widget async work need the runtime in scope.
    let _guard = runtime.enter();

    app.run_main().context("UI loop exited with error")?;

    if let Ok(h) = tokio::runtime::Handle::try_current() {
        h.block_on(app.flush_after_window());
    }

    tracing::info!("Orchid exiting cleanly");
    Ok(())
}
