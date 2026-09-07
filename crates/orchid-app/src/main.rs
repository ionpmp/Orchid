//! Orchid desktop application entry point.

#![warn(clippy::all)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use anyhow::{Context, Result};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use orchid_storage::OrchidPaths;
use orchid_ui::{
    claim_instance, collect_cli_open_paths, forward_open_paths, InstanceClaim, OrchidApp,
};

#[cfg(windows)]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn init_tracing() -> Result<()> {
    // `orchid=debug` used to be the default, which kept per-keystroke `debug!`
    // formatting alive in the terminal input path. Opt in via `RUST_LOG`.
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,orchid=info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .try_init()
        .map_err(|e| anyhow::anyhow!("failed to initialise tracing subscriber: {e}"))?;

    Ok(())
}

fn main() -> Result<()> {
    std::env::set_var("SLINT_BACKEND", "winit-skia");
    init_tracing()?;

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Orchid starting");

    let open_paths = collect_cli_open_paths(std::env::args_os().skip(1));
    if !open_paths.is_empty() {
        tracing::info!(count = open_paths.len(), "opening paths from argv");
    }

    let mut primary = match claim_instance() {
        InstanceClaim::Primary(p) => p,
        InstanceClaim::Secondary => {
            match forward_open_paths(&open_paths) {
                Ok(()) => tracing::info!("forwarded open paths to the running Orchid instance"),
                Err(e) => tracing::warn!(error = %e, "failed to forward open paths to primary"),
            }
            return Ok(());
        }
    };

    let paths = OrchidPaths::resolve().context("failed to resolve Orchid paths")?;

    // Multi-thread runtime for async bootstrap + background indexing.
    // The Slint event loop itself runs on the main thread, outside this runtime.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(4)
        .build()
        .context("failed to build tokio runtime")?;

    let app = runtime
        .block_on(OrchidApp::bootstrap(paths))
        .context("bootstrap failed")?;

    // `slint::spawn_local` and widget async work need the runtime in scope.
    let _guard = runtime.enter();

    let ipc_rx = primary.take_open_receiver();
    app.run_main(open_paths, ipc_rx)
        .context("UI loop exited with error")?;

    // Keep the single-instance mutex until shutdown completes.
    drop(primary);

    if let Ok(h) = tokio::runtime::Handle::try_current() {
        h.block_on(app.flush_after_window());
    }

    tracing::info!("Orchid exiting cleanly");
    Ok(())
}
