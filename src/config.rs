//! Two pieces of persisted state:
//!  - `GlobalConfig` lives in the Windows registry under
//!    `HKCU\Control Panel\Desktop` (the keys Windows itself uses).
//!  - `LocalConfig` lives at `%APPDATA%\ssm\config.yaml` and tracks
//!    ssm-specific preferences (last selection, prevent-sleep).

use std::path::PathBuf;

use winreg::RegKey;
use winreg::enums::*;

const REG_DESKTOP: &str = if cfg!(test) {
    "Software\\ssm\\TestDesktop"
} else {
    "Control Panel\\Desktop"
};
const REG_SCR: &str = "SCRNSAVE.EXE";
const REG_ACTIVE: &str = "ScreenSaveActive";
const REG_TIMEOUT: &str = "ScreenSaveTimeOut";
const DEFAULT_TIMEOUT_SECS: u32 = 600;

#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub active_scr: String,
    pub active: bool,
    pub timeout: u32,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        GlobalConfig {
            active_scr: String::new(),
            active: false,
            timeout: DEFAULT_TIMEOUT_SECS,
        }
    }
}

impl GlobalConfig {
    pub fn load() -> Self {
        let desktop = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(REG_DESKTOP)
            .ok();
        let get =
            |name: &str| -> Option<String> { desktop.as_ref()?.get_value::<String, _>(name).ok() };
        let active_scr = get(REG_SCR).unwrap_or_default();
        let active = get(REG_ACTIVE).as_deref() == Some("1");
        let timeout = get(REG_TIMEOUT)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);
        GlobalConfig {
            active_scr,
            active,
            timeout,
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let res = (|| {
            let (desktop, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(REG_DESKTOP)?;
            desktop.set_value(REG_SCR, &self.active_scr)?;
            desktop.set_value(REG_ACTIVE, &if self.active { "1" } else { "0" })?;
            desktop.set_value(REG_TIMEOUT, &self.timeout.to_string())?;

            // Propagate settings changes to the OS immediately
            if !cfg!(test) {
                crate::win32::update_screensaver_active(self.active);
                crate::win32::update_screensaver_timeout(self.timeout);
            }
            Ok(())
        })();
        if let Err(ref e) = res {
            tracing::error!(error = %e, "Failed to save global configuration to registry");
        }
        res
    }
}

#[derive(Debug, Clone)]
pub struct LocalConfig {
    pub last_selected: Option<String>,
    pub prevent_sleep: bool,
    /// Hidden/advanced setting for power users to customize the random cycle interval (in seconds).
    pub random_cycle_secs: u32,
    pub selected_paths: Vec<String>,
    pub hide_stock: bool,
    pub vanity_mode: bool,
}

impl Default for LocalConfig {
    fn default() -> Self {
        LocalConfig {
            last_selected: None,
            prevent_sleep: false,
            random_cycle_secs: 30,
            selected_paths: Vec::new(),
            hide_stock: false,
            vanity_mode: false,
        }
    }
}

impl LocalConfig {
    pub fn config_path() -> Option<PathBuf> {
        let appdata = std::env::var("APPDATA").ok()?;
        Some(PathBuf::from(appdata).join("ssm").join("config.yaml"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let mut out = Self::default();
        for line in content.lines() {
            if let Some(v) = line.strip_prefix("last_selected: ") {
                out.last_selected = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("prevent_sleep: ") {
                out.prevent_sleep = v.trim() == "true";
            } else if let Some(v) = line.strip_prefix("random_cycle_secs: ") {
                if let Ok(secs) = v.trim().parse::<u32>() {
                    out.random_cycle_secs = secs;
                }
            } else if let Some(v) = line.strip_prefix("selected_paths: ") {
                out.selected_paths = v
                    .split(';')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if let Some(v) = line.strip_prefix("hide_stock: ") {
                out.hide_stock = v.trim() == "true";
            } else if let Some(v) = line.strip_prefix("vanity_mode: ") {
                out.vanity_mode = v.trim() == "true";
            }
        }
        out
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::config_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = format!(
            "last_selected: {}\nprevent_sleep: {}\nrandom_cycle_secs: {}\nselected_paths: {}\nhide_stock: {}\nvanity_mode: {}\n",
            self.last_selected.as_deref().unwrap_or(""),
            self.prevent_sleep,
            self.random_cycle_secs,
            self.selected_paths.join(";"),
            self.hide_stock,
            self.vanity_mode,
        );
        std::fs::write(path, content)
    }
}

#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_config_roundtrip() {
        let _lock = TEST_LOCK.lock().unwrap();
        // Create a unique temp dir for the test to avoid collisions
        let temp_dir = std::env::temp_dir().join(format!(
            "ssm_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Set APPDATA to redirect LocalConfig load/save
        unsafe {
            std::env::set_var("APPDATA", &temp_dir);
        }

        let config = LocalConfig {
            last_selected: Some("mystify.scr".to_string()),
            prevent_sleep: true,
            random_cycle_secs: 45,
            selected_paths: vec![
                "C:\\Windows\\System32\\mystify.scr".to_string(),
                "C:\\Windows\\System32\\bubbles.scr".to_string(),
            ],
            hide_stock: true,
            vanity_mode: true,
        };

        // Save
        config.save().unwrap();

        // Load
        let loaded = LocalConfig::load();
        assert_eq!(loaded.last_selected.as_deref(), Some("mystify.scr"));
        assert!(loaded.prevent_sleep);
        assert_eq!(loaded.random_cycle_secs, 45);
        assert_eq!(
            loaded.selected_paths,
            vec![
                "C:\\Windows\\System32\\mystify.scr".to_string(),
                "C:\\Windows\\System32\\bubbles.scr".to_string(),
            ]
        );
        assert!(loaded.hide_stock);
        assert!(loaded.vanity_mode);

        // Clean up temp dir
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_global_config_roundtrip() {
        let _lock = TEST_LOCK.lock().unwrap();
        // REG_DESKTOP is redirected to "Software\ssm\TestDesktop" in test mode
        let config = GlobalConfig {
            active_scr: "C:\\Windows\\System32\\bubbles.scr".to_string(),
            active: true,
            timeout: 300,
        };

        // Save (this writes to the test key, and doesn't call SystemParametersInfo)
        config.save().unwrap();

        // Load
        let loaded = GlobalConfig::load();
        assert_eq!(loaded.active_scr, "C:\\Windows\\System32\\bubbles.scr");
        assert!(loaded.active);
        assert_eq!(loaded.timeout, 300);

        // Clean up test key in registry
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey("Software\\ssm\\TestDesktop");
    }
}
