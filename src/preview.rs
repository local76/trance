//! Discover installed `.scr` files on the system.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Screensaver {
    pub name: String,
    pub path: PathBuf,
    #[cfg(feature = "downloader")]
    pub download_url: Option<String>,
}

pub fn discover() -> Vec<Screensaver> {
    let mut list = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for dir in search_dirs() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
                != Some("scr")
            {
                continue;
            }
            // Dedup by lowercase filename (e.g. bubbles.scr) to prevent duplicate
            // listings of stock screensavers present in both System32 and SysWOW64.
            let filename = match path.file_name() {
                Some(f) => f.to_string_lossy().to_lowercase(),
                None => continue,
            };
            if seen.contains(&filename) {
                continue;
            }
            seen.push(filename);

            let name = prettify(&path);
            list.push(Screensaver {
                name,
                path,
                #[cfg(feature = "downloader")]
                download_url: None,
            });
        }
    }

    list.sort_by_key(|a| a.name.to_lowercase());
    list
}

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("rIdle").join("screensavers"));
    }

    if let Ok(system_root) = std::env::var("SystemRoot") {
        let root = PathBuf::from(system_root);
        dirs.push(root.clone());
        dirs.push(root.join("System32"));
        dirs.push(root.join("SysWOW64"));
    }

    dirs
}

fn prettify(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if stem.len() >= 2 && stem.starts_with('r') {
        if let Some(second_char) = stem.chars().nth(1) {
            if second_char.is_uppercase() {
                return stem.to_string();
            }
        }
    }
    let mut chars = stem.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Returns true if the screensaver is a Windows stock screensaver.
pub fn is_stock_screensaver(path: &Path) -> bool {
    let filename = path.file_name()
        .and_then(|f| f.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    matches!(
        filename.as_str(),
        "bubbles.scr"
            | "mystify.scr"
            | "ribbons.scr"
            | "sstext3d.scr"
            | "scrnsave.scr"
            | "photoscreensaver.scr"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_prettify() {
        assert_eq!(
            prettify(Path::new("C:/Windows/System32/mystify.scr")),
            "Mystify"
        );
        assert_eq!(prettify(Path::new("bubbles.scr")), "Bubbles");
        assert_eq!(prettify(Path::new("rFire.scr")), "rFire");
        assert_eq!(prettify(Path::new("rLife.scr")), "rLife");
        assert_eq!(prettify(Path::new("")), "");
        assert_eq!(prettify(Path::new(".scr")), ".scr");
    }
}
