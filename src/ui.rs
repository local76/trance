//! Ratatui-based rendering.  Pure function of `App` -> `Frame`.
//!
//! # Model-Render Split
//! WSM uses a strict Model-Render architectural split:
//!
//! * **Model (`app.rs`)**: Owns the state (selected saver, timer configuration, focus, etc.)
//!   and implements the business logic, key handlers, and state modifications.
//! * **Render (`ui.rs`)**: Takes a mutable reference to the `App` state and draws the layout,
//!   widgets, list view, borders, help texts, and active indicators to the screen.
//!
//! The renderer does not manage state or process user input directly; it simply queries
//! the current state fields of `App` and paints them onto the `Frame`.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, FocusedSection, GlobalField};



/// Number of rows reserved for the help block (2 borders + 6 content lines).
const HELP_ROWS: u16 = 8;
/// Number of rows reserved for the global-prefs block (2 borders + 5 content
/// lines + 1 padding).
const PREFS_ROWS: u16 = 9;
/// Number of rows for the title bar (2 lines + 1 bottom border).
const TITLE_ROWS: u16 = 3;

pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let theme = app.theme;
    let (min_w, min_h) = crate::theme::recommended_min_size(96);

    if area.width < min_w || area.height < min_h {
        render_too_small(theme, frame, area);
        return;
    }

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(TITLE_ROWS),
            Constraint::Length(PREFS_ROWS),
            Constraint::Min(3), // saver list
            Constraint::Length(HELP_ROWS),
        ])
        .split(area);

    render_title(app, frame, outer[0]);
    render_prefs(app, frame, outer[1]);
    render_list(app, frame, outer[2]);
    render_help(theme, frame, outer[3]);

    if app.vanity_enabled {
        render_vanity_particles(app, frame);
    }

    if app.notice.is_some() {
        render_notice_overlay(app, frame);
    }
}

fn render_too_small(theme: crate::theme::TuiTheme, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL);
    let (min_w, min_h) = crate::theme::recommended_min_size(96);
    let text = vec![
        Line::from(Span::styled(
            "Terminal too small",
            Style::default()
                .fg(theme.accent_secondary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(format!(
            "Need at least {min_w}x{min_h}, current {}x{}.",
            area.width, area.height
        )),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        area,
    );
}

fn render_title(app: &App, frame: &mut Frame, area: Rect) {
    let theme = app.theme;
    let metrics = crate::win32::SystemMetrics::query();
    let username = std::env::var("USERNAME").unwrap_or_else(|_| std::env::var("USER").unwrap_or_else(|_| "user".to_string()));
    let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "localhost".to_string());
    
    let power_str = if metrics.power.ac_online {
        "AC (Charging)".to_string()
    } else {
        format!("Battery ({}%)", metrics.power.battery_percent)
    };
    
    let theme_mode = if theme.dark_mode { "Dark" } else { "Light" };

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme.border));

    let title_line = Line::from(vec![
        Span::styled(
            " ❖  rSaver  ❖ ",
            Style::default()
                .fg(Color::Rgb(30, 30, 46))
                .bg(theme.border_active)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(theme.border)),
        Span::styled(
            format!("{}@{}", username, hostname),
            Style::default()
                .fg(Color::Rgb(255, 215, 0))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(theme.border)),
        Span::styled(
            format!("Screen: {}x{} ({} DPI)", metrics.screen_w, metrics.screen_h, metrics.dpi),
            Style::default().fg(theme.text_main),
        ),
        Span::styled(" │ ", Style::default().fg(theme.border)),
        Span::styled(
            format!("Power: {}", power_str),
            Style::default().fg(theme.text_main),
        ),
        Span::styled(" │ ", Style::default().fg(theme.border)),
        Span::styled(
            format!("Theme: {}", theme_mode),
            Style::default().fg(theme.text_main),
        ),
    ]);

    let mut lines = vec![title_line];

    #[cfg(feature = "downloader")]
    let mut is_downloading = false;
    #[cfg(feature = "downloader")]
    let mut download_name = String::new();
    #[cfg(feature = "downloader")]
    {
        if let Some(ref state_mutex) = app.download_state {
            is_downloading = true;
            if let Ok(state) = state_mutex.lock() {
                download_name = state.name.clone();
            }
        }
    }

    #[cfg(feature = "downloader")]
    if is_downloading {
        let progress = app.visual_progress;
        let track_width = 30;
        let pacman_pos = ((progress * track_width as f64).round() as usize).min(track_width);
        
        let mut track = String::new();
        let is_mouth_open = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() / 150)
            .unwrap_or(0) % 2) == 0;
            
        let pacman_char = if progress >= 1.0 {
            "o"
        } else if is_mouth_open {
            "ᗧ"
        } else {
            "o"
        };

        if progress < 1.0 {
            for _ in 0..pacman_pos {
                track.push(' ');
            }
            track.push_str(pacman_char);
            for i in (pacman_pos + 1)..track_width {
                if i == track_width - 1 {
                    track.push('ᗣ'); // Ghost
                } else {
                    track.push('·'); // Dots
                }
            }
        } else {
            for _ in 0..track_width.saturating_sub(1) {
                track.push(' ');
            }
            track.push('o');
        }

        lines.push(Line::from(vec![
            Span::styled("  ● ", Style::default().fg(theme.accent_secondary)),
            Span::styled(format!("Downloading ({}): ", download_name), Style::default().fg(theme.text_main).add_modifier(Modifier::BOLD)),
            Span::styled(" [", Style::default().fg(theme.border)),
            Span::styled(track, Style::default().fg(theme.accent_primary)),
            Span::styled("]", Style::default().fg(theme.border)),
            Span::styled(format!(" {:>3.0}%", progress * 100.0), Style::default().fg(theme.accent_secondary)),
        ]));
    } else if let Some(ref status) = app.status {
        let color = match status.kind {
            crate::app::StatusKind::Info => theme.accent_secondary,
            crate::app::StatusKind::Error => theme.missing,
        };
        lines.push(Line::from(vec![
            Span::styled("  ● ", Style::default().fg(color)),
            Span::styled(
                &status.text,
                Style::default()
                    .fg(theme.text_main)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        lines.push(Line::raw(""));
    }

    #[cfg(not(feature = "downloader"))]
    if let Some(ref status) = app.status {
        let color = match status.kind {
            crate::app::StatusKind::Info => theme.accent_secondary,
            crate::app::StatusKind::Error => theme.missing,
        };
        lines.push(Line::from(vec![
            Span::styled("  ● ", Style::default().fg(color)),
            Span::styled(
                &status.text,
                Style::default()
                    .fg(theme.text_main)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        lines.push(Line::raw(""));
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_prefs(app: &mut App, frame: &mut Frame, area: Rect) {
    let theme = app.theme;
    let active = app.focused == FocusedSection::GlobalPrefs;
    let block = Block::default()
        .title(Span::styled(
            " Global System Preferences ",
            Style::default().fg(theme.header),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if active {
            theme.border_active
        } else {
            theme.border
        }));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // active
            Constraint::Length(1), // timeout
            Constraint::Length(1), // prevent sleep
            Constraint::Length(1), // cycle time
            Constraint::Length(1), // hide stock
            Constraint::Length(1), // vanity mode
        ])
        .split(inner);

    let active_status = if app.global.active {
        "ACTIVE"
    } else {
        "DISABLED"
    };
    let active_color = if app.global.active {
        theme.accent_secondary
    } else {
        theme.text_dim
    };
    let sleep_status = if app.local.prevent_sleep {
        "ACTIVE (SYSTEM AWAKE)"
    } else {
        "DISABLED (NORMAL)"
    };
    let sleep_color = if app.local.prevent_sleep {
        theme.accent_secondary
    } else {
        theme.text_dim
    };
    let hide_stock_status = if app.local.hide_stock {
        "YES"
    } else {
        "NO"
    };
    let hide_stock_color = if app.local.hide_stock {
        theme.accent_secondary
    } else {
        theme.text_dim
    };
    let vanity_status = if app.vanity_enabled {
        "ACTIVE (E.G. FIREWORKS ON APPLY)"
    } else {
        "DISABLED"
    };
    let vanity_color = if app.vanity_enabled {
        theme.accent_secondary
    } else {
        theme.text_dim
    };
    let timeout_value = format!("{} minutes", app.global.timeout / 60);
    let cycle_time_value = format!("{} seconds", app.local.random_cycle_secs);

    let mut field_row =
        |row: Rect, field: GlobalField, label: &'static str, value: String, value_color| {
            let focused = active && app.global_field == field;
            let arrow_style = if focused {
                Style::default().fg(theme.accent_primary)
            } else {
                Style::default()
            };
            let label_style = if focused {
                Style::default().fg(theme.accent_secondary)
            } else {
                Style::default().fg(theme.text_main)
            };
            let line = Line::from(vec![
                Span::styled(if focused { "▶ " } else { "  " }, arrow_style),
                Span::styled(label, label_style),
                Span::styled("  ", Style::default()),
                Span::styled(value, Style::default().fg(value_color)),
            ]);
            frame.render_widget(Paragraph::new(line), row);
        };

    field_row(
        rows[0],
        GlobalField::Active,
        "Active:         ",
        active_status.to_string(),
        active_color,
    );
    field_row(
        rows[1],
        GlobalField::Timeout,
        "Timeout:        ",
        timeout_value,
        theme.accent_primary,
    );
    field_row(
        rows[2],
        GlobalField::PreventSleep,
        "Prevent sleep:  ",
        sleep_status.to_string(),
        sleep_color,
    );
    field_row(
        rows[3],
        GlobalField::CycleTime,
        "Cycle time:     ",
        cycle_time_value,
        theme.accent_primary,
    );
    field_row(
        rows[4],
        GlobalField::HideStock,
        "Hide stock:     ",
        hide_stock_status.to_string(),
        hide_stock_color,
    );
    field_row(
        rows[5],
        GlobalField::VanityMode,
        "Vanity mode:    ",
        vanity_status.to_string(),
        vanity_color,
    );
}

fn render_list(app: &mut App, frame: &mut Frame, area: Rect) {
    let theme = app.theme;
    let active = app.focused == FocusedSection::SaverList;

    let title = if app.filtering {
        Line::from(vec![
            Span::styled(" Screen Saver Preferences ", Style::default().fg(theme.header)),
            Span::styled("— Filter: ", Style::default().fg(theme.text_dim)),
            Span::styled(
                &app.filter,
                Style::default()
                    .fg(theme.accent_secondary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "_",
                Style::default()
                    .fg(theme.accent_primary)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
            Span::raw(" "),
        ])
    } else if !app.filter.is_empty() {
        Line::from(vec![
            Span::styled(" Screen Saver Preferences ", Style::default().fg(theme.header)),
            Span::styled("— Filter: ", Style::default().fg(theme.text_dim)),
            Span::styled(&app.filter, Style::default().fg(theme.accent_secondary)),
            Span::styled(
                " (Press Esc to clear) ",
                Style::default().fg(theme.text_dim),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Screen Saver Preferences ", Style::default().fg(theme.header)),
            Span::styled(
                "— Press [/] to filter ",
                Style::default().fg(theme.text_dim),
            ),
        ])
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if active {
            theme.border_active
        } else {
            theme.border
        }));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let indices = app.filtered_indices();

    if indices.is_empty() {
        let text = if app.screensavers.is_empty() {
            vec![
                Line::from("No .scr files found."),
                Line::from(Span::styled(
                    "Drop one into %APPDATA%\\wsm\\screensavers",
                    Style::default().fg(theme.text_dim),
                )),
            ]
        } else {
            vec![
                Line::from(Span::styled(
                    "No matches for filter.",
                    Style::default().fg(theme.missing),
                )),
                Line::from(Span::styled(
                    "Press Esc to clear the filter.",
                    Style::default().fg(theme.text_dim),
                )),
            ]
        };
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), inner);
        return;
    }

    let list_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Table Header
            Constraint::Min(1),    // List items
        ])
        .split(inner);

    let header_line = Line::from(vec![
        Span::styled("  STATUS        ", Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD)),
        Span::styled("LOCATION     ", Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD)),
        Span::styled("FRIENDLY NAME             ", Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD)),
        Span::styled("FILE NAME           ", Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD)),
        Span::styled("TYPE", Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD)),
    ]);
    frame.render_widget(Paragraph::new(header_line), list_layout[0]);

    let total_items = indices.len();
    let visible_height = list_layout[1].height as usize;
    let selected_pos = indices
        .iter()
        .position(|&i| i == app.highlighted)
        .unwrap_or(0);

    // Adjust list_offset to keep selected_pos in view
    if selected_pos < app.list_offset {
        app.list_offset = selected_pos;
    } else if selected_pos >= app.list_offset + visible_height {
        app.list_offset = selected_pos - visible_height + 1;
    }
    if app.list_offset + visible_height > total_items {
        app.list_offset = total_items.saturating_sub(visible_height);
    }

    let start = app.list_offset;
    let end = (start + visible_height).min(total_items);
    let visible_indices = &indices[start..end];

    let items: Vec<ListItem> = visible_indices
        .iter()
        .map(|&i| app.list_items[i].clone())
        .collect();

    let mut state = ListState::default().with_selected(Some(selected_pos.saturating_sub(start)));
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(theme.text_main)
                .bg(theme.bg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(if active { "▶ " } else { "▷ " });
    frame.render_stateful_widget(list, list_layout[1], &mut state);
}

fn render_help(theme: crate::theme::TuiTheme, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(Span::styled(" Keyboard Shortcuts ", Style::default().fg(theme.header)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let col1 = [
        ("[Tab]", "cycle focus between preferences and list"),
        ("[↑/↓]", "navigate preferences or screensavers"),
        ("[←/→]", "adjust timeout or cycle time"),
        ("[Space]", "toggle checkboxes or system active status"),
        ("[Enter]", "apply config to registry"),
        ("[/]", "filter screensavers by name"),
    ];

    let col2 = [
        ("[F5 / R]", "re-scan system screensavers"),
        ("[P]", "launch fullscreen preview"),
        ("[C]", "open native configuration"),
        ("[D]", "delete screensaver from disk"),
        ("[V]", "toggle interactive vanity"),
        ("[q/Esc]", "quit manager interface"),
    ];

    let mut help_lines = vec![];
    for i in 0..6 {
        let (k1, d1) = col1[i];
        let (k2, d2) = col2[i];
        help_lines.push(Line::from(vec![
            Span::styled(format!("  {:<9}", k1), Style::default().fg(theme.accent_primary)),
            Span::raw(format!("{:<42}", d1)),
            Span::styled(format!("  {:<10}", k2), Style::default().fg(theme.accent_primary)),
            Span::raw(d2),
        ]));
    }

    frame.render_widget(Paragraph::new(help_lines).wrap(Wrap { trim: false }), inner);
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}


fn render_vanity_particles(app: &App, frame: &mut Frame) {
    let theme = app.theme;
    let colors = [
        theme.accent_primary,
        theme.accent_secondary,
        theme.applied,
        theme.text_main,
        theme.text_dim,
    ];
    let buffer = frame.buffer_mut();
    let width = buffer.area.width;
    let height = buffer.area.height;

    // Render background stars
    for star in &app.stars {
        let x = star.x.round() as i16;
        let y = star.y.round() as i16;
        if x >= 0 && x < width as i16 && y >= 0 && y < height as i16 {
            let cell = &mut buffer[(x as u16, y as u16)];
            if cell.symbol() == " " {
                if star.brightness > 0.6 {
                    cell.set_symbol("✦");
                    cell.set_fg(theme.accent_secondary);
                } else if star.brightness > 0.15 {
                    cell.set_symbol("✧");
                    cell.set_fg(theme.text_main);
                } else {
                    cell.set_symbol(".");
                    cell.set_fg(theme.text_dim);
                }
            }
        }
    }

    // Overlay active particles
    for p in &app.particles {
        let x = p.x.round() as i16;
        let y = p.y.round() as i16;
        if x >= 0 && x < width as i16 && y >= 0 && y < height as i16 {
            let cell = &mut buffer[(x as u16, y as u16)];
            if cell.symbol() == " " || cell.symbol() == "." || cell.symbol() == "✧" || cell.symbol() == "✦" {
                cell.set_symbol(p.symbol);
                let color = colors[p.color_idx % colors.len()];
                cell.set_fg(color);
            }
        }
    }
}


pub fn render_notice_overlay(app: &App, frame: &mut Frame) {
    let theme = app.theme;
    let area = frame.area();
    
    let popup_width = 46;
    let popup_height = 5;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width.min(area.width), popup_height.min(area.height));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_secondary))
        .title(Span::styled(" Notice ", Style::default().fg(theme.header)));
    
    frame.render_widget(Clear, popup_area);

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if let Some(ref notice_text) = app.notice {
        let lines = vec![
            Line::from(Span::styled(truncate(notice_text, (inner.width as usize).saturating_sub(2)), Style::default().fg(theme.text_main))),
            Line::raw(""),
            Line::from(Span::styled("[ Press any key to dismiss ]", Style::default().fg(theme.text_dim))),
        ];
        let paragraph = Paragraph::new(lines)
            .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(paragraph, inner);
    }
}
