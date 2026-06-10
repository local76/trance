//! Sub-panel rendering components (Preferences & Saver List).
//!
//! **Taxonomy Classification**: Interface (TUI / Panels).

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListState, Paragraph, Wrap};

use crate::app::{App, FocusedSection, GlobalField};

pub fn render_prefs(app: &mut App, frame: &mut Frame, area: Rect) {
    let theme = app.theme;
    let active = app.focused == FocusedSection::GlobalPrefs;
    let border_color = if active { theme.border_active } else { theme.border };

    let prefs_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " Global Screensaver Preferences ",
            Style::default().fg(if active { theme.accent_primary } else { theme.header }).add_modifier(Modifier::BOLD),
        ));

    let active_status = if app.global.active { "ACTIVE" } else { "DISABLED" };
    let active_color = if app.global.active { theme.accent_secondary } else { theme.text_dim };
    
    let sleep_status = if app.local.prevent_sleep { "ACTIVE (SYSTEM AWAKE)" } else { "DISABLED (NORMAL)" };
    let sleep_color = if app.local.prevent_sleep { theme.accent_secondary } else { theme.text_dim };
    
    let hide_stock_status = if app.local.hide_stock { "YES" } else { "NO" };
    let hide_stock_color = if app.local.hide_stock { theme.accent_secondary } else { theme.text_dim };
    
    let timeout_value = format!("{} minutes", app.global.timeout / 60);
    let cycle_time_value = format!("{} seconds", app.local.random_cycle_secs);

    let mut lines = Vec::new();

    let mut add_field = |field: GlobalField, label: &str, value: String, value_color: Color| {
        let focused = active && app.global_field == field;
        let arrow_span = Span::styled(if focused { app.glyphs.play } else { "   " }, Style::default().fg(theme.accent_primary));
        let label_style = if focused {
            Style::default().fg(theme.accent_secondary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_main)
        };
        lines.push(Line::from(vec![
            arrow_span,
            Span::styled(label.to_string(), label_style),
            Span::styled(" ", Style::default()),
            Span::styled(value, Style::default().fg(value_color)),
        ]));
    };

    add_field(GlobalField::Active,       "Active:        ", active_status.to_string(), active_color);
    add_field(GlobalField::Timeout,      "Timeout:       ", timeout_value, theme.accent_primary);
    add_field(GlobalField::PreventSleep, "Prevent sleep: ", sleep_status.to_string(), sleep_color);
    add_field(GlobalField::CycleTime,    "Cycle time:    ", cycle_time_value, theme.accent_primary);
    add_field(GlobalField::HideStock,    "Hide stock:    ", hide_stock_status.to_string(), hide_stock_color);

    let prefs_inner = prefs_block.inner(area);
    frame.render_widget(prefs_block, area);
    frame.render_widget(Paragraph::new(lines), prefs_inner);
}

pub fn render_list(app: &mut App, frame: &mut Frame, area: Rect) {
    let theme = app.theme;
    let active = app.focused == FocusedSection::SaverList;
    let border_color = if active { theme.border_active } else { theme.border };

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " Screensaver Preferences ",
            Style::default().fg(if active { theme.accent_primary } else { theme.header }).add_modifier(Modifier::BOLD),
        ));

    let list_inner = list_block.inner(area);
    frame.render_widget(list_block, area);

    let list_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Table Header
            Constraint::Min(1),    // List Items
        ])
        .split(list_inner);

    let header_line = Line::from(vec![
        Span::raw("   "),
        Span::styled("ACTIVE    ", if active { theme.accent_primary } else { theme.header }),
        Span::styled("NAME                              ", Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD)),
        Span::styled("TYPE", Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD)),
    ]);
    frame.render_widget(Paragraph::new(header_line), list_chunks[0]);

    let indices = app.filtered_indices();

    if indices.is_empty() {
        let text = vec![
            Line::from("  No .scr files found."),
            Line::from(Span::styled(
                "  Drop one into %APPDATA%\\local76\\trance\\screensavers",
                Style::default().fg(theme.text_dim),
            )),
        ];
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), list_chunks[1]);
        return;
    }

    let total_items = indices.len();
    let visible_height = list_chunks[1].height as usize;
    let selected_pos = indices
        .iter()
        .position(|&i| i == app.highlighted)
        .unwrap_or(0);

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

    let items: Vec<ratatui::widgets::ListItem> = visible_indices
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
        .highlight_symbol(if active { app.glyphs.play } else { app.glyphs.play_empty });
    frame.render_stateful_widget(list, list_chunks[1], &mut state);
}
