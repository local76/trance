//! Main event loop and UI runner for trance.
//!
//! **Taxonomy Classification**: Interface (UI / Presentation Layer).


use std::time::Duration;
use tracing::info;

use library::apps::tui_bootstrap::{bootstrap_tui, shutdown_tui, TuiBootstrapConfig};
use crossterm::event::{self, Event, KeyEventKind};

pub mod preview;

#[cfg(target_os = "windows")]
#[path = "saver/win32.rs"]
pub mod saver_win32;

#[cfg(not(target_os = "windows"))]
#[path = "saver/stub.rs"]
pub mod saver_win32;



use crate::app::{App, KeyCode, KeyModifiers};
use crate::config::{GlobalConfig, LocalConfig};
use crate::theme::TuiTheme;
use crate::ui;
use crate::win32;



/// Run the screensaver manager interactive app.
pub fn run_ui(theme_override: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    use crossterm::tty::IsTty;
    if !std::io::stdin().is_tty() {
        return Err("Interactive app requires a TTY stdin.".into());
    }

    if library::window::should_relaunch_in_conhost() {
        let _ = library::window::relaunch_in_conhost();
        std::process::exit(0);
    }

    let mut tui_config = TuiBootstrapConfig::new("trance");
    tui_config.size = (100, 35);

    let (mut terminal, _guards) = bootstrap_tui(tui_config)?;

    let screensavers = preview::discover();

    let global = GlobalConfig::load();
    let local = LocalConfig::load();
    let theme = TuiTheme::detect(theme_override);
    log_environment(&theme);

    let mut app = App::new(screensavers, global, local, theme);

    let mut status_ttl: u32 = 0;
    let mut last_sleep_prevented = false;
    let mut sync_check_timer: u32 = 0;

    loop {
        if library::apps::tui_bootstrap::is_app_shutting_down() {
            break;
        }
        if app.should_quit {
            break;
        }
        app.sync_power_status_if_needed();

        // Apply the sleep-inhibition state to the OS.  We only call into
        // Win32 when the desired state changes, so a stationary event loop
        // does no work.
        if app.local.prevent_sleep != last_sleep_prevented {
            win32::set_thread_execution_state(app.local.prevent_sleep);
            last_sleep_prevented = app.local.prevent_sleep;
        }

        terminal.draw(|f| ui::render(&mut app, f))?;

        let is_animating = false;

        let poll = if is_animating {
            Duration::from_millis(30)
        } else {
            let base = Duration::from_millis(250);
            if app.on_battery {
                base * 2
            } else {
                base
            }
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
                Event::Mouse(mouse) => match mouse.kind {
                    event::MouseEventKind::Down(event::MouseButton::Left) => {
                        let mut clicked_btn = false;
                        if let Some((btn_y, btn_start, btn_end)) = app.quit_btn_bounds {
                            if mouse.row == btn_y && mouse.column >= btn_start && mouse.column < btn_end {
                                app.should_quit = true;
                                clicked_btn = true;
                            }
                        }
                        if !clicked_btn {
                            if let Some((btn_y, btn_start, btn_end)) = app.help_btn_bounds {
                                if mouse.row == btn_y && mouse.column >= btn_start && mouse.column < btn_end {
                                    app.show_help = !app.show_help;
                                    app.status = Some(crate::app::StatusMessage {
                                        text: if app.show_help {
                                            "Help overlay active. Press ESC/q to close.".to_string()
                                        } else {
                                            "Help overlay closed.".to_string()
                                        },
                                        kind: crate::app::StatusKind::Info,
                                    });
                                    clicked_btn = true;
                                }
                            }
                        }
                        if !clicked_btn {
                            if mouse.row <= 2 {
                                if let Some(cursor_pos) = win32::query_cursor_pos() {
                                    if let Some(rect) = win32::get_window_rect() {
                                        app.drag_active = true;
                                        app.drag_start_cursor = Some(cursor_pos);
                                        app.drag_start_window = Some((rect.left, rect.top));
                                    }
                                }
                            } else {
                                app.selection_start = Some((mouse.column, mouse.row));
                                app.selection_end = Some((mouse.column, mouse.row));
                                app.selection_pending_copy = false;
                            }
                        }
                    }
                    event::MouseEventKind::Drag(event::MouseButton::Left) => {
                        if app.drag_active {
                            if let (Some(start_cursor), Some(start_window)) = (app.drag_start_cursor, app.drag_start_window) {
                                if let Some(curr_cursor) = win32::query_cursor_pos() {
                                    let dx = curr_cursor.0 - start_cursor.0;
                                    let dy = curr_cursor.1 - start_cursor.1;
                                    win32::set_window_pos(start_window.0 + dx, start_window.1 + dy);
                                }
                            }
                        } else if app.selection_start.is_some() {
                            app.selection_end = Some((mouse.column, mouse.row));
                        }
                    }
                    event::MouseEventKind::Up(event::MouseButton::Left) => {
                        if app.drag_active {
                            app.drag_active = false;
                            app.drag_start_cursor = None;
                            app.drag_start_window = None;
                        } else if let (Some(start), Some(end)) = (app.selection_start, app.selection_end) {
                            if start != end {
                                app.selection_pending_copy = true;
                            } else {
                                app.selection_start = None;
                                app.selection_end = None;
                            }
                        }
                    }
                    event::MouseEventKind::ScrollUp => {
                        app.handle_key(KeyCode::Up, KeyModifiers::empty());
                    }
                    event::MouseEventKind::ScrollDown => {
                        app.handle_key(KeyCode::Down, KeyModifiers::empty());
                    }
                    _ => {}
                },
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
    shutdown_tui(&mut terminal)?;
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
