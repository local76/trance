//! Diagnostic and repair tools for trance.
//!
//! **Taxonomy Classification**: Interface (CLI / Diagnostics Layer).

use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
use winreg::RegKey;
#[cfg(target_os = "windows")]
use winreg::enums::HKEY_CURRENT_USER;

use crate::config::{GlobalConfig, LocalConfig};
use crate::backend::preview;
use crate::theme::TuiTheme;
use crate::win32;

/// Run the doctor diagnostic check.
pub fn run_doctor(fix: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("trance Doctor — Diagnostic Report");
    println!("=============================");

    // 1. Check Registry/Config Access
    print!("Config Access:           ");
    #[cfg(target_os = "windows")]
    {
        let desktop = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Control Panel\\Desktop");
        match desktop {
            Ok(_) => println!("OK (Registry Readable)"),
            Err(e) => println!("FAILED (Error: {})", e),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(path) = LocalConfig::config_path() {
            if let Some(parent) = path.parent() {
                if parent.exists() || std::fs::create_dir_all(parent).is_ok() {
                    println!("OK (Config Dir Accessible)");
                } else {
                    println!("FAILED (Cannot access config directory)");
                }
            } else {
                println!("FAILED (Invalid config path parent)");
            }
        } else {
            println!("FAILED (No config path)");
        }
    }

    // 2. Check Active Screensaver
    print!("Active Screensaver Path: ");
    let mut global = GlobalConfig::load();
    if global.active_scr.is_empty() {
        println!("None Configured");
        if fix {
            let discovered = preview::discover();
            if !discovered.is_empty() {
                let first_path = discovered[0].path.to_string_lossy().into_owned();
                global.active_scr = first_path.clone();
                if global.save().is_ok() {
                    println!("    [FIXED] Set active screensaver to first discovered: {}", first_path);
                }
            }
        }
    } else {
        let path = PathBuf::from(&global.active_scr);
        if path.exists() {
            println!("OK ({})", global.active_scr);
        } else {
            println!("MISSING FILE ({})", global.active_scr);
            if fix {
                let discovered = preview::discover();
                let first_valid = discovered.iter().find(|s| s.path.exists());
                if let Some(s) = first_valid {
                    let new_path = s.path.to_string_lossy().into_owned();
                    global.active_scr = new_path.clone();
                    if global.save().is_ok() {
                        println!("    [FIXED] Reset active screensaver to valid path: {}", new_path);
                    }
                }
            }
        }
    }

    // 3. Discovery Directories
    println!("\nDiscovery Directories:");
    if let Ok(appdata) = std::env::var("APPDATA") {
        let rsaver_dir = PathBuf::from(appdata)
            .join("local76")
            .join("trance")
            .join("screensavers");
        let exists = rsaver_dir.exists();
        println!(
            "  - %APPDATA%/local76/trance/screensavers: {}",
            if exists { "EXISTS" } else { "NOT FOUND" }
        );
        if !exists && fix {
            if std::fs::create_dir_all(&rsaver_dir).is_ok() {
                println!("    [FIXED] Created directory: {:?}", rsaver_dir);
            } else {
                println!("    [FAILED] Could not create directory: {:?}", rsaver_dir);
            }
        }
    }
    if let Ok(sys_root) = std::env::var("SystemRoot") {
        let root_path = PathBuf::from(&sys_root);
        println!(
            "  - SystemRoot:                {}",
            if root_path.exists() {
                "EXISTS"
            } else {
                "NOT FOUND"
            }
        );
        let sys32 = root_path.join("System32");
        println!(
            "  - System32:                  {}",
            if sys32.exists() {
                "EXISTS"
            } else {
                "NOT FOUND"
            }
        );
        let syswow64 = PathBuf::from(sys_root).join("SysWOW64");
        println!(
            "  - SysWOW64:                  {}",
            if syswow64.exists() {
                "EXISTS"
            } else {
                "NOT FOUND"
            }
        );
    }

    // 4. Log File Check
    print!("\nLog File Writable:       ");
    let log_path = LocalConfig::config_path()
        .and_then(|p| p.parent().map(|p| p.join("trance.log")))
        .unwrap_or_else(|| PathBuf::from("trance.log"));
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(_) => println!("OK ({:?})", log_path),
        Err(e) => println!("FAILED (Error: {})", e),
    }

    // 4.5 Clipboard Write Access
    print!("Windows Clipboard:        ");
    match win32::copy_text_to_clipboard("trance Diagnostic Test Connection") {
        Ok(_) => println!("OK (Writable)"),
        Err(e) => println!("FAILED (Error: {})", e),
    }

    // 5. Local Preferences Check
    println!("\nLocal Preferences Check:");
    let mut local = LocalConfig::load();
    println!("  - Prevent System Sleep:      {}", if local.prevent_sleep { "ENABLED (Active Awake)" } else { "DISABLED (Normal)" });
    println!("  - Random Cycle Duration:     {} seconds", local.random_cycle_secs);
    println!("  - Selected Cycle Screensavers ({}):", local.selected_paths.len());
    if local.selected_paths.is_empty() {
        println!("      (None selected; default cycle will cycle all discovered screensavers)");
    } else {
        let mut missing_count = 0;
        for path in &local.selected_paths {
            let p = Path::new(path);
            let exists = p.exists();
            if !exists {
                missing_count += 1;
            }
            let status = if exists { "OK" } else { "MISSING FILE" };
            let filename = p.file_name().and_then(|f| f.to_str()).unwrap_or(path);
            println!("      - {} [{}] ({})", filename, status, path);
        }
        if missing_count > 0 && fix {
            local.selected_paths.retain(|path| Path::new(path).exists());
            if local.save().is_ok() {
                println!("    [FIXED] Removed {} missing screensaver(s) from cycle selection.", missing_count);
            }
        }
    }

    // 6. Theme Detection Check
    print!("\nTheme Detection:         ");
    let theme = TuiTheme::detect(None);
    println!(
        "OK (High Contrast: {}, No Color: {})",
        theme.high_contrast, theme.no_color
    );

    println!("\nDiagnostics Complete.");
    Ok(())
}
