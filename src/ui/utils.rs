//! Helper styling and formatting utilities for UI modules.
//!
//! **Taxonomy Classification**: Interface (TUI / UI Utilities).

use ratatui::style::Color;
use ratatui::text::Line;
use crate::theme::TuiTheme;

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}



pub fn format_help_row(
    key: &str,
    description: &str,
    max_desc_width: usize,
    theme: &TuiTheme,
) -> Vec<Line<'static>> {
    let theme_colors = library::interface::tui::design::prelude::ThemeColors {
        border: Color::Reset,
        border_active: Color::Reset,
        text_main: theme.text_main,
        text_dim: Color::Reset,
        accent: theme.accent_primary,
        username: Color::Reset,
        help_btn: Color::Reset,
        quit_btn: Color::Reset,
        warning: Color::Reset,
        success: Color::Reset,
        selection_bg: Color::Reset,
        selection_fg: Color::Reset,
    };
    library::interface::tui::design::prelude::format_help_row(
        key,
        description,
        max_desc_width,
        &theme_colors,
    )
}
