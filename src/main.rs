//! trance — Windows Screensaver Manager.
//!
//! Standalone TUI for configuring any Windows screensaver.

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
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Shutdown::LockWorkStation;

use crate::config::{GlobalConfig, LocalConfig};

/// Screen saver management for Windows.
#[derive(Parser, Debug)]
#[command(
    name = "trance",
    version,
    about,
    long_about = None,
    after_help = "ENVIRONMENT VARIABLES:\n  RUST_LOG  Set log level (error, warn, info, debug, trace)\n  NO_COLOR  Disable TUI color rendering"
)]
struct Cli {
    /// Force TUI theme: dark, light, high-contrast, no-color
    #[arg(long, value_name = "THEME")]
    theme: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Launch the TUI dashboard (default).
    Tui,
    /// Launch the active screensaver fullscreen.
    #[command(alias = "start", alias = "s", alias = "S")]
    Run,
    /// Stop any running screensavers discovered on the system.
    Stop,
    /// Toggle the system screensaver active flag in the registry.
    #[command(name = "toggle-active")]
    ToggleActive,
    /// Lock the workstation, then launch the active screensaver.
    Lock,
    /// Windows `.scr` configure entry point (alias for `tui`).
    #[command(alias = "c", alias = "C")]
    Configure,
    /// Windows `.scr` preview entry point.
    #[command(alias = "p", alias = "P")]
    Preview {
        /// HWND handle of the window to render the preview in.
        hwnd: Option<String>,
    },
    /// Check system configuration and diagnostic reports.
    Doctor {
        /// Attempt to fix any discovered issues automatically.
        #[arg(long)]
        fix: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = init_tracing();
    let cli = Cli::parse_from(pre_munge_args(std::env::args().collect()));
    info!(?cli, "trance start");

    let command = cli.command.unwrap_or(Command::Tui);
    let result: Result<(), Box<dyn std::error::Error>> = match command {
        Command::Tui | Command::Configure => backend::run_tui(cli.theme.as_deref()),
        Command::Run | Command::Lock => {
            run_active_screensaver(matches!(command, Command::Lock)).map_err(Into::into)
        }
        Command::Stop => stop_all_screensavers(),
        Command::ToggleActive => toggle_active(),
        Command::Preview { hwnd } => run_active_screensaver_preview(hwnd).map_err(Into::into),
        Command::Doctor { fix } => doctor::run_doctor(fix),
    };

    if let Err(ref e) = result {
        error!(error = %e, "trance failed");
    }
    result
}

/// Translate Windows `.scr` calling-convention flags (`/s`, `/c`, `/p`) into
/// clap subcommand names so `trance.exe /s` works the same as `trance.exe run`.
fn pre_munge_args(args: Vec<String>) -> Vec<String> {
    let mut args = args;
    args.retain(|arg| arg != "--relaunched");
    if args.len() < 2 {
        return args;
    }
    // Handle Windows Screen Saver Preview formatting "/p:HWND"
    if args[1].starts_with("/p:") || args[1].starts_with("/P:") {
        let hwnd = args[1][3..].to_string();
        args[1] = "preview".to_string();
        args.insert(2, hwnd);
        return args;
    }
    if let Some(stripped) = args[1].strip_prefix('/') {
        let lowered = stripped.to_ascii_lowercase();
        let translated = match lowered.as_str() {
            "s" => "run",
            "c" => "configure",
            "p" => "preview",
            other => other,
        };
        args[1] = translated.to_string();
    }
    args
}

/// Initialize a file-based tracing subscriber so logs don't interfere with
/// the TUI.
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


fn run_active_screensaver(lock_first: bool) -> std::io::Result<()> {
    if lock_first {
        #[cfg(target_os = "windows")]
        unsafe { LockWorkStation() };
        #[cfg(not(target_os = "windows"))]
        println!("Locking workstation is not supported on this platform.");
    }
    let global = GlobalConfig::load();
    if global.active_scr.is_empty() {
        eprintln!("Error: no active screensaver configured.");
        std::process::exit(1);
    }
    let path = PathBuf::from(&global.active_scr);
    if !path.exists() {
        eprintln!(
            "Error: active screensaver path does not exist: {}",
            global.active_scr
        );
        std::process::exit(1);
    }
    let is_self = path == std::env::current_exe().unwrap_or_default();
    if is_self {
        app::run_random_cycle();
    } else {
        let mut child = std::process::Command::new(&path).arg("/s").spawn()?;
        let _ = child.wait();
    }
    Ok(())
}

fn run_active_screensaver_preview(hwnd: Option<String>) -> std::io::Result<()> {
    let global = GlobalConfig::load();
    if global.active_scr.is_empty() {
        return Ok(());
    }
    let path = PathBuf::from(&global.active_scr);
    if !path.exists() {
        return Ok(());
    }
    let is_self = path == std::env::current_exe().unwrap_or_default();
    if is_self {
        // Can't render ourselves recursively inside the preview window.
        return Ok(());
    }
    let mut cmd = std::process::Command::new(&path);
    cmd.arg("/p");
    if let Some(h) = hwnd {
        cmd.arg(h);
    }
    let mut child = cmd.spawn()?;
    let _ = child.wait();
    Ok(())
}

fn stop_all_screensavers() -> Result<(), Box<dyn std::error::Error>> {
    for s in backend::preview::discover() {
        if let Some(filename) = s.path.file_name().and_then(|f| f.to_str()) {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/IM", filename])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
    println!("Stopped all running screensavers.");
    Ok(())
}

fn toggle_active() -> Result<(), Box<dyn std::error::Error>> {
    let mut global = GlobalConfig::load();
    global.active = !global.active;
    if let Err(e) = global.save() {
        eprintln!("Error toggling screensaver: {e}");
        std::process::exit(1);
    }
    println!("Screensaver active state set to: {}", global.active);
    Ok(())
}
