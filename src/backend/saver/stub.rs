#![allow(dead_code, non_camel_case_types, non_snake_case, unused_variables)]
use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn from_argb(argb: u32) -> Self {
        Rgb(
            ((argb >> 16) & 0xff) as u8,
            ((argb >> 8) & 0xff) as u8,
            (argb & 0xff) as u8,
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub colors: [Rgb; 16],
}

impl Default for Palette {
    fn default() -> Self {
        let c = |r, g, b| Rgb(r, g, b);
        Palette {
            colors: [
                c(12, 12, 12),    // 0 black
                c(197, 15, 31),   // 1 red
                c(19, 161, 14),   // 2 green
                c(193, 156, 0),   // 3 yellow
                c(0, 0, 238),     // 4 blue
                c(136, 23, 152),  // 5 magenta
                c(58, 150, 221),  // 6 cyan
                c(204, 204, 204), // 7 white
                c(118, 118, 118), // 8 dark grey
                c(231, 72, 86),   // 9 light red
                c(22, 198, 12),   // 10 light green
                c(249, 241, 165), // 11 light yellow
                c(59, 120, 255),  // 12 light blue
                c(180, 0, 158),   // 13 light magenta
                c(97, 214, 214),  // 14 light cyan
                c(242, 242, 242), // 15 white
            ],
        }
    }
}

impl Palette {
    pub fn query() -> Self {
        Self::default()
    }
}

pub struct SystemMetrics {
    pub screen_w: i32,
    pub screen_h: i32,
    pub dpi: u32,
    pub window_dpi: u32,
    pub dark_mode: bool,
    pub high_contrast: bool,
    pub accent: Rgb,
    pub power: PowerStatus,
}

#[derive(Debug, Clone, Copy)]
pub struct PowerStatus {
    pub ac_online: bool,
    pub battery_percent: u8,
}

impl SystemMetrics {
    pub fn query() -> Self {
        SystemMetrics {
            screen_w: 1920,
            screen_h: 1080,
            dpi: 96,
            window_dpi: 96,
            dark_mode: false,
            high_contrast: false,
            accent: Rgb(0, 120, 215),
            power: PowerStatus {
                ac_online: true,
                battery_percent: 100,
            },
        }
    }
}

pub fn query_power_status() -> PowerStatus {
    PowerStatus {
        ac_online: true,
        battery_percent: 100,
    }
}

pub fn set_thread_execution_state(prevent: bool) {}

pub type RECT = isize;

pub fn console_window_rect() -> Option<isize> {
    None
}

pub fn update_screensaver_active(active: bool) {}

pub fn update_screensaver_timeout(timeout_secs: u32) {}

pub fn get_console_title() -> io::Result<String> {
    Ok(String::new())
}

pub fn set_console_title(title: &str) -> io::Result<()> {
    Ok(())
}

pub struct CycleMask {}

impl CycleMask {
    pub fn new() -> Option<Self> {
        Some(CycleMask {})
    }
}
