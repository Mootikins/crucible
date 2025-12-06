//! Crucible Desktop - Entry point
//!
//! Launches the GPUI-based desktop chat application.

use anyhow::Result;
use crucible_desktop::app::run_app;

fn main() -> Result<()> {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    tracing::info!("Starting Crucible Desktop");

    run_app()
}
