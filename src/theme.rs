//! Build a `TuiTheme` from live system data so the UI matches what the user
//! is already looking at.
//!
//! Sources (in order of preference):
//!  1. Windows high-contrast mode (overrides everything).
//!  2. `NO_COLOR` environment variable (no chromatic styling at all).
//!  3. The console's actual 16-color palette (via Win32).
//!  4. The Windows accent color (from DWM).
//!  5. Light/dark mode (from the registry).
//!
//! If any of those queries fail, sensible defaults are used.

use ratatui::style::Color;

use crate::win32::{Palette, Rgb, SystemMetrics};

#[derive(Debug, Clone, Copy)]
pub struct TuiTheme {
    pub border: Color,
    pub border_active: Color,
    pub header: Color,
    pub accent_primary: Color,
    pub accent_secondary: Color,
    pub text_main: Color,
    pub text_dim: Color,
    pub bg: Color,
    pub applied: Color,
    pub missing: Color,
    #[allow(dead_code)]
    pub dark_mode: bool,
    #[allow(dead_code)]
    pub high_contrast: bool,
    pub no_color: bool,
}

impl TuiTheme {
    pub fn detect(theme_override: Option<&str>) -> Self {
        if let Some(o) = theme_override {
            let dark = true; // default to dark for overrides unless specified
            match o.to_ascii_lowercase().as_str() {
                "light" => {
                    return Self::from_metrics_and_palette(
                        crate::win32::SystemMetrics {
                            dark_mode: false,
                            high_contrast: false,
                            ..crate::win32::SystemMetrics::query()
                        },
                        crate::win32::Palette::query(),
                    );
                }
                "dark" => {
                    return Self::from_metrics_and_palette(
                        crate::win32::SystemMetrics {
                            dark_mode: true,
                            high_contrast: false,
                            ..crate::win32::SystemMetrics::query()
                        },
                        crate::win32::Palette::query(),
                    );
                }
                "high-contrast" => return Self::high_contrast(dark),
                "no-color" => return Self::no_color(dark),
                _ => {}
            }
        }
        let metrics = SystemMetrics::query();
        let palette = Palette::query();
        let no_color = std::env::var_os("NO_COLOR").is_some();
        Self::detect_impl(metrics, palette, no_color)
    }

    fn detect_impl(metrics: SystemMetrics, palette: Palette, no_color: bool) -> Self {
        if metrics.high_contrast {
            return Self::high_contrast(metrics.dark_mode);
        }
        if no_color {
            return Self::no_color(metrics.dark_mode);
        }
        Self::from_metrics_and_palette(metrics, palette)
    }

    pub fn high_contrast(dark: bool) -> Self {
        // Windows high-contrast palette: black & white with a single accent
        // (default yellow).  Flips fg/bg for light high-contrast.
        let (main, dim) = if dark {
            (Color::White, Color::White)
        } else {
            (Color::Black, Color::Black)
        };
        let bg = if dark { Color::Black } else { Color::White };
        let accent = Color::Yellow;
        TuiTheme {
            border: dim,
            border_active: accent,
            header: accent,
            accent_primary: accent,
            accent_secondary: accent,
            text_main: main,
            text_dim: dim,
            bg,
            applied: accent,
            missing: accent,
            dark_mode: dark,
            high_contrast: true,
            no_color: false,
        }
    }

    pub fn no_color(dark: bool) -> Self {
        // Honor NO_COLOR (https://no-color.org/).  No chromatic styling;
        // everything is white or black, dim variants are darker shades.
        let main = if dark { Color::White } else { Color::Black };
        let dim = if dark { Color::DarkGray } else { Color::Gray };
        TuiTheme {
            border: dim,
            border_active: main,
            header: main,
            accent_primary: main,
            accent_secondary: main,
            text_main: main,
            text_dim: dim,
            bg: Color::Reset,
            applied: main,
            missing: main,
            dark_mode: dark,
            high_contrast: false,
            no_color: true,
        }
    }

    fn from_metrics_and_palette(metrics: SystemMetrics, palette: Palette) -> Self {
        let dark = metrics.dark_mode;
        // Standard palette indices
        let i = |idx: usize| -> Color { rgb_to_color(palette.colors[idx]) };

        // The dim foreground is palette index 8 in both light and dark mode;
        // the main text flips between white (7) for dark and black (0) for
        // light to maintain contrast.
        let text_dim = i(8);
        let text_main = if dark { i(7) } else { i(0) };
        let bg = Color::Reset;

        // Use the Windows accent for accent_primary when it has any saturation;
        // otherwise fall back to the bright cyan from the palette.
        let accent = metrics.accent;
        let accent_primary = if accent == Rgb(0, 0, 0) {
            i(14) // bright cyan
        } else {
            rgb_to_color(accent)
        };

        TuiTheme {
            border: i(8),
            border_active: accent_primary,
            header: i(14),
            accent_primary,
            accent_secondary: i(11), // bright yellow
            text_main,
            text_dim,
            bg,
            applied: i(10), // bright green
            missing: i(9),  // bright red
            dark_mode: dark,
            high_contrast: false,
            no_color: false,
        }
    }
}

fn rgb_to_color(c: Rgb) -> Color {
    Color::Rgb(c.0, c.1, c.2)
}

/// Recommendation for minimum terminal size, scaled by DPI.
pub fn recommended_min_size(dpi: u32) -> (u16, u16) {
    // 100% DPI => 60x25.  Scale up by DPI for HiDPI displays so the UI is
    // comfortable to read.
    let scale = (dpi as f32 / 96.0).clamp(1.0, 3.0);
    let w = (60.0 * scale).round() as u16;
    let h = (25.0 * scale).round() as u16;
    (w.max(60), h.max(25))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win32::{Palette, PowerStatus, Rgb, SystemMetrics};

    fn mock_metrics(dark_mode: bool, high_contrast: bool) -> SystemMetrics {
        SystemMetrics {
            screen_w: 1920,
            screen_h: 1080,
            dpi: 96,
            window_dpi: 96,
            dark_mode,
            high_contrast,
            accent: Rgb(0, 120, 215),
            power: PowerStatus {
                ac_online: true,
                battery_percent: 100,
            },
        }
    }

    #[test]
    fn test_recommended_min_size() {
        assert_eq!(recommended_min_size(96), (60, 25));
        assert_eq!(recommended_min_size(144), (90, 38)); // 1.5x scale
        assert_eq!(recommended_min_size(192), (120, 50)); // 2x scale
    }

    #[test]
    fn test_theme_detect_high_contrast_dark() {
        let metrics = mock_metrics(true, true);
        let palette = Palette::default();
        let theme = TuiTheme::detect_impl(metrics, palette, false);
        assert!(theme.high_contrast);
        assert!(!theme.no_color);
        assert_eq!(theme.bg, Color::Black);
        assert_eq!(theme.border, Color::White);
        assert_eq!(theme.border_active, Color::Yellow);
    }

    #[test]
    fn test_theme_detect_high_contrast_light() {
        let metrics = mock_metrics(false, true);
        let palette = Palette::default();
        let theme = TuiTheme::detect_impl(metrics, palette, false);
        assert!(theme.high_contrast);
        assert!(!theme.no_color);
        assert_eq!(theme.bg, Color::White);
        assert_eq!(theme.border, Color::Black);
        assert_eq!(theme.border_active, Color::Yellow);
    }

    #[test]
    fn test_theme_detect_no_color_dark() {
        let metrics = mock_metrics(true, false);
        let palette = Palette::default();
        let theme = TuiTheme::detect_impl(metrics, palette, true); // no_color = true
        assert!(!theme.high_contrast);
        assert!(theme.no_color);
        assert_eq!(theme.border, Color::DarkGray);
        assert_eq!(theme.border_active, Color::White);
    }

    #[test]
    fn test_theme_detect_normal_dark() {
        let metrics = mock_metrics(true, false);
        let palette = Palette::default();
        let theme = TuiTheme::detect_impl(metrics, palette, false);
        assert!(!theme.high_contrast);
        assert!(!theme.no_color);
        assert_eq!(theme.bg, Color::Reset);
        assert_eq!(theme.border, Color::Rgb(118, 118, 118)); // dark grey (palette index 8)
        assert_eq!(theme.border_active, Color::Rgb(0, 120, 215)); // accent color
    }
}
