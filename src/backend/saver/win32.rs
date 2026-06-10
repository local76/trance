#![allow(dead_code, non_snake_case, unused_imports)]
use std::ptr::NonNull;
use std::ffi::c_void;
use windows_sys::Win32::Foundation::HWND;
pub use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::System::Console::{
    GetStdHandle, STD_OUTPUT_HANDLE, CONSOLE_SCREEN_BUFFER_INFOEX,
    GetConsoleScreenBufferInfoEx, GetConsoleWindow, GetConsoleTitleW, SetConsoleTitleW
};
use windows_sys::Win32::System::Power::{
    SetThreadExecutionState,
    ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, ES_AWAYMODE_REQUIRED
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SystemParametersInfoW,
    SPI_GETHIGHCONTRAST, SPI_SETSCREENSAVEACTIVE, SPI_SETSCREENSAVETIMEOUT,
    SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SM_CXSCREEN, SM_CYSCREEN, GetWindowRect,
    WNDCLASSW, CS_HREDRAW, CS_VREDRAW, RegisterClassW, CreateWindowExW,
    ShowWindow, WS_POPUP, SW_SHOW, WS_EX_TOPMOST, PeekMessageW, TranslateMessage,
    DispatchMessageW, MSG, DestroyWindow
};
use windows_sys::Win32::UI::Accessibility::{HIGHCONTRASTW, HCF_HIGHCONTRASTON};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};

/// An RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Convert a 0x00BBGGRR (COLORREF / standard BGR) color into RGB.
    pub const fn from_bgr(bgr: u32) -> Self {
        Rgb(bgr as u8, (bgr >> 8) as u8, (bgr >> 16) as u8)
    }

    /// Convert a 0xAARRGGBB (ARGB) color into RGB.
    pub const fn from_argb(argb: u32) -> Self {
        Rgb((argb >> 16) as u8, (argb >> 8) as u8, argb as u8)
    }
}

/// 16-color console palette.
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
    /// Query the live console palette via `GetConsoleScreenBufferInfoEx`.
    pub fn query() -> Self {
        let stdout = match unsafe { stdout_handle() } {
            Some(h) => h.as_ptr(),
            None => return Self::default(),
        };
        let mut info: CONSOLE_SCREEN_BUFFER_INFOEX = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<CONSOLE_SCREEN_BUFFER_INFOEX>() as u32;
        let ok = unsafe { GetConsoleScreenBufferInfoEx(stdout, &mut info) };
        if ok == 0 {
            return Self::default();
        }
        let mut colors = [Rgb(0, 0, 0); 16];
        for (i, slot) in info.ColorTable.iter().enumerate() {
            colors[i] = Rgb::from_bgr(*slot);
        }
        Palette { colors }
    }
}

unsafe fn stdout_handle() -> Option<NonNull<c_void>> {
    let h = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if h.is_null() { None } else { NonNull::new(h) }
}

/// System metrics collected at startup.
#[derive(Debug, Clone, Copy)]
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
        let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let dpi = unsafe { GetDpiForSystem() };
        let hwnd = unsafe { GetConsoleWindow() };
        let window_dpi = if hwnd.is_null() {
            dpi
        } else {
            unsafe { GetDpiForWindow(hwnd) }
        };

        SystemMetrics {
            screen_w,
            screen_h,
            dpi,
            window_dpi,
            dark_mode: library::sys_info::query_dark_mode(),
            high_contrast: query_high_contrast(),
            accent: query_accent_color(),
            power: query_power_status(),
        }
    }
}

unsafe fn system_parameters_info_get<T>(action: u32, mut payload: T) -> Option<T> {
    let size = std::mem::size_of::<T>() as u32;
    let ok = unsafe { SystemParametersInfoW(action, size, &mut payload as *mut _ as *mut _, 0) };
    if ok == 0 { None } else { Some(payload) }
}

fn query_high_contrast() -> bool {
    let mut hc: HIGHCONTRASTW = unsafe { std::mem::zeroed() };
    hc.cbSize = std::mem::size_of::<HIGHCONTRASTW>() as u32;
    let Some(res) = (unsafe { system_parameters_info_get(SPI_GETHIGHCONTRAST, hc) }) else {
        return false;
    };
    res.dwFlags & HCF_HIGHCONTRASTON != 0
}

fn query_accent_color() -> Rgb {
    // Delegate to library (which uses DWM internally when "widgets" + "sys-info" features are enabled).
    // Convert ratatui Color::Rgb back to our local Rgb for the palette.
    match library::sys_info::get_dwm_accent_color() {
        ratatui::style::Color::Rgb(r, g, b) => Rgb(r, g, b),
        _ => Rgb(0, 120, 215),
    }
}

pub fn query_power_status() -> PowerStatus {
    // Delegate to library for the common power query logic (reduces duplication of GetSystemPowerStatus).
    if let Some(p) = library::sys_info::query_power_status() {
        PowerStatus {
            ac_online: p.ac_online,
            battery_percent: p.battery_percent,
        }
    } else {
        PowerStatus {
            ac_online: true,
            battery_percent: 255,
        }
    }
}

pub fn set_thread_execution_state(prevent: bool) {
    let flags = if prevent {
        ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED | ES_AWAYMODE_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    unsafe { SetThreadExecutionState(flags) };
}

pub fn console_window_rect() -> Option<RECT> {
    let hwnd = unsafe { GetConsoleWindow() };
    if hwnd.is_null() {
        return None;
    }
    let mut r: RECT = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetWindowRect(hwnd, &mut r) };
    if ok == 0 { None } else { Some(r) }
}

unsafe fn system_parameters_info_set(action: u32, param: u32) {
    unsafe {
        SystemParametersInfoW(
            action,
            param,
            std::ptr::null_mut(),
            SPIF_SENDCHANGE | SPIF_UPDATEINIFILE,
        );
    }
}

pub fn update_screensaver_active(active: bool) {
    unsafe {
        system_parameters_info_set(SPI_SETSCREENSAVEACTIVE, active as u32);
    }
}

pub fn update_screensaver_timeout(timeout_secs: u32) {
    unsafe {
        system_parameters_info_set(SPI_SETSCREENSAVETIMEOUT, timeout_secs);
    }
}

pub fn get_console_title() -> std::io::Result<String> {
    let mut buf = [0u16; 512];
    let len = unsafe { GetConsoleTitleW(buf.as_mut_ptr(), buf.len() as u32) };
    if len == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(String::from_utf16_lossy(&buf[..len as usize]))
}

pub fn set_console_title(title: &str) -> std::io::Result<()> {
    let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let ok = unsafe { SetConsoleTitleW(title_w.as_ptr()) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

pub struct CycleMask {
    hwnd: HWND,
}

unsafe extern "system" fn mask_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

impl CycleMask {
    pub fn new() -> Option<Self> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            RegisterClassW, CreateWindowExW, WNDCLASSW, WS_POPUP, SW_SHOW,
            CS_HREDRAW, CS_VREDRAW, WS_EX_TOPMOST
        };
        use windows_sys::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};

        let class_name: Vec<u16> = "trance_mask_class\0".encode_utf16().collect();

        unsafe {
            let wnd_class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(mask_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: std::ptr::null_mut(),
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: GetStockObject(BLACK_BRUSH) as HBRUSH,
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
            };

            RegisterClassW(&wnd_class);

            let metrics = SystemMetrics::query();
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST,
                class_name.as_ptr(),
                std::ptr::null(),
                WS_POPUP,
                0,
                0,
                metrics.screen_w,
                metrics.screen_h,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );

            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_SHOW);

                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, hwnd, 0, 0, 1) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                Some(CycleMask { hwnd })
            } else {
                None
            }
        }
    }
}

impl Drop for CycleMask {
    fn drop(&mut self) {
        if !self.hwnd.is_null() {
            unsafe {
                DestroyWindow(self.hwnd);
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, 1) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}
