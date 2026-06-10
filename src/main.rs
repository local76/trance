//! trance — Windows Screensaver Manager.
//!
//! Standalone UI for configuring any Windows screensaver.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

mod app;
mod backend;
mod config;
mod doctor;
mod theme;
mod ui;
mod win32;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use tracing_appender::non_blocking::WorkerGuard;
use crate::config::LocalConfig;

/// Screen saver management for Windows.
#[derive(Parser, Debug)]
#[command(
    name = "trance",
    version,
    about,
    long_about = None,
    after_help = "ENVIRONMENT VARIABLES:\n  RUST_LOG  Set log level (error, warn, info, debug, trace)\n  NO_COLOR  Disable UI color rendering"
)]
struct Cli {
    /// Force UI theme: dark, light, high-contrast, no-color
    #[arg(long, value_name = "THEME")]
    theme: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand, Debug)]
enum Command {
    /// Launch the app dashboard (default).
    Ui,
    /// Check system configuration and diagnostic reports.
    Doctor {
        /// Attempt to fix any discovered issues automatically.
        #[arg(long)]
        fix: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = init_tracing();
    let cli = Cli::parse();
    info!(?cli, "trance start");

    let command = cli.command.unwrap_or(Command::Ui);
    let result: Result<(), Box<dyn std::error::Error>> = match command {
        Command::Ui => backend::run_ui(cli.theme.as_deref()),
        Command::Doctor { fix } => doctor::run_doctor(fix),
    };

    if let Err(ref e) = result {
        error!(error = %e, "trance failed");
    }
    result
}

/// Initialize a file-based tracing subscriber so logs don't interfere with
/// the UI.
fn init_tracing() -> WorkerGuard {
    let log_path = LocalConfig::config_path()
        .and_then(|p| p.parent().map(|p| p.join("trance.log")))
        .unwrap_or_else(|| PathBuf::from("trance.log"));
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok();
    let (writer, guard) = match file {
        Some(f) => tracing_appender::non_blocking(f),
        None => tracing_appender::non_blocking(std::io::sink()),
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init();
    guard
}
