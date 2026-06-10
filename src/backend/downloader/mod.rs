//! Downloader module for curated screensavers from the registry.
//! Supports platform-specific downloads (windows, linux, linux-deb, linux-rpm, linux-arch)
//! so trance can fetch the correct binary or package for the current OS.
//!
//! Registry entries can specify a `downloads` map (preferred) or legacy `download_url`.
//! On Linux, best_linux_variant() prefers deb/rpm/arch packages when the matching
//! package manager (dpkg/apt, rpm/dnf, pacman) is detected. Raw ELF + generated .xml
//! (option B) is the fallback. No tarballs — single linux/ folder with ELF + packages.

use std::collections::HashMap;
use std::io::Read;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{info, error};

/// An entry in the curated screensaver online registry.
/// Supports cross-platform downloads via the `downloads` map (preferred).
/// For backward compatibility, a top-level `download_url` is still accepted
/// (treated as the "windows" platform).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RegistryEntry {
    /// Friendly name of the screensaver.
    pub name: String,
    /// Author / developer name.
    pub author: String,
    /// Brief description of the screensaver.
    pub description: String,
    /// Legacy single download URL (used as fallback for "windows").
    #[serde(default)]
    pub download_url: Option<String>,
    /// Platform-specific download URLs, e.g.
    /// "windows": "...beams.scr",
    /// "linux-deb": "...beams.deb"
    #[serde(default)]
    pub downloads: Option<HashMap<String, String>>,
    /// Current version string.
    pub version: String,
}

impl RegistryEntry {
    /// Returns the best download URL for the current platform.
    /// On Linux, prefers the most appropriate package (deb/rpm/arch) based on
    /// detected package managers, falling back to the raw ELF "linux" binary.
    /// This supports the single linux/ folder model (ELF + .deb + .rpm + .pkg.tar.zst).
    pub fn download_url_for_current_platform(&self) -> Option<String> {
        let platform = current_platform();
        if let Some(ref map) = self.downloads {
            if platform == "linux" {
                if let Some(key) = best_linux_variant(map) {
                    if let Some(url) = map.get(&key) {
                        return Some(url.clone());
                    }
                }
                if let Some(url) = map.get("linux") {
                    return Some(url.clone());
                }
            }
            if let Some(url) = map.get(platform) {
                return Some(url.clone());
            }
            if let Some(url) = map.get("linux") {
                return Some(url.clone());
            }
        }
        // Fallback to legacy field (assumed windows)
        self.download_url.clone()
    }
}

fn command_exists(_cmd: &str) -> bool {
    // Use shell builtin 'command -v' which is portable and doesn't require 'which' package.
    #[cfg(unix)]
    {
        std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {} >/dev/null 2>&1", _cmd))
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// On Linux, pick the best package type available in the downloads map
/// based on detected package manager (dpkg/apt, rpm/dnf, pacman).
/// Falls back to raw "linux" ELF if no package manager matches or key missing.
fn best_linux_variant(map: &HashMap<String, String>) -> Option<String> {
    let has = |k: &str| map.contains_key(k);

    // Debian/Ubuntu/Pop!_OS etc.
    if (command_exists("dpkg") || command_exists("apt") || command_exists("apt-get")) && has("linux-deb") {
        return Some("linux-deb".to_string());
    }
    // Fedora/RHEL/SUSE/openSUSE etc.
    if (command_exists("rpm") || command_exists("dnf") || command_exists("yum") || command_exists("zypper")) && has("linux-rpm") {
        return Some("linux-rpm".to_string());
    }
    // Arch/Manjaro/Endeavour etc.
    if command_exists("pacman") && has("linux-arch") {
        return Some("linux-arch".to_string());
    }
    if has("linux") {
        return Some("linux".to_string());
    }
    None
}

/// Generates a minimal xscreensaver .xml descriptor for the given saver.
/// This is option B: generate on the client for simplicity (no need to ship .xml in every linux/ folder).
pub fn generate_xscreensaver_xml(name: &str, label: &str, description: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<screensaver name="{name}" _label="{label}" >
  <command>  %  -root </command>
  <string id="description"
   _label="Description"
   _description="The description of this screensaver."
   >{description}</string>
</screensaver>
"#,
        name = name,
        label = label,
        description = description
    )
}

/// Returns a platform key suitable for the downloads map.
/// "windows", "linux", "macos", etc.
/// On Linux the *caller* (download_url_for_current_platform) selects the best
/// concrete variant (linux-deb / linux-rpm / linux-arch / linux) from the map.
pub fn current_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "linux" => "linux",
        "macos" => "macos",
        _ => "unknown",
    }
}

/// The status of a background screensaver file download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadStatus {
    /// File download is currently in progress.
    Downloading,
    /// File download finished successfully.
    Success,
    /// An error occurred during download.
    Error(String),
}

/// The shared thread-safe state tracking the progress of an active download.
#[derive(Debug, Clone)]
pub struct DownloadState {
    /// Friendly name of the downloading screensaver.
    pub name: String,
    /// Fractional download progress (from 0.0 to 1.0).
    pub progress: f64,
    /// Total size of the file in bytes.
    pub total_bytes: u64,
    /// Number of bytes downloaded so far.
    pub downloaded_bytes: u64,
    /// Current execution status of the download.
    pub status: DownloadStatus,
    /// Optional post-install instruction (primarily for Linux deb/rpm/arch packages).
    /// Example: "sudo dpkg -i /path/to/beams.deb"
    pub post_install_command: Option<String>,
}

/// Fetch registry entry list from the target URL.
/// Each entry may contain platform-specific `downloads` (preferred) or a legacy
/// `download_url`. Callers should use `entry.download_url_for_current_platform()`.
pub fn fetch_registry(url: &str) -> Result<Vec<RegistryEntry>, Box<dyn std::error::Error>> {
    let response = ureq::get(url).call()?;
    let body = response.into_string()?;
    let entries: Vec<RegistryEntry> = serde_json::from_str(&body)?;
    Ok(entries)
}

/// Load the local registry.json (if present).
/// Tries current working dir, then the directory next to the executable,
/// then walks up parent directories (to find it when running from target/release).
/// This is useful during development/testing so you can iterate on the catalog
/// without pushing to GitHub first.
pub fn load_local_registry() -> Result<Vec<RegistryEntry>, Box<dyn std::error::Error>> {
    // Try cwd first (works great with `cargo run`)
    if let Ok(content) = std::fs::read_to_string("registry.json") {
        let entries: Vec<RegistryEntry> = serde_json::from_str(&content)?;
        return Ok(entries);
    }

    // Try next to the exe, and walk up parents (handles running target/release/trance.exe)
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            let local_path = d.join("registry.json");
            if local_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&local_path) {
                    let entries: Vec<RegistryEntry> = serde_json::from_str(&content)?;
                    return Ok(entries);
                }
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    Err("no local registry.json found".into())
}

/// Spawn background download of the specified screensaver for the current platform.
pub fn spawn_download(entry: &RegistryEntry) -> Arc<Mutex<DownloadState>> {
    let name = entry.name.clone();
    let download_url = entry.download_url_for_current_platform()
        .unwrap_or_else(|| entry.download_url.clone().unwrap_or_default());
    
    let state = Arc::new(Mutex::new(DownloadState {
        name: name.clone(),
        progress: 0.0,
        total_bytes: 0,
        downloaded_bytes: 0,
        status: DownloadStatus::Downloading,
        post_install_command: None,
    }));

    let thread_state = state.clone();
    let description = entry.description.clone();
    
    let dest_path = {
        // Cross-platform screensavers drop directory (cache for downloaded artifacts)
        let base = if cfg!(target_os = "windows") {
            crate::config::LocalConfig::config_path()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        } else {
            // Linux / macOS: use XDG data or HOME
            std::env::var("XDG_DATA_HOME")
                .ok()
                .map(PathBuf::from)
                .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".local/share")))
                .map(|p| p.join("local76").join("trance"))
        };

        base.map(|parent| {
            let filename = download_url.split('/').next_back().unwrap_or("screensaver.bin").to_string();
            parent.join("screensavers").join(filename)
        })
    };

    std::thread::spawn(move || {
        let res = (|| -> Result<(), Box<dyn std::error::Error>> {
            let Some(path) = dest_path else {
                return Err("Failed to resolve appdata directory".into());
            };
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            info!("Starting download for '{}' from '{}'", name, download_url);
            let total_bytes: u64;
            let mut downloaded: u64 = 0;
            let mut file = File::create(&path)?;

            if download_url.starts_with("file://") || download_url.starts_with("file:/") {
                // Parse file path from file:// URL
                let file_path_str = download_url.trim_start_matches("file://").trim_start_matches("file:/");
                // On Windows, file:///C:/path/to/file or file://C:/path/to/file or file:/C:/path/to/file
                let file_path_str = file_path_str.strip_prefix('/').unwrap_or(file_path_str);
                let decoded_path = file_path_str.replace("%20", " ");
                let src_path = PathBuf::from(decoded_path);

                info!("Local copy initiated from '{}' to '{}'", src_path.display(), path.display());
                let mut src_file = File::open(&src_path)?;
                total_bytes = src_file.metadata()?.len();

                let mut buffer = [0; 8192];
                loop {
                    if library::lifecycle::foreground::tui_bootstrap::is_app_shutting_down() {
                        return Err("Download cancelled: Application shutting down".into());
                    }
                    let bytes_read = src_file.read(&mut buffer)?;
                    if bytes_read == 0 {
                        break;
                    }
                    file.write_all(&buffer[..bytes_read])?;
                    downloaded += bytes_read as u64;

                    // Update state
                    if let Ok(mut s) = thread_state.lock() {
                        s.downloaded_bytes = downloaded;
                        s.total_bytes = total_bytes;
                        if total_bytes > 0 {
                            s.progress = downloaded as f64 / total_bytes as f64;
                        }
                    }
                    // Add tiny sleep to allow the TUI animation to render for local files
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            } else {
                let response = ureq::get(&download_url)
                    .set("User-Agent", "trance/2.6.4 (+https://github.com/local76)")
                    .call()?;
                total_bytes = response
                    .header("Content-Length")
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);

                let mut reader = response.into_reader();
                let mut buffer = [0; 8192];

                loop {
                    if library::lifecycle::foreground::tui_bootstrap::is_app_shutting_down() {
                        return Err("Download cancelled: Application shutting down".into());
                    }
                    let bytes_read = reader.read(&mut buffer)?;
                    if bytes_read == 0 {
                        break;
                    }
                    file.write_all(&buffer[..bytes_read])?;
                    downloaded += bytes_read as u64;

                    // Update state
                    if let Ok(mut s) = thread_state.lock() {
                        s.downloaded_bytes = downloaded;
                        s.total_bytes = total_bytes;
                        if total_bytes > 0 {
                            s.progress = downloaded as f64 / total_bytes as f64;
                        }
                    }
                }
            }

            // --- Post-download platform-specific handling (Linux xscreensaver) ---
            if cfg!(target_os = "linux") {
                let pstr = path.to_string_lossy().to_lowercase();
                let is_deb = pstr.ends_with(".deb");
                let is_rpm = pstr.ends_with(".rpm");
                let is_arch = pstr.ends_with(".pkg.tar.zst") || pstr.ends_with(".zst");

                if is_deb || is_rpm || is_arch {
                    // Package downloaded to cache. Write a sidecar install hint and expose note.
                    let hint = if is_deb {
                        format!(
                            "sudo dpkg -i \"{}\"\n# or: sudo apt install ./{}",
                            path.display(),
                            path.file_name().unwrap_or_default().to_string_lossy()
                        )
                    } else if is_rpm {
                        format!(
                            "sudo rpm -i \"{}\"\n# or: sudo dnf install \"{}\"",
                            path.display(),
                            path.display()
                        )
                    } else {
                        format!(
                            "sudo pacman -U \"{}\"\n# (Arch Linux package)",
                            path.display()
                        )
                    };
                    // Sidecar file next to the package for easy reference
                    let sidecar = path.with_file_name(format!(
                        "{}.install.txt",
                        path.file_stem().unwrap_or_default().to_string_lossy()
                    ));
                    let _ = std::fs::write(&sidecar, &hint);

                    if let Ok(mut s) = thread_state.lock() {
                        s.post_install_command = Some(hint);
                    }
                } else {
                    // Raw ELF binary ("linux" key). Place executable + generate .xml (option B).
                    // This keeps the published linux/ folder simple: just the ELF + optional packages.
                    let saver_name: String = name
                        .to_lowercase()
                        .chars()
                        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                        .collect();

                    if let Ok(home) = std::env::var("HOME") {
                        let xs_dir = PathBuf::from(&home).join(".xscreensaver");
                        let _ = std::fs::create_dir_all(&xs_dir);

                        // Install the hack binary (executable) to ~/.xscreensaver/<name>
                        let target_bin = xs_dir.join(&saver_name);
                        if let Ok(data) = std::fs::read(&path) {
                            let _ = std::fs::write(&target_bin, &data);
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                if let Ok(mut perms) = std::fs::metadata(&target_bin).map(|m| m.permissions()) {
                                    perms.set_mode(0o755);
                                    let _ = std::fs::set_permissions(&target_bin, perms);
                                }
                            }
                        }

                        // Generate minimal xscreensaver .xml descriptor (client-side, option B)
                        // Place in ~/.xscreensaver/config/<name>.xml so xscreensaver can discover it.
                        let desc = if description.trim().is_empty() {
                            format!("{} (dynamic live OS/kernel)", name)
                        } else {
                            description.clone()
                        };
                        let xml_content = generate_xscreensaver_xml(&saver_name, &name, &desc);
                        let config_dir = xs_dir.join("config");
                        let _ = std::fs::create_dir_all(&config_dir);
                        let xml_path = config_dir.join(format!("{}.xml", saver_name));
                        let _ = std::fs::write(&xml_path, xml_content);
                    }
                }
            }

            if let Ok(mut s) = thread_state.lock() {
                s.status = DownloadStatus::Success;
                s.progress = 1.0;
            }

            Ok(())
        })();

        if let Err(e) = res {
            error!("Download failed for '{}': {}", name, e);
            if let Ok(mut s) = thread_state.lock() {
                s.status = DownloadStatus::Error(e.to_string());
            }
        } else {
            info!("Download succeeded for '{}'", name);
        }
    });

    state
}
