//! State mutation and action execution triggers.
//!
//! **Taxonomy Classification**: Interface (TUI / Action Handlers).

use crate::app::{App, StatusMessage, StatusKind};
#[cfg(feature = "downloader")]
use crate::app::PendingAction;

#[cfg(feature = "downloader")]
use crate::backend::downloader;
use crate::backend::preview;

const TIMEOUT_STEP_SECS: u32 = 60;
const TIMEOUT_MIN_SECS: u32 = 60;
const TIMEOUT_MAX_SECS: u32 = 7200;

const CYCLE_TIME_STEP_SECS: u32 = 5;
const CYCLE_TIME_MIN_SECS: u32 = 5;
const CYCLE_TIME_MAX_SECS: u32 = 600;

impl App {
    /// Apply the currently-highlighted screensaver as the system screensaver.
    pub fn apply_highlighted(&mut self) {
        #[cfg(feature = "downloader")]
        if self.trigger_online_download(PendingAction::Apply) {
            return;
        }

        let exe = std::env::current_exe().unwrap_or_default();

        let count = self.local.selected_paths.len();
        if count > 1 {
            self.global.active_scr = exe.to_string_lossy().into_owned();
            self.global.active = true;
            self.status = Some(StatusMessage {
                text: format!("Applied cycle of {} screensavers", count),
                kind: StatusKind::Info,
            });
        } else if count == 1 {
            let path = self.local.selected_paths[0].clone();
            self.global.active_scr = path.clone();
            self.global.active = true;

            // Find the name of the screensaver for the status message
            let name = self.screensavers.iter()
                .find(|s| s.path.to_string_lossy() == path)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "Selected Screensaver".to_string());

            self.status = Some(StatusMessage {
                text: format!("Applied: {}", name),
                kind: StatusKind::Info,
            });
        } else {
            self.global.active_scr = String::new();
            self.global.active = false;
            self.status = Some(StatusMessage {
                text: "Screensaver deactivated (turned off)".to_string(),
                kind: StatusKind::Info,
            });
        }

        if let Err(e) = self.global.save() {
            self.status = Some(StatusMessage {
                text: format!("Failed to save: {e}"),
                kind: StatusKind::Error,
            });
            return;
        }

        if let Some(s) = self.current_screensaver() {
            if let Some(name) = s.path.file_name().and_then(|f| f.to_str()) {
                self.local.last_selected = Some(name.to_string());
            }
        }
        let _ = self.local.save();
        self.update_list_items();
    }

    /// Toggle selection of the highlighted screensaver and immediately apply it to the registry.
    pub fn toggle_and_apply_highlighted(&mut self) {
        #[cfg(feature = "downloader")]
        if self.trigger_online_download(PendingAction::ToggleAndApply) {
            return;
        }

        self.toggle_highlighted_selection();
        self.apply_highlighted();
    }

    /// Toggle the global `active` flag in the registry.
    pub fn toggle_active(&mut self) {
        self.global.active = !self.global.active;
        match self.global.save() {
            Ok(()) => {
                self.status = Some(StatusMessage {
                    text: format!("Active = {}", self.global.active),
                    kind: StatusKind::Info,
                })
            }
            Err(e) => {
                self.status = Some(StatusMessage {
                    text: format!("Save failed: {e}"),
                    kind: StatusKind::Error,
                })
            }
        }
    }

    /// Toggle the "prevent system sleep" mode. The state lives in
    /// `LocalConfig` because it's a per-user preference, not a system one.
    pub fn toggle_prevent_sleep(&mut self) {
        self.local.prevent_sleep = !self.local.prevent_sleep;
        if let Some(s) = self.current_screensaver() {
            if let Some(name) = s.path.file_name().and_then(|f| f.to_str()) {
                self.local.last_selected = Some(name.to_string());
            }
        }
        match self.local.save() {
            Ok(()) => {
                self.status = Some(StatusMessage {
                    text: format!("Prevent sleep = {}", self.local.prevent_sleep),
                    kind: StatusKind::Info,
                })
            }
            Err(e) => {
                self.status = Some(StatusMessage {
                    text: format!("Save failed: {e}"),
                    kind: StatusKind::Error,
                })
            }
        }
    }

    /// Toggle hiding stock windows screensavers.
    pub fn toggle_hide_stock(&mut self) {
        self.local.hide_stock = !self.local.hide_stock;
        if self.local.hide_stock {
            // If stock screensavers are hidden, they should not be active in the cycle.
            self.local.selected_paths.retain(|p| {
                !preview::is_stock_screensaver(std::path::Path::new(p))
            });
        }
        if let Some(s) = self.current_screensaver() {
            if let Some(name) = s.path.file_name().and_then(|f| f.to_str()) {
                self.local.last_selected = Some(name.to_string());
            }
        }
        match self.local.save() {
            Ok(()) => {
                self.resolve_highlight();
                self.status = Some(StatusMessage {
                    text: format!("Hide stock screensavers = {}", self.local.hide_stock),
                    kind: StatusKind::Info,
                });
                self.update_list_items();
            }
            Err(e) => {
                self.status = Some(StatusMessage {
                    text: format!("Save failed: {e}"),
                    kind: StatusKind::Error,
                })
            }
        }
    }

    /// Adjust the screensaver timeout by one step.
    pub fn adjust_timeout(&mut self, delta: i32) {
        let next = (self.global.timeout as i32 + delta * TIMEOUT_STEP_SECS as i32)
            .clamp(TIMEOUT_MIN_SECS as i32, TIMEOUT_MAX_SECS as i32) as u32;
        if next == self.global.timeout {
            return;
        }
        self.global.timeout = next;
        if let Err(e) = self.global.save() {
            self.status = Some(StatusMessage {
                text: format!("Save failed: {e}"),
                kind: StatusKind::Error,
            });
        }
    }

    /// Adjust the screensaver cycle time by one step.
    pub fn adjust_cycle_time(&mut self, delta: i32) {
        let next = (self.local.random_cycle_secs as i32 + delta * CYCLE_TIME_STEP_SECS as i32)
            .clamp(CYCLE_TIME_MIN_SECS as i32, CYCLE_TIME_MAX_SECS as i32) as u32;
        if next == self.local.random_cycle_secs {
            return;
        }
        self.local.random_cycle_secs = next;
        if let Err(e) = self.local.save() {
            self.status = Some(StatusMessage {
                text: format!("Save failed: {e}"),
                kind: StatusKind::Error,
            });
        }
    }

    /// Re-discover screensavers and refresh the list.
    pub fn refresh_screensavers(&mut self) {
        self.screensavers = preview::discover();
        #[cfg(feature = "downloader")]
        {
            let entries = self.registry_entries.clone();
            self.merge_registry_entries(entries);
        }
        self.resolve_highlight();
        self.status = Some(StatusMessage {
            text: "Refreshed screensavers list.".to_string(),
            kind: StatusKind::Info,
        });
        self.update_list_items();
    }

    /// Spawn the currently-highlighted screensaver fullscreen.
    pub fn preview_highlighted(&mut self) {
        #[cfg(feature = "downloader")]
        if self.trigger_online_download(PendingAction::Preview) {
            return;
        }

        let Some(s) = self.current_screensaver() else {
            return;
        };
        if let Err(e) = std::process::Command::new(&s.path).arg("/s").spawn() {
            self.status = Some(StatusMessage {
                text: format!("Preview failed: {e}"),
                kind: StatusKind::Error,
            });
        }
    }

    /// Spawn the currently-highlighted screensaver's native configuration dialog.
    pub fn configure_highlighted(&mut self) {
        #[cfg(feature = "downloader")]
        if self.trigger_online_download(PendingAction::Configure) {
            return;
        }

        let Some(s) = self.current_screensaver() else {
            return;
        };
        if let Err(e) = std::process::Command::new(&s.path).arg("/c").spawn() {
            self.status = Some(StatusMessage {
                text: format!("Configure failed: {e}"),
                kind: StatusKind::Error,
            });
        } else {
            self.status = Some(StatusMessage {
                text: format!("Opened settings for {}", s.name),
                kind: StatusKind::Info,
            });
        }
    }

    /// Delete a downloaded screensaver file from disk.
    pub fn delete_highlighted(&mut self) {
        let (path, name) = {
            let Some(s) = self.current_screensaver() else {
                return;
            };
            (s.path.clone(), s.name.clone())
        };

        if preview::is_stock_screensaver(&path) {
            self.status = Some(StatusMessage {
                text: "Cannot delete stock Windows screensavers.".to_string(),
                kind: StatusKind::Error,
            });
            return;
        }

        if !path.exists() {
            self.status = Some(StatusMessage {
                text: "Screensaver is not downloaded locally.".to_string(),
                kind: StatusKind::Error,
            });
            return;
        }

        match std::fs::remove_file(&path) {
            Ok(()) => {
                self.status = Some(StatusMessage {
                    text: format!("Deleted screensaver: {}", name),
                    kind: StatusKind::Info,
                });
                let path_str = path.to_string_lossy().into_owned();
                if let Some(pos) = self.local.selected_paths.iter().position(|p| p == &path_str) {
                    self.local.selected_paths.remove(pos);
                    let _ = self.local.save();
                }
                self.refresh_screensavers();
            }
            Err(e) => {
                self.status = Some(StatusMessage {
                    text: format!("Failed to delete: {e}"),
                    kind: StatusKind::Error,
                });
            }
        }
    }

    /// Merge online screensaver entries into local list.
    #[cfg(feature = "downloader")]
    pub fn merge_registry_entries(&mut self, entries: Vec<downloader::RegistryEntry>) {
        self.registry_entries = entries.clone();

        let local_filenames: std::collections::HashSet<String> = self.screensavers.iter()
            .map(|s| s.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_lowercase())
            .collect();

        for entry in entries {
            let url = entry.download_url_for_current_platform()
                .or_else(|| entry.download_url.clone())
                .unwrap_or_default();
            if url.is_empty() {
                continue;
            }
            let filename = url.split('/').next_back().unwrap_or("").to_lowercase();
            if filename.is_empty() {
                continue;
            }
            if local_filenames.contains(&filename) {
                continue; // Already downloaded/present locally
            }

            let path = crate::config::LocalConfig::config_path()
                .and_then(|p| p.parent().map(|parent| {
                    parent.join("screensavers").join(&filename)
                }))
                .unwrap_or_else(|| std::path::PathBuf::from(&filename));

            self.screensavers.push(preview::Screensaver {
                name: entry.name,
                path,
                download_url: Some(url),
            });
        }

        // Re-sort alphabetically
        self.screensavers.sort_by_key(|s| s.name.to_lowercase());
        self.resolve_highlight();
        self.update_list_items();
    }

    /// Trigger download of the curated screensaver, performing action once done.
    #[cfg(feature = "downloader")]
    pub fn trigger_online_download(&mut self, action: PendingAction) -> bool {
        if let Some(s) = self.current_screensaver() {
            if s.download_url.is_some() && !s.path.exists() {
                if let Some(ref url) = s.download_url {
                    let entry = downloader::RegistryEntry {
                        name: s.name.clone(),
                        author: String::new(),
                        description: String::new(),
                        download_url: Some(url.clone()),
                        downloads: None,
                        version: String::new(),
                    };
                    self.pending_action = Some(action);
                    self.download_state = Some(downloader::spawn_download(&entry));
                    self.visual_progress = 0.0;
                    return true;
                }
            }
        }
        false
    }

    /// Update visual progress towards the actual download progress.
    pub fn update_download_progress(&mut self) {
        #[cfg(feature = "downloader")]
        if self.download_state.is_some() {
            let mut actual_progress = 0.0;
            if let Some(ref state_mutex) = self.download_state {
                if let Ok(state) = state_mutex.lock() {
                    actual_progress = state.progress;
                }
            }
            // Increment visual progress smoothly
            let target = if actual_progress >= 1.0 { 1.0 } else { actual_progress };
            if self.visual_progress < target {
                self.visual_progress = (self.visual_progress + 0.015).min(target);
            }
        }
    }

    /// Toggle selection of the highlighted screensaver for custom cycling.
    pub fn toggle_highlighted_selection(&mut self) {
        #[cfg(feature = "downloader")]
        if self.trigger_online_download(PendingAction::ToggleSelection) {
            return;
        }

        let (path_str, name) = {
            let Some(s) = self.current_screensaver() else {
                return;
            };
            (s.path.to_string_lossy().into_owned(), s.name.clone())
        };

        if let Some(pos) = self.local.selected_paths.iter().position(|p| p == &path_str) {
            self.local.selected_paths.remove(pos);
            self.status = Some(StatusMessage {
                text: format!("Deselected: {}", name),
                kind: StatusKind::Info,
            });
        } else {
            self.local.selected_paths.push(path_str);
            self.status = Some(StatusMessage {
                text: format!("Selected: {}", name),
                kind: StatusKind::Info,
            });
        }
        let _ = self.local.save();
        self.update_list_items();
    }
}
