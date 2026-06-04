//! Ratatui-based rendering.  Pure function of `App` -> `Frame`.
//!
//! # Model-Render Split
//! SSM uses a strict Model-Render architectural split:
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
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, FocusedSection, GlobalField};

const STATUS_TTL_EVENTS: u32 = 30; // about 7.5s at 250ms poll

/// Number of rows reserved for the help block (2 borders + 8 content lines).
const HELP_ROWS: u16 = 10;
/// Number of rows reserved for the global-prefs block (2 borders + 4 content
/// lines + 1 padding).
const PREFS_ROWS: u16 = 7;
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
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme.border));
    let mut lines = vec![Line::from(vec![
        Span::styled(
            "SCREEN SAVER MANAGEMENT",
            Style::default()
                .fg(theme.accent_secondary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  (SSM)", Style::default().fg(theme.text_dim)),
    ])];
    if let Some(ref status) = app.status {
        let color = match status.kind {
            crate::app::StatusKind::Info => theme.accent_secondary,
            crate::app::StatusKind::Error => theme.missing,
        };
        lines.push(Line::from(vec![
            Span::styled("● ", Style::default().fg(color)),
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
            Constraint::Length(1), // applied
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
    let timeout_value = format!("{} minutes", app.global.timeout / 60);
    let (active_path, exists) = active_screensaver_info(app);
    let applied_name = active_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("(none)");
    let applied_color = if exists {
        theme.text_main
    } else {
        theme.missing
    };
    let applied_suffix = if !exists && !app.global.active_scr.is_empty() {
        Span::styled("  (missing)", Style::default().fg(theme.missing))
    } else {
        Span::raw("")
    };

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

    let applied_line = Line::from(vec![
        Span::styled("    ", Style::default()),
        Span::styled("Applied:        ", Style::default().fg(theme.text_dim)),
        Span::styled(applied_name, Style::default().fg(applied_color)),
        applied_suffix,
    ]);
    frame.render_widget(Paragraph::new(applied_line), rows[3]);
}

fn active_screensaver_info(app: &App) -> (std::path::PathBuf, bool) {
    let path = std::path::PathBuf::from(&app.global.active_scr);
    let exists = app.screensavers.iter().any(|s| s.path == path) || path.exists();
    (path, exists)
}

fn render_list(app: &mut App, frame: &mut Frame, area: Rect) {
    let theme = app.theme;
    let active = app.focused == FocusedSection::SaverList;
    let title = if app.filtering {
        Line::from(vec![
            Span::styled(" Screen Savers ", Style::default().fg(theme.header)),
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
            Span::styled(" Screen Savers ", Style::default().fg(theme.header)),
            Span::styled("— Filter: ", Style::default().fg(theme.text_dim)),
            Span::styled(&app.filter, Style::default().fg(theme.accent_secondary)),
            Span::styled(
                " (Press Esc to clear) ",
                Style::default().fg(theme.text_dim),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Screen Savers ", Style::default().fg(theme.header)),
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
                    "Drop one into %APPDATA%\\SSM\\screensavers",
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

    let total_items = indices.len();
    let visible_height = inner.height as usize;
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
    frame.render_stateful_widget(list, inner, &mut state);
}

fn render_help(theme: crate::theme::TuiTheme, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(Span::styled(" Help ", Style::default().fg(theme.header)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled("[Tab]     ", Style::default().fg(theme.accent_primary)),
            Span::raw("cycle focus"),
        ]),
        Line::from(vec![
            Span::styled("[↑/↓]     ", Style::default().fg(theme.accent_primary)),
            Span::raw("navigate"),
        ]),
        Line::from(vec![
            Span::styled("[←/→]     ", Style::default().fg(theme.accent_primary)),
            Span::raw("adjust timeout"),
        ]),
        Line::from(vec![
            Span::styled("[Space/⏎] ", Style::default().fg(theme.accent_primary)),
            Span::raw("toggle / apply"),
        ]),
        Line::from(vec![
            Span::styled("[F5]      ", Style::default().fg(theme.accent_primary)),
            Span::raw("refresh list"),
        ]),
        Line::from(vec![
            Span::styled("[P]       ", Style::default().fg(theme.accent_primary)),
            Span::raw("preview"),
        ]),
        Line::from(vec![
            Span::styled("[C]       ", Style::default().fg(theme.accent_primary)),
            Span::raw("configure settings"),
        ]),
        Line::from(vec![
            Span::styled("[q / Esc] ", Style::default().fg(theme.accent_primary)),
            Span::raw("quit"),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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

/// Clear the status message after `STATUS_TTL_EVENTS` have elapsed.
pub fn status_ttl_events() -> u32 {
    STATUS_TTL_EVENTS
}
