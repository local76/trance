//! Downloader module for curated Windows screensavers from the registry.

use std::io::Read;
use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};

/// An entry in the curated screensaver online registry.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RegistryEntry {
    /// Friendly name of the screensaver.
    pub name: String,
    /// Author / developer name.
    pub author: String,
    /// Brief description of the screensaver.
    pub description: String,
    /// Direct URL to download the `.scr` binary.
    pub download_url: String,
    /// Current version string.
    pub version: String,
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
}

/// Fetch registry entry list from the target URL.
pub fn fetch_registry(url: &str) -> Result<Vec<RegistryEntry>, Box<dyn std::error::Error>> {
    let response = ureq::get(url).call()?;
    let body = response.into_string()?;
    let entries: Vec<RegistryEntry> = serde_json::from_str(&body)?;
    Ok(entries)
}

/// Spawn background download of the specified screensaver.
pub fn spawn_download(entry: &RegistryEntry) -> Arc<Mutex<DownloadState>> {
    let name = entry.name.clone();
    let download_url = entry.download_url.clone();
    
    let state = Arc::new(Mutex::new(DownloadState {
        name: name.clone(),
        progress: 0.0,
        total_bytes: 0,
        downloaded_bytes: 0,
        status: DownloadStatus::Downloading,
    }));

    let thread_state = state.clone();
    
    let dest_path = crate::config::LocalConfig::config_path().and_then(|p| {
        p.parent().map(|parent| {
            parent
                .join("screensavers")
                .join(format!("{}.scr", name.to_lowercase().replace(" ", "_")))
        })
    });

    std::thread::spawn(move || {
        let res = (|| -> Result<(), Box<dyn std::error::Error>> {
            let Some(path) = dest_path else {
                return Err("Failed to resolve appdata directory".into());
            };
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let response = ureq::get(&download_url).call()?;
            let total_bytes = response
                .header("Content-Length")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);

            let mut reader = response.into_reader();
            let mut file = File::create(&path)?;
            let mut buffer = [0; 8192];
            let mut downloaded: u64 = 0;

            loop {
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

            if let Ok(mut s) = thread_state.lock() {
                s.status = DownloadStatus::Success;
                s.progress = 1.0;
            }
            Ok(())
        })();

        if let Err(e) = res {
            if let Ok(mut s) = thread_state.lock() {
                s.status = DownloadStatus::Error(e.to_string());
            }
        }
    });

    state
}
