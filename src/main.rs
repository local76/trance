//! WSM — Windows Screensaver Manager.
//!
//! Standalone TUI for configuring any Windows screensaver.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

mod app;
mod config;
mod preview;
mod theme;
mod ui;
mod win32;

#[cfg(feature = "downloader")]
pub mod downloader;

use std::io::{Write, stdout};
use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::terminal::LeaveAlternateScreen;
use tracing::{error, info};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use windows_sys::Win32::System::Shutdown::LockWorkStation;

use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

use crate::app::{App, KeyCode, KeyModifiers};
use crate::config::{GlobalConfig, LocalConfig};
use crate::theme::TuiTheme;
use crate::win32::BorderlessConsole;

/// Screen saver management for Windows.
#[derive(Parser, Debug)]
#[command(
    name = "wsm",
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
    install_panic_hook();
    let cli = Cli::parse_from(pre_munge_args(std::env::args().collect()));
    info!(?cli, "wsm start");

    let command = cli.command.unwrap_or(Command::Tui);
    let result: Result<(), Box<dyn std::error::Error>> = match command {
        Command::Tui | Command::Configure => run_tui(cli.theme.as_deref()),
        Command::Run | Command::Lock => {
            run_active_screensaver(matches!(command, Command::Lock)).map_err(Into::into)
        }
        Command::Stop => stop_all_screensavers(),
        Command::ToggleActive => toggle_active(),
        Command::Preview { hwnd } => run_active_screensaver_preview(hwnd).map_err(Into::into),
        Command::Doctor { fix } => run_doctor(fix),
    };

    if let Err(ref e) = result {
        error!(error = %e, "wsm failed");
    }
    result
}

/// Translate Windows `.scr` calling-convention flags (`/s`, `/c`, `/p`) into
/// clap subcommand names so `wsm.exe /s` works the same as `wsm.exe run`.
fn pre_munge_args(args: Vec<String>) -> Vec<String> {
    let mut args = args;
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
        .and_then(|p| p.parent().map(|p| p.join("wsm.log")))
        .unwrap_or_else(|| PathBuf::from("wsm.log"));
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

/// Install a panic hook that restores the terminal before delegating to the
/// default handler.  Without this, a panic inside `run_tui` would leave the
/// user stuck in raw mode.
fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("unknown panic");
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_default();
        error!("Panic occurred at {}: {}", loc, msg);

        let _ = ratatui::crossterm::terminal::disable_raw_mode();
        let mut out = stdout();
        let _ = ratatui::crossterm::execute!(
            out,
            LeaveAlternateScreen,
            ratatui::crossterm::cursor::Show
        );
        let _ = out.flush();
        original(info);
    }));
}

fn run_tui(theme_override: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    use ratatui::crossterm::tty::IsTty;
    if !std::io::stdin().is_tty() {
        return Err("Interactive TUI requires a TTY stdin.".into());
    }

    let _instance_guard = match win32::SingleInstanceGuard::try_new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let _title_guard = ConsoleTitleGuard::new("WSM — Windows Screensavers Manager");

    let screensavers = preview::discover();

    let global = GlobalConfig::load();
    let local = LocalConfig::load();
    let theme = TuiTheme::detect(theme_override);
    log_environment(&theme);

    let mut app = App::new(screensavers, global, local, theme);

    ratatui::crossterm::terminal::enable_raw_mode()?;
    let mut out = stdout();
    ratatui::crossterm::execute!(
        out,
        ratatui::crossterm::terminal::EnterAlternateScreen,
        ratatui::crossterm::cursor::Hide,
        ratatui::crossterm::event::DisableMouseCapture
    )?;

    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;
    let _borderless = BorderlessConsole::enable();

    let mut status_ttl: u32 = 0;
    let mut last_sleep_prevented = false;
    let mut sync_check_timer: u32 = 0;

    loop {
        // Apply the sleep-inhibition state to the OS.  We only call into
        // Win32 when the desired state changes, so a stationary event loop
        // does no work.
        if app.local.prevent_sleep != last_sleep_prevented {
            win32::set_thread_execution_state(app.local.prevent_sleep);
            last_sleep_prevented = app.local.prevent_sleep;
        }

        #[cfg(feature = "downloader")]
        {
            let mut got_entries = None;
            if let Some(ref registry_mutex) = app.registry_results {
                if let Ok(mut lock) = registry_mutex.try_lock() {
                    if let Some(entries) = lock.take() {
                        got_entries = Some(entries);
                    }
                }
            }
            if let Some(entries) = got_entries {
                app.registry_results = None; // Stop polling
                app.merge_registry_entries(entries);
            }

            let mut reset_state = false;
            let mut download_success = false;
            let mut err_msg = None;
            let mut downloaded_name = String::new();

            if let Some(ref state_mutex) = app.download_state {
                if let Ok(state) = state_mutex.lock() {
                    downloaded_name = state.name.clone();
                    match state.status {
                        crate::downloader::DownloadStatus::Success => {
                            if app.visual_progress >= 1.0 {
                                reset_state = true;
                                download_success = true;
                            }
                        }
                        crate::downloader::DownloadStatus::Error(ref err) => {
                            reset_state = true;
                            err_msg = Some(err.clone());
                        }
                        crate::downloader::DownloadStatus::Downloading => {}
                    }
                }
            }

            if reset_state {
                app.download_state = None;
                if download_success {
                    app.status = Some(crate::app::StatusMessage {
                        text: "Download completed successfully!".to_string(),
                        kind: crate::app::StatusKind::Info,
                    });
                    app.notice = Some(format!("Downloaded screensaver: {}", downloaded_name));
                    app.refresh_screensavers();
                    if let Some(action) = app.pending_action.take() {
                        match action {
                            crate::app::PendingAction::Apply => app.apply_highlighted(),
                            crate::app::PendingAction::ToggleSelection => app.toggle_highlighted_selection(),
                            crate::app::PendingAction::Preview => app.preview_highlighted(),
                            crate::app::PendingAction::Configure => app.configure_highlighted(),
                        }
                    }
                } else if let Some(msg) = err_msg {
                    app.pending_action = None;
                    app.status = Some(crate::app::StatusMessage {
                        text: format!("Download failed: {}", msg),
                        kind: crate::app::StatusKind::Error,
                    });
                }
            }
        }

        #[cfg(feature = "downloader")]
        {
            app.update_download_progress();
        }

        let term_size = terminal.size().unwrap_or_default();
        app.update_particles(term_size.width, term_size.height);

        terminal.draw(|f| ui::render(&mut app, f))?;

        let is_animating = (app.vanity_enabled && !app.particles.is_empty())
            || {
                #[cfg(feature = "downloader")]
                {
                    app.download_state.is_some()
                }
                #[cfg(not(feature = "downloader"))]
                {
                    false
                }
            };

        let poll = if is_animating {
            Duration::from_millis(30)
        } else {
            Duration::from_millis(250)
        };

        let start_time = std::time::Instant::now();
        let has_event = event::poll(poll)?;
        let elapsed_ms = start_time.elapsed().as_millis() as u32;
        let tick_ms = if has_event { elapsed_ms.max(1) } else { poll.as_millis() as u32 };

        if has_event {
            let ev = event::read()?;
            tracing::info!(?ev, "Received event");
            match ev {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        let code: KeyCode = key.code;
                        let mods: KeyModifiers = key.modifiers;
                        tracing::info!(?code, ?mods, "Key press event");
                        if app.handle_key(code, mods) {
                            tracing::info!("app.handle_key returned true, breaking loop");
                            break;
                        }
                        status_ttl = 7500;
                    } else {
                        tracing::info!(?key.kind, ?key.code, "Ignored non-press key event");
                    }
                }
                Event::Resize(w, h) => {
                    tracing::info!(w, h, "Terminal resize event");
                }
                _ => {
                    tracing::info!("Other event ignored");
                }
            }
        }

        if status_ttl > 0 {
            status_ttl = status_ttl.saturating_sub(tick_ms);
            if status_ttl == 0 {
                if let Some(ref msg) = app.status {
                    if msg.kind == crate::app::StatusKind::Info {
                        app.status = None;
                    }
                }
            }
        }

        sync_check_timer = sync_check_timer.saturating_add(tick_ms);
        if sync_check_timer >= 2500 {
            sync_check_timer = 0;
            app.check_registry_sync();
        }
    }

    // Release any sleep-inhibition we may have set, then restore the
    // terminal and console window.
    win32::set_thread_execution_state(false);
    let _ = ratatui::crossterm::terminal::disable_raw_mode();
    ratatui::crossterm::execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        ratatui::crossterm::cursor::Show
    )?;
    Ok(())
}

fn log_environment(theme: &TuiTheme) {
    let metrics = win32::SystemMetrics::query();
    info!(
        screen = format!("{}x{}", metrics.screen_w, metrics.screen_h),
        dpi = metrics.dpi,
        window_dpi = metrics.window_dpi,
        dark_mode = metrics.dark_mode,
        high_contrast = metrics.high_contrast,
        no_color = theme.no_color,
        accent = ?metrics.accent,
        ac_online = metrics.power.ac_online,
        battery = metrics.power.battery_percent,
        "environment"
    );
}

fn run_active_screensaver(lock_first: bool) -> std::io::Result<()> {
    if lock_first {
        unsafe { LockWorkStation() };
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
    for s in preview::discover() {
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

fn run_doctor(fix: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("WSM Doctor — Diagnostic Report");
    println!("=============================");

    // 1. Check Registry Access
    print!("Registry Access:         ");
    let desktop = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Control Panel\\Desktop");
    match desktop {
        Ok(_) => println!("OK (Readable)"),
        Err(e) => println!("FAILED (Error: {})", e),
    }

    // 2. Check Active Screensaver
    print!("Active Screensaver Path: ");
    let mut global = GlobalConfig::load();
    if global.active_scr.is_empty() {
        println!("None Configured");
        if fix {
            let discovered = preview::discover();
            if !discovered.is_empty() {
                let first_path = discovered[0].path.to_string_lossy().into_owned();
                global.active_scr = first_path.clone();
                if global.save().is_ok() {
                    println!("    [FIXED] Set active screensaver to first discovered: {}", first_path);
                }
            }
        }
    } else {
        let path = std::path::PathBuf::from(&global.active_scr);
        if path.exists() {
            println!("OK ({})", global.active_scr);
        } else {
            println!("MISSING FILE ({})", global.active_scr);
            if fix {
                let discovered = preview::discover();
                let first_valid = discovered.iter().find(|s| s.path.exists());
                if let Some(s) = first_valid {
                    let new_path = s.path.to_string_lossy().into_owned();
                    global.active_scr = new_path.clone();
                    if global.save().is_ok() {
                        println!("    [FIXED] Reset active screensaver to valid path: {}", new_path);
                    }
                }
            }
        }
    }

    // 3. Discovery Directories
    println!("\nDiscovery Directories:");
    if let Ok(appdata) = std::env::var("APPDATA") {
        let wsm_dir = std::path::PathBuf::from(appdata)
            .join("wsm")
            .join("screensavers");
        let exists = wsm_dir.exists();
        println!(
            "  - %APPDATA%/wsm/screensavers: {}",
            if exists { "EXISTS" } else { "NOT FOUND" }
        );
        if !exists && fix {
            if std::fs::create_dir_all(&wsm_dir).is_ok() {
                println!("    [FIXED] Created directory: {:?}", wsm_dir);
            } else {
                println!("    [FAILED] Could not create directory: {:?}", wsm_dir);
            }
        }
    }
    if let Ok(sys_root) = std::env::var("SystemRoot") {
        let root_path = std::path::PathBuf::from(&sys_root);
        println!(
            "  - SystemRoot:                {}",
            if root_path.exists() {
                "EXISTS"
            } else {
                "NOT FOUND"
            }
        );
        let sys32 = root_path.join("System32");
        println!(
            "  - System32:                  {}",
            if sys32.exists() {
                "EXISTS"
            } else {
                "NOT FOUND"
            }
        );
        let syswow64 = std::path::PathBuf::from(sys_root).join("SysWOW64");
        println!(
            "  - SysWOW64:                  {}",
            if syswow64.exists() {
                "EXISTS"
            } else {
                "NOT FOUND"
            }
        );
    }

    // 4. Log File Check
    print!("\nLog File Writable:       ");
    let log_path = LocalConfig::config_path()
        .and_then(|p| p.parent().map(|p| p.join("wsm.log")))
        .unwrap_or_else(|| std::path::PathBuf::from("wsm.log"));
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(_) => println!("OK ({:?})", log_path),
        Err(e) => println!("FAILED (Error: {})", e),
    }

    // 5. Local Preferences Check
    println!("\nLocal Preferences Check:");
    let mut local = LocalConfig::load();
    println!("  - Prevent System Sleep:      {}", if local.prevent_sleep { "ENABLED (Active Awake)" } else { "DISABLED (Normal)" });
    println!("  - Random Cycle Duration:     {} seconds", local.random_cycle_secs);
    println!("  - Selected Cycle Screensavers ({}):", local.selected_paths.len());
    if local.selected_paths.is_empty() {
        println!("      (None selected; default cycle will cycle all discovered screensavers)");
    } else {
        let mut missing_count = 0;
        for path in &local.selected_paths {
            let p = std::path::Path::new(path);
            let exists = p.exists();
            if !exists {
                missing_count += 1;
            }
            let status = if exists { "OK" } else { "MISSING FILE" };
            let filename = p.file_name().and_then(|f| f.to_str()).unwrap_or(path);
            println!("      - {} [{}] ({})", filename, status, path);
        }
        if missing_count > 0 && fix {
            local.selected_paths.retain(|path| std::path::Path::new(path).exists());
            if local.save().is_ok() {
                println!("    [FIXED] Removed {} missing screensaver(s) from cycle selection.", missing_count);
            }
        }
    }

    // 6. Theme Detection Check
    print!("\nTheme Detection:         ");
    let theme = TuiTheme::detect(None);
    println!(
        "OK (High Contrast: {}, No Color: {})",
        theme.high_contrast, theme.no_color
    );

    println!("\nDiagnostics Complete.");
    Ok(())
}

struct ConsoleTitleGuard {
    original_title: Option<String>,
}

impl ConsoleTitleGuard {
    fn new(new_title: &str) -> Self {
        let original_title = crate::win32::get_console_title().ok();
        if original_title.is_some() {
            let _ = crate::win32::set_console_title(new_title);
        }
        ConsoleTitleGuard { original_title }
    }
}

impl Drop for ConsoleTitleGuard {
    fn drop(&mut self) {
        if let Some(ref title) = self.original_title {
            let _ = crate::win32::set_console_title(title);
        }
    }
}
