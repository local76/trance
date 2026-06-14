//! Host system information and theme querying utilities.

pub use crate::backend::shell_terminal::query_shell_and_terminal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerStatus {
    pub ac_online: bool,
    pub battery_percent: u8,
}

impl Default for PowerStatus {
    fn default() -> Self {
        Self {
            ac_online: true,
            battery_percent: 100,
        }
    }
}

impl PowerStatus {
    pub const BATTERY_PERCENT_UNKNOWN: u8 = 255;

    pub fn is_battery_percent_unknown(&self) -> bool {
        self.battery_percent == Self::BATTERY_PERCENT_UNKNOWN
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemBiosInfo {
    pub manufacturer: String,
    pub product: String,
    pub model: String,
}

/// Helper structure for caching query results
struct Cached<T> {
    last_updated: std::time::Instant,
    value: T,
    duration: std::time::Duration,
}

impl<T> Cached<T> {
    fn new(value: T, duration: std::time::Duration) -> Self {
        Self {
            last_updated: std::time::Instant::now(),
            value,
            duration,
        }
    }

    fn is_valid(&self) -> bool {
        self.last_updated.elapsed() < self.duration
    }
}

// Cross-platform query_local_ip
pub fn query_local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}

#[path = "sys_info_win.rs"]
#[cfg(target_os = "windows")]
mod win_impl;

#[cfg(target_os = "windows")]
use win_impl as platform_impl;

#[cfg(not(target_os = "windows"))]
mod fallback_impl {
    use super::{PowerStatus, SystemBiosInfo};

    pub fn query_accent_color() -> (u8, u8, u8) { (0, 245, 255) }
    pub fn get_win_accent_color_hex() -> String { "#00F5FF".to_string() }
    pub fn query_high_contrast() -> bool { false }
    pub fn query_os_version() -> String {
        if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("PRETTY_NAME=") {
                    let val = line.split('=').nth(1).unwrap_or("").trim_matches('"');
                    if !val.is_empty() {
                        return val.to_string();
                    }
                }
            }
        }
        "Linux".to_string()
    }
    pub fn query_dark_mode() -> bool { true }
    
    pub fn query_power_status() -> Option<PowerStatus> {
        let mut ac_online = true;
        let mut has_ac = false;
        let mut battery_percent: Option<u8> = None;
        if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(ty_str) = std::fs::read_to_string(path.join("type")) {
                    match ty_str.trim() {
                        "Mains" => {
                            if let Ok(online_str) = std::fs::read_to_string(path.join("online")) {
                                let online = online_str.trim() == "1";
                                if !has_ac {
                                    ac_online = online;
                                    has_ac = true;
                                } else {
                                    ac_online = ac_online || online;
                                }
                            }
                        }
                        "Battery" => {
                            if let Ok(cap_str) = std::fs::read_to_string(path.join("capacity")) {
                                if let Ok(pct) = cap_str.trim().parse::<u8>() {
                                    battery_percent = Some(pct);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        battery_percent.map(|pct| PowerStatus {
            ac_online: if has_ac { ac_online } else { true },
            battery_percent: pct,
        })
    }

    pub fn query_bios_info() -> Option<SystemBiosInfo> {
        let manufacturer = std::fs::read_to_string("/sys/class/dmi/id/sys_vendor")
            .ok()
            .unwrap_or_default()
            .trim()
            .to_string();
        let product = std::fs::read_to_string("/sys/class/dmi/id/product_name")
            .ok()
            .unwrap_or_default()
            .trim()
            .to_string();
        let model = std::fs::read_to_string("/sys/class/dmi/id/product_version")
            .ok()
            .unwrap_or_default()
            .trim()
            .to_string();
        if manufacturer.is_empty() && product.is_empty() && model.is_empty() {
            None
        } else {
            Some(SystemBiosInfo {
                manufacturer,
                product,
                model,
            })
        }
    }

    pub fn query_gpu_names() -> Vec<String> {
        let mut gpus = Vec::new();
        if let Ok(output) = std::process::Command::new("lspci").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let lower = line.to_lowercase();
                if lower.contains("vga compatible controller:") || lower.contains("3d controller:") || lower.contains("display controller:") {
                    if let Some(c_idx) = lower.find("controller:") {
                        let name = line[c_idx + 11..].trim().to_string();
                        if !name.is_empty() {
                            gpus.push(name);
                        }
                    } else if let Some(last_colon) = line.rfind(':') {
                        let name = line[last_colon + 1..].trim().to_string();
                        if !name.is_empty() {
                            gpus.push(name);
                        }
                    }
                }
            }
        }
        if gpus.is_empty() {
            vec!["Unknown GPU".to_string()]
        } else {
            gpus
        }
    }

    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const u8,
    }

    unsafe extern "C" {
        fn time(time: *mut i64) -> i64;
        fn localtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
    }

    pub fn get_local_time_string() -> String {
        unsafe {
            let mut t = 0i64;
            time(&mut t);
            let mut tm = std::mem::zeroed::<Tm>();
            if !localtime_r(&t, &mut tm).is_null() {
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                    tm.tm_year + 1900,
                    tm.tm_mon + 1,
                    tm.tm_mday,
                    tm.tm_hour,
                    tm.tm_min,
                    tm.tm_sec
                )
            } else {
                "2026-06-06 12:00:00".to_string()
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
use fallback_impl as platform_impl;

#[derive(Debug, Clone, Copy)]
pub struct GlyphMap {
    pub status_ok: &'static str,
    pub status_err: &'static str,
    pub info: &'static str,
    pub warning: &'static str,
    pub cpu: &'static str,
    pub gpu: &'static str,
    pub memory: &'static str,
    pub disk: &'static str,
    pub package: &'static str,
    pub battery: &'static str,
    pub shell: &'static str,
    pub terminal: &'static str,
    pub network: &'static str,
    pub clipboard: &'static str,
    pub play: &'static str,
    pub play_empty: &'static str,
}

impl GlyphMap {
    pub fn load() -> Self {
        let (_, terminal) = query_shell_and_terminal();
        if terminal == "Windows Console Host" {
            Self {
                status_ok: "[OK]",
                status_err: "[ERR]",
                info: "[i]",
                warning: "[!]",
                cpu: "[CPU]",
                gpu: "[GPU]",
                memory: "[RAM]",
                disk: "[DISK]",
                package: "[PKG]",
                battery: "[BAT]",
                shell: "[SH]",
                terminal: "[TERM]",
                network: "[NET]",
                clipboard: "[CLIP]",
                play: "> ",
                play_empty: "  ",
            }
        } else {
            Self {
                status_ok: "✔️",
                status_err: "❌",
                info: "ℹ️",
                warning: "⚠️",
                cpu: "🧠",
                gpu: "🎮",
                memory: "📟",
                disk: "💾",
                package: "📦",
                battery: "🔋",
                shell: "🐚",
                terminal: "📟",
                network: "🌐",
                clipboard: "📋",
                play: "▶ ",
                play_empty: "  ",
            }
        }
    }
}

fn get_global_theme_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(|appdata| {
            std::path::PathBuf::from(appdata)
                .join("local76")
                .join("theme.yaml")
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var("HOME").ok().map(|home| {
                    std::path::PathBuf::from(home).join(".config")
                })
            })
            .map(|b| b.join("local76").join("theme.yaml"))
    }
}

static GLOBAL_THEME_CACHE: std::sync::OnceLock<std::sync::Mutex<(Option<(Option<(u8, u8, u8)>, Option<bool>)>, std::time::Instant)>> = std::sync::OnceLock::new();

pub fn load_global_theme() -> (Option<(u8, u8, u8)>, Option<bool>) {
    let cache_mutex = GLOBAL_THEME_CACHE.get_or_init(|| std::sync::Mutex::new((None, std::time::Instant::now())));
    let mut cache = cache_mutex.lock().unwrap();
    if let Some(ref val) = cache.0 {
        if cache.1.elapsed() < std::time::Duration::from_secs(1) {
            return val.clone();
        }
    }
    let val = load_global_theme_raw();
    cache.0 = Some(val.clone());
    cache.1 = std::time::Instant::now();
    val
}

fn load_global_theme_raw() -> (Option<(u8, u8, u8)>, Option<bool>) {
    if let Some(path) = get_global_theme_path() {
        if let Ok(content) = std::fs::read_to_string(path) {
            let mut accent = None;
            let mut dark = None;
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some(idx) = line.find(':') {
                    let key = line[..idx].trim();
                    let val = line[idx + 1..].trim().trim_matches('"').trim_matches('\'');
                    match key {
                        "accent_color" => {
                            if !val.is_empty() && val != "none" {
                                if val.starts_with('#') && val.len() == 7 {
                                    let r = u8::from_str_radix(&val[1..3], 16).unwrap_or(0);
                                    let g = u8::from_str_radix(&val[3..5], 16).unwrap_or(245);
                                    let b = u8::from_str_radix(&val[5..7], 16).unwrap_or(255);
                                    accent = Some((r, g, b));
                                }
                            }
                        }
                        "dark_mode" | "is_dark_mode" => {
                            if let Ok(b) = val.parse::<bool>() {
                                dark = Some(b);
                            }
                        }
                        _ => {}
                    }
                }
            }
            return (accent, dark);
        }
    }
    (None, None)
}

pub fn query_accent_color() -> (u8, u8, u8) {
    if let (Some(accent), _) = load_global_theme() {
        return accent;
    }
    platform_impl::query_accent_color()
}

pub fn get_win_accent_color_hex() -> String {
    let (r, g, b) = query_accent_color();
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

pub fn get_dwm_accent_color() -> ratatui::style::Color {
    let (r, g, b) = query_accent_color();
    ratatui::style::Color::Rgb(r, g, b)
}

pub fn query_dark_mode() -> bool {
    if let (_, Some(dark)) = load_global_theme() {
        return dark;
    }
    platform_impl::query_dark_mode()
}

pub use platform_exports::*;

#[cfg(target_os = "windows")]
#[allow(unused_imports)]
mod platform_exports {
    pub use super::platform_impl::{
        query_high_contrast, query_os_version, query_power_status,
        query_bios_info, query_gpu_names, get_local_time_string,
    };
}

#[cfg(not(target_os = "windows"))]
#[allow(unused_imports)]
mod platform_exports {
    pub use super::platform_impl::{
        query_high_contrast, query_os_version, query_power_status,
        query_bios_info, query_gpu_names, get_local_time_string,
    };
}
