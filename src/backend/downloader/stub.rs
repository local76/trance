#![allow(dead_code, unused_variables)]
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub name: String,
    pub author: String,
    pub description: String,
    pub download_url: Option<String>,
    pub downloads: Option<HashMap<String, String>>,
    pub version: String,
}

impl RegistryEntry {
    pub fn download_url_for_current_platform(&self) -> Option<String> {
        None
    }
}

pub fn generate_xscreensaver_xml(name: &str, label: &str, description: &str) -> String {
    String::new()
}

pub fn current_platform() -> &'static str {
    "linux"
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadStatus {
    Downloading,
    Success,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct DownloadState {
    pub name: String,
    pub progress: f64,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub status: DownloadStatus,
    pub post_install_command: Option<String>,
}

pub fn fetch_registry(url: &str) -> Result<Vec<RegistryEntry>, Box<dyn std::error::Error>> {
    Ok(Vec::new())
}

pub fn load_local_registry() -> Result<Vec<RegistryEntry>, Box<dyn std::error::Error>> {
    Ok(Vec::new())
}

pub fn spawn_download(entry: &RegistryEntry) -> Arc<Mutex<DownloadState>> {
    Arc::new(Mutex::new(DownloadState {
        name: entry.name.clone(),
        progress: 1.0,
        total_bytes: 0,
        downloaded_bytes: 0,
        status: DownloadStatus::Success,
        post_install_command: None,
    }))
}
