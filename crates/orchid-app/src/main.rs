//! Orchid desktop application entry point.
//!
//! Initialises tracing and will, in a later stage, construct the core runtime,
//! mount the Slint UI, and drive the main event loop. For now it only proves
//! the workspace wires together end-to-end.

#![warn(clippy::all)]

use anyhow::Result;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

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

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "Orchid starting"
    );

    // TODO(app): construct the Orchid runtime, mount the Slint UI, and enter
    // the main loop. This is intentionally left as a placeholder while the
    // supporting crates are stubbed out.

    Ok(())
}
