//! Ratatui-based rendering. Pure function of `App` -> `Frame`.
//!
//! **Taxonomy Classification**: Interface (TUI / Main Rendering Layout).

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use library::ui::layout::centered_rect;

use crate::app::App;

pub mod widgets;
pub mod text_format;

pub use text_format::truncate;

/// Render the entire application interface to the Ratatui Frame.
pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let theme = app.theme;
    let min_w = 100;
    let min_h = 35;

    if area.width < min_w || area.height < min_h {
        render_too_small(theme, frame, area);
        return;
    }

    // Split entire area vertically into bordered boxes
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // 0: Header box
            Constraint::Length(7), // 1: Global Screensaver Preferences (full width)
            Constraint::Min(10),   // 2: Screensaver Preferences list
            Constraint::Length(3), // 3: Status / Progress footer box
        ])
        .split(area);

    // 0. Render Header Info Box
    let header_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " trance - Screensaver Manager ",
            Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD),
        ));
    
    let username = &app.username;
    let hostname = &app.hostname;
    let os_str_val = app.os_version.clone();

    let ver_str = format!("trance v{}", env!("CARGO_PKG_VERSION"));
    let user_host_str = format!("{}@{}", username, hostname);

    let button_y = chunks[0].y + 1;
    let inner_width = chunks[0].width.saturating_sub(2) as usize;
    
    let left_len = ver_str.len() + 3 + user_host_str.len() + 3 + os_str_val.len();
    let right_len = 6 + 3 + 6; // " help " + " │ " + " quit "

    let header_line = if inner_width > left_len + right_len {
        let padding_len = inner_width - (left_len + right_len);
        let padding_str = " ".repeat(padding_len);
        
        let help_offset = 1 + left_len + padding_len;
        let help_start_x = chunks[0].x + help_offset as u16;
        let help_end_x = help_start_x + 6;
        app.help_btn_bounds = Some((button_y, help_start_x, help_end_x));

        let quit_offset = help_offset + 6 + 3;
        let quit_start_x = chunks[0].x + quit_offset as u16;
        let quit_end_x = quit_start_x + 6;
        app.quit_btn_bounds = Some((button_y, quit_start_x, quit_end_x));

        Line::from(vec![
            Span::styled(ver_str, Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD)),
            Span::styled(" │ ", Style::default().fg(theme.border)),
            Span::styled(user_host_str, Style::default().fg(Color::Rgb(255, 215, 0)).add_modifier(Modifier::BOLD)),
            Span::styled(" │ ", Style::default().fg(theme.border)),
            Span::styled(os_str_val, Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD)),
            Span::styled(padding_str, Style::default()),
            Span::styled(" ", Style::default().bg(Color::Rgb(250, 210, 50)).fg(Color::Black).add_modifier(Modifier::BOLD)),
            Span::styled("h", Style::default().bg(Color::Rgb(250, 210, 50)).fg(Color::Black).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("elp ", Style::default().bg(Color::Rgb(250, 210, 50)).fg(Color::Black).add_modifier(Modifier::BOLD)),
            Span::styled(" │ ", Style::default().fg(theme.border)),
            Span::styled(" ", Style::default().bg(Color::Rgb(255, 85, 85)).fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("q", Style::default().bg(Color::Rgb(255, 85, 85)).fg(Color::White).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("uit ", Style::default().bg(Color::Rgb(255, 85, 85)).fg(Color::White).add_modifier(Modifier::BOLD)),
        ])
    } else {
        let help_offset = 1 + ver_str.len() + 3 + user_host_str.len() + 3 + os_str_val.len() + 3;
        let help_start_x = chunks[0].x + help_offset as u16;
        let help_end_x = help_start_x + 6;
        app.help_btn_bounds = Some((button_y, help_start_x, help_end_x));

        let quit_offset = help_offset + 6 + 3;
        let quit_start_x = chunks[0].x + quit_offset as u16;
        let quit_end_x = quit_start_x + 6;
        app.quit_btn_bounds = Some((button_y, quit_start_x, quit_end_x));

        Line::from(vec![
            Span::styled(ver_str, Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD)),
            Span::styled(" │ ", Style::default().fg(theme.border)),
            Span::styled(user_host_str, Style::default().fg(Color::Rgb(255, 215, 0)).add_modifier(Modifier::BOLD)),
            Span::styled(" │ ", Style::default().fg(theme.border)),
            Span::styled(os_str_val, Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD)),
            Span::styled(" │ ", Style::default().fg(theme.border)),
            Span::styled(" ", Style::default().bg(Color::Rgb(250, 210, 50)).fg(Color::Black).add_modifier(Modifier::BOLD)),
            Span::styled("h", Style::default().bg(Color::Rgb(250, 210, 50)).fg(Color::Black).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("elp ", Style::default().bg(Color::Rgb(250, 210, 50)).fg(Color::Black).add_modifier(Modifier::BOLD)),
            Span::styled(" │ ", Style::default().fg(theme.border)),
            Span::styled(" ", Style::default().bg(Color::Rgb(255, 85, 85)).fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("q", Style::default().bg(Color::Rgb(255, 85, 85)).fg(Color::White).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("uit ", Style::default().bg(Color::Rgb(255, 85, 85)).fg(Color::White).add_modifier(Modifier::BOLD)),
        ])
    };
    let header_inner = header_block.inner(chunks[0]);
    frame.render_widget(header_block, chunks[0]);
    frame.render_widget(Paragraph::new(header_line), header_inner);

    // 1. Render Global Screensaver Preferences (full width)
    widgets::render_prefs(app, frame, chunks[1]);

    // 2. Render Screensaver Preferences List Table
    widgets::render_list(app, frame, chunks[2]);

    // 3. Render Footer Status Box
    let footer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            " Status ",
            Style::default().fg(theme.accent_primary).add_modifier(Modifier::BOLD),
        ));

    let footer_p = if let Some(ref status) = app.status {
        let (color, icon) = match status.kind {
            crate::app::StatusKind::Info => (theme.accent_primary, app.glyphs.info),
            crate::app::StatusKind::Error => (theme.missing, app.glyphs.status_err),
        };
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{} ", icon), Style::default().fg(color)),
            Span::styled(&status.text, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ]))
    } else {
        Paragraph::new(Line::from(vec![
            Span::styled("Ready. Press Tab to cycle focus.", Style::default().fg(theme.text_dim)),
        ]))
    };

    let footer_inner = footer_block.inner(chunks[3]);
    frame.render_widget(footer_block, chunks[3]);
    frame.render_widget(footer_p, footer_inner);

    // Handle Mouse Selection Highlights & Clipboard Copy
    if let (Some(start), Some(end)) = (app.selection_start, app.selection_end) {
        let buf = frame.buffer_mut();
        let width = buf.area.width;
        let height = buf.area.height;

        let (col1, row1) = start;
        let (col2, row2) = end;

        let is_selected = |x: u16, y: u16| -> bool {
            let (c1, r1) = (col1, row1);
            let (c2, r2) = (col2, row2);
            if r1 == r2 {
                y == r1 && x >= c1.min(c2) && x <= c1.max(c2)
            } else if r1 < r2 {
                (y == r1 && x >= c1) || (y > r1 && y < r2) || (y == r2 && x <= c2)
            } else {
                (y == r2 && x >= c2) || (y > r2 && y < r1) || (y == r1 && x <= c1)
            }
        };

        // 1. Draw Highlight
        for y in 0..height {
            for x in 0..width {
                if is_selected(x, y) {
                    let cell = &mut buf[(x, y)];
                    cell.set_bg(Color::Rgb(0, 120, 215));
                    cell.set_fg(Color::White);
                }
            }
        }

        // 2. Perform Copy on Release
        if app.selection_pending_copy {
            let mut selected_text = String::new();
            let mut current_row: Option<u16> = None;
            let mut current_line = String::new();

            for y in 0..height {
                for x in 0..width {
                    if is_selected(x, y) {
                        let cell = &buf[(x, y)];
                        if current_row != Some(y) {
                            if current_row.is_some() {
                                selected_text.push_str(current_line.trim_end());
                                selected_text.push('\n');
                                current_line.clear();
                            }
                            current_row = Some(y);
                        }
                        current_line.push_str(cell.symbol());
                    }
                }
            }
            if !current_line.is_empty() {
                selected_text.push_str(current_line.trim_end());
            }

            if !selected_text.is_empty() {
                let _ = crate::win32::copy_text_to_clipboard(&selected_text);
                let truncated = if selected_text.len() > 30 {
                    format!("{}...", &selected_text[..27].replace('\n', " "))
                } else {
                    selected_text.replace('\n', " ")
                };
                app.status = Some(crate::app::StatusMessage {
                    text: format!("{} Copied selection to clipboard: {}", app.glyphs.clipboard, truncated),
                    kind: crate::app::StatusKind::Info,
                });
            }

            app.selection_start = None;
            app.selection_end = None;
            app.selection_pending_copy = false;
        }
    }

    // 5. Scrollable Markdown Document Viewer Modal
    if let Some(ref filename) = app.show_markdown {
        let area = centered_rect(85, 80, frame.area());
        let popup_block = Block::default()
            .title(format!(
                " Document Viewer: {} (Press Esc/q to Close) ",
                filename
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent_primary));

        let paragraph = Paragraph::new(app.markdown_lines.clone())
            .block(popup_block)
            .wrap(ratatui::widgets::Wrap { trim: true })
            .alignment(ratatui::layout::Alignment::Left)
            .scroll((app.markdown_scroll as u16, 0));

        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    // 6. Help Shortcuts Overlay Modal
    if app.show_help {
        let area = centered_rect(65, 75, frame.area());
        let popup_block = Block::default()
            .title(" Keyboard Shortcuts & App Commands ")
            .title_style(
                Style::default()
                    .fg(theme.accent_primary)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent_primary));

        let key_col_width = 18;
        let border_padding = 2;
        let total_inner_width = area.width.saturating_sub(border_padding);
        let max_desc_width = (total_inner_width as usize)
            .saturating_sub(key_col_width)
            .saturating_sub(2); // for ": "

        let mut help_text = Vec::new();
        help_text.push(Line::from(""));

        help_text.extend(text_format::format_help_row(
            "Tab / Shift-Tab",
            "Cycle active panel focus",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "Up / Down",
            "Navigate lists and preference fields",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "Left / Right",
            "Adjust settings and toggle flags",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "Space / Enter",
            "Toggle screensaver selection / Apply settings",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "p / t",
            "Preview highlighted screensaver",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "c / C",
            "Configure highlighted screensaver",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "d / D",
            "Delete downloaded screensaver from list",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "r / R",
            "Refresh screensavers list",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "Esc / q",
            "Close dialogs / Help Overlay, or Quit application",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "h / H",
            "Toggle this help shortcut overlay modal",
            max_desc_width,
            &theme,
        ));

        help_text.push(Line::from(""));
        help_text.extend(text_format::format_help_row(
            "F1",
            "View README.md document",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "F2",
            "View SUPPORT.md document",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "F3",
            "View LICENSE.md document",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "F4",
            "View COPYRIGHT.md document",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "F5",
            "View PRIVACY.md document",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "F6",
            "View SECURITY.md document",
            max_desc_width,
            &theme,
        ));
        help_text.extend(text_format::format_help_row(
            "F7",
            "View CONTRIBUTING.md document",
            max_desc_width,
            &theme,
        ));

        help_text.push(Line::from(""));
        help_text.extend(text_format::format_help_row(
            "CLI Subcommands",
            "trance.exe [tui | doctor]",
            max_desc_width,
            &theme,
        ));

        frame.render_widget(Clear, area);
        let paragraph = Paragraph::new(help_text).block(popup_block);
        frame.render_widget(paragraph, area);
    }
}

fn render_too_small(theme: crate::theme::TuiTheme, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL);
    let min_w = 100;
    let min_h = 35;
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


