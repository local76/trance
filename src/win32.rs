//! Safe wrappers around the Win32 APIs that WSM touches.
//!
//! Everything that calls into `windows-sys` lives here so the rest of the
//! codebase never needs `unsafe`.

use std::ffi::c_void;
use std::ptr::NonNull;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::System::Console::{
    CONSOLE_SCREEN_BUFFER_INFOEX, GetConsoleScreenBufferInfoEx, GetConsoleTitleW, GetConsoleWindow,
    SetConsoleTitleW,
};
use windows_sys::Win32::System::Console::{GetStdHandle, STD_OUTPUT_HANDLE};
use windows_sys::Win32::System::Power::{
    ES_AWAYMODE_REQUIRED, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
    GetSystemPowerStatus, SYSTEM_POWER_STATUS, SetThreadExecutionState,
};
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetSystemMetrics, GetWindowLongPtrW, SM_CXSCREEN, SM_CYSCREEN, SPI_GETHIGHCONTRAST,
    SPI_SETSCREENSAVEACTIVE, SPI_SETSCREENSAVETIMEOUT, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos,
    SystemParametersInfoW, WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU, WS_THICKFRAME,
};

const STYLE_MASK_TO_STRIP: i32 =
    (WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU) as i32;

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

/// 16-color console palette. Index matches the standard ANSI / Windows color
/// table (0 = black, 1 = red, ..., 8 = bright black / dark grey, etc).
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub colors: [Rgb; 16],
}

impl Default for Palette {
    fn default() -> Self {
        // Windows console defaults; used as a fallback if the API call fails.
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
                c(204, 204, 204), //7 white
                c(118, 118, 118), //8 dark grey
                c(231, 72, 86),   // 9 light red
                c(22, 198, 12),   // 10 light green
                c(249, 241, 165), //11 light yellow
                c(59, 120, 255),  //12 light blue
                c(180, 0, 158),   // 13 light magenta
                c(97, 214, 214),  //14 light cyan
                c(242, 242, 242), //15 white
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

// SAFETY: Caller must ensure standard handle handles are valid.
unsafe fn stdout_handle() -> Option<NonNull<c_void>> {
    // SAFETY: STD_OUTPUT_HANDLE is query-safe.
    let h = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if h.is_null() { None } else { NonNull::new(h) }
}

/// RAII guard that strips the console window's title bar / borders / system
/// menu and restores them on drop.
pub struct BorderlessConsole {
    hwnd: HWND,
    original_style: i32,
    original_rect: RECT,
    active: bool,
}

impl BorderlessConsole {
    pub fn enable() -> Self {
        let hwnd = unsafe { GetConsoleWindow() };
        if hwnd.is_null() {
            return BorderlessConsole {
                hwnd: std::ptr::null_mut(),
                original_style: 0,
                original_rect: unsafe { std::mem::zeroed() },
                active: false,
            };
        }
        let original = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as i32;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
        let mut original_rect: RECT = unsafe { std::mem::zeroed() };
        unsafe {
            GetWindowRect(hwnd, &mut original_rect);
        }
        let new_style = original & !STYLE_MASK_TO_STRIP;
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_STYLE, new_style as isize);
        }

        let metrics = SystemMetrics::query();
        let dpi = metrics.window_dpi;
        let scale = dpi as f32 / 96.0;
        let width = (780.0 * scale) as i32;
        let height = (520.0 * scale) as i32;
        let x = (metrics.screen_w - width) / 2;
        let y = (metrics.screen_h - height) / 2;

        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                x,
                y,
                width,
                height,
                SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }

        BorderlessConsole {
            hwnd,
            original_style: original,
            original_rect,
            active: true,
        }
    }
}

impl Drop for BorderlessConsole {
    fn drop(&mut self) {
        if !self.active || self.hwnd.is_null() {
            return;
        }
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWL_STYLE, self.original_style as isize);
            let width = self.original_rect.right - self.original_rect.left;
            let height = self.original_rect.bottom - self.original_rect.top;
            SetWindowPos(
                self.hwnd,
                std::ptr::null_mut(),
                self.original_rect.left,
                self.original_rect.top,
                width,
                height,
                SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
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
    pub battery_percent: u8, // 0..=100, 255 = unknown
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
            dark_mode: query_dark_mode(),
            high_contrast: query_high_contrast(),
            accent: query_accent_color(),
            power: query_power_status(),
        }
    }
}

// SAFETY: Caller must verify query target memory representation matching T.
unsafe fn system_parameters_info_get<T>(action: u32, mut payload: T) -> Option<T> {
    let size = std::mem::size_of::<T>() as u32;
    // SAFETY: SPI query is safe with correct type size and layout.
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

/// Tell Windows whether the calling thread should keep the system / display
/// awake.  `prevent = true` requests ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
/// | ES_AWAYMODE_REQUIRED; `prevent = false` returns to the default
/// ES_CONTINUOUS state.  Always pairs with `ES_CONTINUOUS` so subsequent
/// changes take effect immediately.
pub fn set_thread_execution_state(prevent: bool) {
    let flags = if prevent {
        ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED | ES_AWAYMODE_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    unsafe { SetThreadExecutionState(flags) };
}

fn query_dark_mode() -> bool {
    // AppsUseLightTheme = 0 means dark mode is on.
    use winreg::RegKey;
    use winreg::enums::*;
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
    else {
        return true; // default to dark if we can't tell
    };
    key.get_value::<u32, _>("AppsUseLightTheme")
        .map(|v| v == 0)
        .unwrap_or(true)
}

fn query_accent_color() -> Rgb {
    // DwmGetColorizationColor returns an ARGB color (0xAARRGGBB).
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmGetColorizationColor(pcr_color: *mut u32, pf_opaque_blend: *mut i32) -> i32;
    }
    let mut color: u32 = 0;
    let mut _opaque: i32 = 0;
    let hr = unsafe { DwmGetColorizationColor(&mut color, &mut _opaque) };
    if hr != 0 {
        return Rgb(0, 120, 215); // canonical Windows blue
    }
    Rgb::from_argb(color)
}

fn query_power_status() -> PowerStatus {
    let mut s: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetSystemPowerStatus(&mut s) };
    if ok == 0 {
        return PowerStatus {
            ac_online: true,
            battery_percent: 255,
        };
    }
    PowerStatus {
        ac_online: s.ACLineStatus == 1,
        battery_percent: s.BatteryLifePercent,
    }
}

/// Bounding rect of the console window, in screen pixels.
#[allow(dead_code)]
pub fn console_window_rect() -> Option<RECT> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
    let hwnd = unsafe { GetConsoleWindow() };
    if hwnd.is_null() {
        return None;
    }
    let mut r: RECT = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetWindowRect(hwnd, &mut r) };
    if ok == 0 { None } else { Some(r) }
}

// SAFETY: Action must represent a valid parameter write.
unsafe fn system_parameters_info_set(action: u32, param: u32) {
    // SAFETY: Parameter write fits typical Win32 representation bounds.
    unsafe {
        SystemParametersInfoW(
            action,
            param,
            std::ptr::null_mut(),
            SPIF_SENDCHANGE | SPIF_UPDATEINIFILE,
        );
    }
}

/// Notify the OS whether the screensaver active flag is enabled or disabled.
pub fn update_screensaver_active(active: bool) {
    // SAFETY: SPI_SETSCREENSAVEACTIVE action is safe.
    unsafe {
        system_parameters_info_set(SPI_SETSCREENSAVEACTIVE, active as u32);
    }
}

/// Notify the OS of the screensaver timeout, in seconds.
pub fn update_screensaver_timeout(timeout_secs: u32) {
    // SAFETY: SPI_SETSCREENSAVETIMEOUT action is safe.
    unsafe {
        system_parameters_info_set(SPI_SETSCREENSAVETIMEOUT, timeout_secs);
    }
}

/// Query the current console window title.
pub fn get_console_title() -> std::io::Result<String> {
    let mut buf = [0u16; 512];
    // SAFETY: buf is valid and its size matches the size parameter.
    let len = unsafe { GetConsoleTitleW(buf.as_mut_ptr(), buf.len() as u32) };
    if len == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(String::from_utf16_lossy(&buf[..len as usize]))
}

/// Set the console window title.
pub fn set_console_title(title: &str) -> std::io::Result<()> {
    let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: title_w is null-terminated and its pointer is valid.
    let ok = unsafe { SetConsoleTitleW(title_w.as_ptr()) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// A guard that holds a named system mutex to ensure only one instance of WSM TUI is running.
pub struct SingleInstanceGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

impl SingleInstanceGuard {
    /// Attempt to acquire the single-instance mutex. Returns Err if another instance is running.
    pub fn try_new() -> Result<Self, String> {
        use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn CreateMutexW(
                lp_mutex_attributes: *const std::ffi::c_void,
                b_initial_owner: i32,
                lp_name: *const u16,
            ) -> windows_sys::Win32::Foundation::HANDLE;
        }

        let name: Vec<u16> = "Local\\WSM_SingleInstanceMutex_2026"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: The name pointer is valid and null-terminated.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
        if handle as isize == 0 || handle as isize == -1 {
            return Err("Failed to create single-instance mutex.".to_string());
        }

        // SAFETY: GetLastError is safe to call.
        let err = unsafe { GetLastError() };
        if err == ERROR_ALREADY_EXISTS {
            // SAFETY: CloseHandle is safe to call on non-null handle.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
            return Err("Another instance of WSM is already running.".to_string());
        }

        Ok(SingleInstanceGuard { handle })
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        if self.handle as isize != 0 && self.handle as isize != -1 {
            // SAFETY: CloseHandle is safe to call on valid non-null handle.
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

/// A temporary topmost full-screen black window to mask desktop flashes during cycle transition.
pub struct CycleMask {
    hwnd: HWND,
}

unsafe extern "system" fn mask_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    // SAFETY: DefWindowProcW is safe to call
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

impl CycleMask {
    /// Create and show a new topmost black full-screen window to cover the screen.
    pub fn new() -> Option<Self> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            RegisterClassW, CreateWindowExW, ShowWindow, WNDCLASSW, WS_POPUP, SW_SHOW,
            CS_HREDRAW, CS_VREDRAW, WS_EX_TOPMOST, PeekMessageW, TranslateMessage,
            DispatchMessageW, MSG,
        };
        use windows_sys::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};

        let class_name: Vec<u16> = "wsm_mask_class\0".encode_utf16().collect();

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

                // Pump pending paint/create messages once to guarantee background renders black
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
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                DestroyWindow, PeekMessageW, TranslateMessage, DispatchMessageW, MSG,
            };
            unsafe {
                DestroyWindow(self.hwnd);
                // Pump messages briefly to allow clean up
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, 1) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}
