//! Application state, focus, and key bindings.
//!
//! # Model-Render Split
//! WSM uses a strict Model-Render architectural split:
//!
//! * **Model (`app.rs`)**: Owns all the application state, configuration structures,
//!   selection metrics, event handling, and mutations. It is completely decoupled from
//!   direct drawing operations and does not import or know about specific rendering layouts.
//! * **Render (`ui.rs`)**: Responsible for presenting the state stored in `App` onto a
//!   Ratatui `Frame`. It is a pure visual mapping from the current state to the terminal screen.
//!
//! This ensures that the state logic can be easily tested in isolation without having to
//! mock terminal drawing frames or deal with layout constraints.

use std::path::PathBuf;

use crate::config::{GlobalConfig, LocalConfig};
use crate::preview::Screensaver;
use crate::theme::TuiTheme;

const TIMEOUT_STEP_SECS: u32 = 60;
const TIMEOUT_MIN_SECS: u32 = 60;
const TIMEOUT_MAX_SECS: u32 = 7200;

const CYCLE_TIME_STEP_SECS: u32 = 5;
const CYCLE_TIME_MIN_SECS: u32 = 5;
const CYCLE_TIME_MAX_SECS: u32 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedSection {
    GlobalPrefs,
    SaverList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalField {
    Active,
    Timeout,
    PreventSleep,
    CycleTime,
    HideStock,
    VanityMode,
}

impl GlobalField {
    pub const ALL: [GlobalField; 6] = [
        GlobalField::Active,
        GlobalField::Timeout,
        GlobalField::PreventSleep,
        GlobalField::CycleTime,
        GlobalField::HideStock,
        GlobalField::VanityMode,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "downloader")]
pub enum PendingAction {
    Apply,
    ToggleSelection,
    Preview,
    Configure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusMessage {
    pub text: String,
    pub kind: StatusKind,
}

pub struct App {
    pub screensavers: Vec<Screensaver>,
    pub highlighted: usize,
    pub focused: FocusedSection,
    pub global_field: GlobalField,
    pub global: GlobalConfig,
    pub local: LocalConfig,
    pub theme: TuiTheme,
    pub status: Option<StatusMessage>,
    pub should_quit: bool,
    /// Active text in the filter input.  Empty = no filter.
    pub filter: String,
    /// True when the user is typing into the filter.
    pub filtering: bool,
    pub list_offset: usize,
    /// Cached list items for rendering the screensavers list.
    pub list_items: Vec<ratatui::widgets::ListItem<'static>>,
    pub vanity_enabled: bool,
    pub particles: Vec<Particle>,
    pub stars: Vec<Star>,
    pub term_width: u16,
    pub term_height: u16,
    pub visual_progress: f64,
    pub notice: Option<String>,
    #[cfg(feature = "downloader")]
    pub download_state: Option<std::sync::Arc<std::sync::Mutex<crate::downloader::DownloadState>>>,
    #[cfg(feature = "downloader")]
    pub registry_results: Option<std::sync::Arc<std::sync::Mutex<Option<Vec<crate::downloader::RegistryEntry>>>>>,
    #[cfg(feature = "downloader")]
    pub registry_entries: Vec<crate::downloader::RegistryEntry>,
    #[cfg(feature = "downloader")]
    pub pending_action: Option<PendingAction>,
}

impl App {
    pub fn new(
        screensavers: Vec<Screensaver>,
        global: GlobalConfig,
        local: LocalConfig,
        theme: TuiTheme,
    ) -> Self {
        let highlighted = local
            .last_selected
            .as_deref()
            .and_then(|name| {
                screensavers
                    .iter()
                    .position(|s| s.path.file_name().and_then(|f| f.to_str()) == Some(name))
            })
            .unwrap_or(0)
            .min(screensavers.len().saturating_sub(1));

        let vanity_enabled = local.vanity_mode;

        #[cfg(feature = "downloader")]
        let registry_results = {
            let state = std::sync::Arc::new(std::sync::Mutex::new(None));
            let thread_state = state.clone();
            let feed_urls = local.feed_urls.clone();
            std::thread::spawn(move || {
                let mut all_entries = Vec::new();
                for url in feed_urls {
                    if let Ok(entries) = crate::downloader::fetch_registry(&url) {
                        for entry in entries {
                            if !all_entries.iter().any(|e: &crate::downloader::RegistryEntry| e.download_url == entry.download_url) {
                                all_entries.push(entry);
                            }
                        }
                    }
                }
                if !all_entries.is_empty() {
                    if let Ok(mut lock) = thread_state.lock() {
                        *lock = Some(all_entries);
                    }
                }
            });
            Some(state)
        };

        let mut app = App {
            screensavers,
            highlighted,
            focused: FocusedSection::GlobalPrefs,
            global_field: GlobalField::Active,
            global,
            local,
            theme,
            status: None,
            should_quit: false,
            filter: String::new(),
            filtering: false,
            list_offset: 0,
            list_items: Vec::new(),
            vanity_enabled,
            particles: Vec::new(),
            stars: Vec::new(),
            term_width: 80,
            term_height: 25,
            visual_progress: 0.0,
            notice: None,
            #[cfg(feature = "downloader")]
            download_state: None,
            #[cfg(feature = "downloader")]
            registry_results,
            #[cfg(feature = "downloader")]
            registry_entries: Vec::new(),
            #[cfg(feature = "downloader")]
            pending_action: None,
        };
        app.update_list_items();
        app
    }

    /// Indices into `self.screensavers` that match the current filter.
    /// Empty filter → all indices, in order.
    pub fn filtered_indices(&self) -> Vec<usize> {
        let indices: Vec<usize> = if self.filter.is_empty() {
            (0..self.screensavers.len()).collect()
        } else {
            let needle = self.filter.to_lowercase();
            self.screensavers
                .iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    let in_name = s.name.to_lowercase().contains(&needle);
                    let in_path = s.path.to_string_lossy().to_lowercase().contains(&needle);
                    if in_name || in_path { Some(i) } else { None }
                })
                .collect()
        };

        if self.local.hide_stock {
            indices
                .into_iter()
                .filter(|&i| !crate::preview::is_stock_screensaver(&self.screensavers[i].path))
                .collect()
        } else {
            indices
        }
    }

    /// Map a position in the filtered list to the real index, clamping.
    pub fn resolve_highlight(&mut self) {
        let indices = self.filtered_indices();
        if indices.is_empty() {
            self.highlighted = 0;
            return;
        }
        // Try to keep the current highlighted item selected if it's still
        // visible after filtering.
        if let Some(pos) = indices.iter().position(|&i| i == self.highlighted) {
            self.highlighted = indices[pos];
        } else {
            self.highlighted = indices[0];
        }
    }

    /// Update the cached ListItem widgets in `self.list_items`.
    pub fn update_list_items(&mut self) {
        let theme = self.theme;
        let active_filename = std::path::Path::new(&self.global.active_scr)
            .file_name()
            .and_then(|f| f.to_str())
            .map(str::to_lowercase);
        let exe = std::env::current_exe().unwrap_or_default();
        let exe_filename = exe.file_name()
            .and_then(|f| f.to_str())
            .map(str::to_lowercase);
        let is_cycle_active = active_filename.is_some() && active_filename == exe_filename;

        self.list_items = self
            .screensavers
            .iter()
            .map(|s| {
                let s_filename = s.path.file_name()
                    .and_then(|f| f.to_str())
                    .map(str::to_lowercase);
                let is_checked = self.local.selected_paths.contains(&s.path.to_string_lossy().into_owned());
                let is_applied = self.global.active && (if is_cycle_active {
                    is_checked
                } else {
                    s_filename.is_some() && s_filename == active_filename
                });
                let exists = s.path.exists();

                let prefix = if is_checked { "[x]" } else { "[ ]" };
                let status_str = if is_applied {
                    format!("{} Applied", prefix)
                } else if is_checked {
                    format!("{} Active", prefix)
                } else {
                    format!("{} Inactive", prefix)
                };
                let status_color = if is_applied {
                    theme.applied
                } else if is_checked {
                    theme.accent_primary
                } else {
                    theme.text_dim
                };

                let location_str = if !exists {
                    "            ".to_string() // 12 spaces
                } else {
                    let path_lower = s.path.to_string_lossy().to_lowercase();
                    if path_lower.contains("system32") {
                        "\\system32   ".to_string()
                    } else if path_lower.contains("windows") {
                        "\\windows    ".to_string()
                    } else if path_lower.contains("appdata") || path_lower.contains("roaming") {
                        "\\appdata    ".to_string()
                    } else {
                        "\\local      ".to_string()
                    }
                };
                let location_color = if !exists {
                    theme.text_dim
                } else {
                    theme.text_main
                };

                let friendly_name = crate::ui::truncate(&s.name, 24);
                let friendly_str = format!("{:<24}  ", friendly_name);

                let filename = s.path.file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("");
                let filename_truncated = crate::ui::truncate(filename, 18);
                let filename_str = format!("{:<18}  ", filename_truncated);

                let is_stock = crate::preview::is_stock_screensaver(&s.path);
                let stock_str = if is_stock {
                    "Stock"
                } else {
                    "Custom"
                };
                let stock_color = if is_stock {
                    theme.text_dim
                } else {
                    theme.accent_secondary
                };

                let spans = vec![
                    ratatui::text::Span::styled(
                        format!("{:<12}  ", status_str),
                        ratatui::style::Style::default().fg(status_color),
                    ),
                    ratatui::text::Span::styled(
                        location_str,
                        ratatui::style::Style::default().fg(location_color),
                    ),
                    ratatui::text::Span::styled(
                        friendly_str,
                        ratatui::style::Style::default().fg(if is_applied {
                            theme.text_main
                        } else {
                            theme.text_dim
                        }),
                    ),
                    ratatui::text::Span::styled(
                        filename_str,
                        ratatui::style::Style::default().fg(if is_applied {
                            theme.text_main
                        } else {
                            theme.text_dim
                        }),
                    ),
                    ratatui::text::Span::styled(
                        stock_str.to_string(),
                        ratatui::style::Style::default().fg(stock_color),
                    ),
                ];
                ratatui::widgets::ListItem::new(ratatui::text::Line::from(spans))
            })
            .collect();
    }

    pub fn current_screensaver(&self) -> Option<&Screensaver> {
        self.screensavers.get(self.highlighted)
    }

    /// Apply the currently-highlighted screensaver as the system screensaver.
    pub fn apply_highlighted(&mut self) {
        #[cfg(feature = "downloader")]
        if self.trigger_online_download(PendingAction::Apply) {
            return;
        }

        let exe = std::env::current_exe().unwrap_or_default();
        let highlighted_path = if let Some(s) = self.current_screensaver() {
            Some(s.path.to_string_lossy().into_owned())
        } else {
            None
        };

        // Determine if the highlighted screensaver is currently applied and active
        let active_filename = std::path::Path::new(&self.global.active_scr)
            .file_name()
            .and_then(|f| f.to_str())
            .map(str::to_lowercase);
        let exe_filename = exe.file_name()
            .and_then(|f| f.to_str())
            .map(str::to_lowercase);
        let is_cycle_active = active_filename.is_some() && active_filename == exe_filename;

        let is_highlighted_applied = if let Some(ref path_str) = highlighted_path {
            let s_filename = std::path::Path::new(path_str)
                .file_name()
                .and_then(|f| f.to_str())
                .map(str::to_lowercase);
            let is_checked = self.local.selected_paths.contains(path_str);
            self.global.active && (if is_cycle_active {
                is_checked
            } else {
                s_filename.is_some() && s_filename == active_filename
            })
        } else {
            false
        };

        if is_highlighted_applied {
            // Hitting Enter on an already applied screensaver deselects/deactivates it
            self.global.active_scr = String::new();
            self.global.active = false;
            self.local.selected_paths.clear();
            self.status = Some(StatusMessage {
                text: "Screensaver deactivated (turned off)".to_string(),
                kind: StatusKind::Info,
            });
        } else {
            // If selected_paths is empty, automatically check the highlighted screensaver.
            if self.local.selected_paths.is_empty() {
                if let Some(ref path) = highlighted_path {
                    self.local.selected_paths.push(path.clone());
                }
            }

            // Decide what to write to the registry based on selected_paths
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
        self.trigger_firework();
        self.update_list_items();
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

    /// Toggle the "prevent system sleep" mode.  The state lives in
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

    /// Toggle vanity mode (fireworks / background stars) and persist it.
    pub fn toggle_vanity_mode(&mut self) {
        self.vanity_enabled = !self.vanity_enabled;
        self.local.vanity_mode = self.vanity_enabled;
        if let Some(s) = self.current_screensaver() {
            if let Some(name) = s.path.file_name().and_then(|f| f.to_str()) {
                self.local.last_selected = Some(name.to_string());
            }
        }
        match self.local.save() {
            Ok(()) => {
                self.status = Some(StatusMessage {
                    text: format!("Vanity Mode = {}", if self.vanity_enabled { "ACTIVE" } else { "DISABLED" }),
                    kind: StatusKind::Info,
                });
                if self.vanity_enabled {
                    self.trigger_firework();
                }
            }
            Err(e) => {
                self.status = Some(StatusMessage {
                    text: format!("Save failed: {e}"),
                    kind: StatusKind::Error,
                });
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
        self.screensavers = crate::preview::discover();
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

        if crate::preview::is_stock_screensaver(&path) {
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
                self.notice = Some(format!("Deleted screensaver: {}", name));
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
    pub fn merge_registry_entries(&mut self, entries: Vec<crate::downloader::RegistryEntry>) {
        self.registry_entries = entries.clone();

        let local_filenames: std::collections::HashSet<String> = self.screensavers.iter()
            .map(|s| s.path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_lowercase())
            .collect();

        for entry in entries {
            let filename = entry.download_url.split('/').last().unwrap_or("").to_lowercase();
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

            self.screensavers.push(Screensaver {
                name: entry.name,
                path,
                download_url: Some(entry.download_url),
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
                    let entry = crate::downloader::RegistryEntry {
                        name: s.name.clone(),
                        author: String::new(),
                        description: String::new(),
                        download_url: url.clone(),
                        version: String::new(),
                    };
                    self.pending_action = Some(action);
                    self.download_state = Some(crate::downloader::spawn_download(&entry));
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
            // Increment visual progress smoothly (e.g. by 0.02 per frame, roughly ~1.5s total duration for instant downloads)
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
        self.trigger_firework();
        self.update_list_items();
    }

    /// Adjust the highlight in the saver list, clamping to bounds.
    pub fn move_highlight(&mut self, delta: i32) {

        let indices = self.filtered_indices();
        if indices.is_empty() {
            return;
        }
        let current_pos = indices
            .iter()
            .position(|&i| i == self.highlighted)
            .unwrap_or(0);
        let len = indices.len() as i32;
        let next = (current_pos as i32 + delta).rem_euclid(len);
        self.highlighted = indices[next as usize];
    }

    /// Cycle the focused section.
    pub fn cycle_focus(&mut self) {
        self.focused = match self.focused {
            FocusedSection::GlobalPrefs => FocusedSection::SaverList,
            FocusedSection::SaverList => FocusedSection::GlobalPrefs,
        };
    }

    /// Move focus / highlight depending on direction.
    pub fn move_focus(&mut self, delta: i32) {
        match self.focused {
            FocusedSection::GlobalPrefs => {
                let idx = GlobalField::ALL
                    .iter()
                    .position(|f| *f == self.global_field)
                    .unwrap_or(0) as i32;
                let len = GlobalField::ALL.len() as i32;
                let next = (idx + delta).rem_euclid(len);
                self.global_field = GlobalField::ALL[next as usize];
            }
            FocusedSection::SaverList => self.move_highlight(delta),
        }
    }

    /// Handle a single key event.  Returns `true` if the app should quit.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        if self.notice.is_some() {
            self.notice = None;
            return self.should_quit;
        }

        // Clear any error status on any user keypress. Info status remains subject to the timer.
        if let Some(ref msg) = self.status {
            if msg.kind == StatusKind::Error {
                self.status = None;
            }
        }

        if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            return true;
        }

        // While the filter is focused, all printable input goes to the
        // filter buffer; Backspace deletes, Esc clears & exits filter mode.
        if self.filtering {
            match code {
                KeyCode::Esc => {
                    self.filter.clear();
                    self.filtering = false;
                    self.resolve_highlight();
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.resolve_highlight();
                }
                KeyCode::Down => {
                    self.move_focus(1);
                }
                KeyCode::Up => {
                    self.move_focus(-1);
                }
                KeyCode::Enter => {
                    self.on_activate();
                }
                KeyCode::Tab | KeyCode::BackTab => {
                    self.filtering = false;
                    self.cycle_focus();
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.resolve_highlight();
                }
                _ => {}
            }
            return self.should_quit;
        }

        match code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('/') => self.filtering = true,
            KeyCode::F(5) | KeyCode::Char('r') | KeyCode::Char('R') => self.refresh_screensavers(),
            KeyCode::Tab => self.cycle_focus(),
            KeyCode::BackTab => self.cycle_focus(),
            KeyCode::Up => self.move_focus(-1),
            KeyCode::Down => self.move_focus(1),
            KeyCode::Left => self.on_left(),
            KeyCode::Right => self.on_right(),
            KeyCode::Char(' ') => {
                if self.focused == FocusedSection::SaverList {
                    self.toggle_highlighted_selection();
                } else {
                    self.on_activate();
                }
            }
            KeyCode::Enter => self.on_activate(),
            KeyCode::Char('p') | KeyCode::Char('P') | KeyCode::Char('t') | KeyCode::Char('T') => {
                self.preview_highlighted()
            }
            KeyCode::Char('c') | KeyCode::Char('C') => self.configure_highlighted(),
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if self.focused == FocusedSection::SaverList {
                    self.delete_highlighted();
                }
            }
            KeyCode::Char('v') | KeyCode::Char('V') => {
                self.toggle_vanity_mode();
            }
            _ => {}
        }
        self.should_quit
    }

    fn on_left(&mut self) {
        if self.focused == FocusedSection::GlobalPrefs {
            match self.global_field {
                GlobalField::Timeout => self.adjust_timeout(-1),
                GlobalField::CycleTime => self.adjust_cycle_time(-1),
                _ => {}
            }
        }
    }

    fn on_right(&mut self) {
        if self.focused == FocusedSection::GlobalPrefs {
            match self.global_field {
                GlobalField::Timeout => self.adjust_timeout(1),
                GlobalField::CycleTime => self.adjust_cycle_time(1),
                _ => {}
            }
        }
    }

    fn on_activate(&mut self) {
        match self.focused {
            FocusedSection::GlobalPrefs => match self.global_field {
                GlobalField::Active => self.toggle_active(),
                GlobalField::PreventSleep => self.toggle_prevent_sleep(),
                GlobalField::HideStock => self.toggle_hide_stock(),
                GlobalField::VanityMode => self.toggle_vanity_mode(),
                GlobalField::Timeout | GlobalField::CycleTime => {}
            },
            FocusedSection::SaverList => self.apply_highlighted(),
        }
    }

    /// Check if the registry matches the global config, reload if out of sync.
    pub fn check_registry_sync(&mut self) -> bool {
        let current_reg = GlobalConfig::load();
        if current_reg.active_scr != self.global.active_scr
            || current_reg.active != self.global.active
            || current_reg.timeout != self.global.timeout
        {
            self.global = current_reg;
            self.status = Some(StatusMessage {
                text: "External registry change detected! Config reloaded.".to_string(),
                kind: StatusKind::Info,
            });
            self.update_list_items();
            true
        } else {
            false
        }
    }

}

pub use ratatui::crossterm::event::{KeyCode, KeyModifiers};



/// Convenience: kick off the random cycle and return when it finishes.
pub fn run_random_cycle() {
    let local_config = LocalConfig::load();
    let exe = std::env::current_exe().ok();

    let candidates: Vec<PathBuf> = local_config.selected_paths
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists() && !is_self(p, exe.as_ref()) && !is_uninstall(p))
        .collect();

    if candidates.is_empty() {
        return;
    }

    let cycle_duration = std::time::Duration::from_secs(local_config.random_cycle_secs as u64);

    let mut seed: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut mask = None;
    loop {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let idx = (seed as usize) % candidates.len();
        let target = &candidates[idx];
        let mut child = match std::process::Command::new(target).arg("/s").spawn() {
            Ok(c) => c,
            Err(_) => break,
        };

        if mask.is_some() {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let _ = mask.take();
        }

        let start = std::time::Instant::now();
        let mut exited = false;
        while start.elapsed() < cycle_duration {
            match child.try_wait() {
                Ok(Some(_)) => {
                    exited = true;
                    break;
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                Err(_) => {
                    exited = true;
                    break;
                }
            }
        }
        if exited {
            break;
        }
        mask = crate::win32::CycleMask::new();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = child.kill();
    }
}

fn is_self(p: &PathBuf, exe: Option<&PathBuf>) -> bool {
    exe.map(|e| e == p).unwrap_or(false)
}

fn is_uninstall(p: &std::path::Path) -> bool {
    p.file_name()
        .and_then(|f| f.to_str())
        .map(str::to_lowercase)
        .map(|n| n.contains("uninstall"))
        .unwrap_or(false)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Star {
    pub x: f64,
    pub y: f64,
    pub brightness: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParticlePhase {
    Ascent,
    Explosion,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Particle {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub symbol: &'static str,
    pub age: u32,
    pub max_age: u32,
    pub color_idx: usize,
    pub phase: ParticlePhase,
}

impl App {
    /// Generate random background stars matching the terminal bounds
    pub fn generate_stars(&mut self) {
        let width = self.term_width;
        let height = self.term_height;
        let mut stars = Vec::new();
        let mut seed = 12345u64;
        if let Ok(d) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            seed = d.as_micros() as u64;
        }
        let star_count = ((width as u32 * height as u32) / 80).clamp(15, 60);
        for _ in 0..star_count {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let x = (seed % width.max(1) as u64) as f64;
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let y = (seed % height.max(1) as u64) as f64;
            stars.push(Star { x, y, brightness: 0.0 });
        }
        self.stars = stars;
    }

    /// Trigger a firework launch from the bottom center of the terminal.
    pub fn trigger_firework(&mut self) {
        if !self.vanity_enabled {
            return;
        }
        let width = self.term_width;
        let height = self.term_height;
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

        let start_x = (width / 2) as f64 + ((seed % 30) as f64 - 15.0);
        let start_y = height.saturating_sub(1) as f64;

        // Random target height: between top 10% and 50% of terminal height.
        let min_target = (height as f64 * 0.1).max(2.0);
        let max_target = (height as f64 * 0.5).max(4.0);
        let range = if max_target > min_target { max_target - min_target } else { 1.0 };
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let target_y = min_target + ((seed % 100) as f64 / 100.0) * range;

        let dist = (start_y - target_y).max(1.0);
        
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let vy: f64 = -0.5 - ((seed % 100) as f64 / 100.0) * 0.4; // Launch upwards at 30 FPS speed
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let vx: f64 = ((seed % 100) as f64 - 50.0) / 100.0 * 0.25; // Random launch direction (slight angle)
        
        let max_age = (dist / vy.abs()).round() as u32;

        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.particles.push(Particle {
            x: start_x,
            y: start_y,
            vx,
            vy,
            symbol: "▲",
            age: 0,
            max_age,
            color_idx: (seed % 5) as usize,
            phase: ParticlePhase::Ascent,
        });
    }

    /// Update TUI fireworks particle simulation physics
    pub fn update_particles(&mut self, width: u16, height: u16) {
        if !self.vanity_enabled {
            self.particles.clear();
            self.stars.clear();
            return;
        }

        // Initialize or regenerate stars if window size changed
        if width != self.term_width || height != self.term_height || self.stars.is_empty() {
            self.term_width = width;
            self.term_height = height;
            self.generate_stars();
        }

        // Decay background stars
        for star in &mut self.stars {
            star.brightness = (star.brightness - 0.04).max(0.0);
        }

        let mut next_particles = Vec::new();
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

        for mut p in self.particles.drain(..) {
            p.x += p.vx;
            p.y += p.vy;
            p.age += 1;

            if p.x >= 0.0 && p.x < width as f64 && p.y >= 0.0 && p.y < height as f64 {
                match p.phase {
                    ParticlePhase::Ascent => {
                        if p.age >= p.max_age {
                            // Explode into a burst of sparkles!
                            let symbols = ["✦", "✧", "*", "+", ".", "°", "o"];
                            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                            let count = 14 + (seed % 10) as usize;
                            for i in 0..count {
                                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                                let angle = (i as f64 / count as f64) * 2.0 * std::f64::consts::PI;
                                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                                let speed = 0.15 + ((seed % 100) as f64 / 100.0) * 0.25;
                                let vx = angle.cos() * speed;
                                // Scale y-velocity slightly to accommodate rectangular terminal cell aspect ratio
                                let vy = angle.sin() * speed * 0.45;

                                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                                let symbol_idx = (seed as usize) % symbols.len();
                                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                                let max_age = 12 + (seed % 8) as u32;

                                next_particles.push(Particle {
                                    x: p.x,
                                    y: p.y,
                                    vx,
                                    vy,
                                    symbol: symbols[symbol_idx],
                                    age: 0,
                                    max_age,
                                    color_idx: p.color_idx,
                                    phase: ParticlePhase::Explosion,
                                });
                            }
                        } else {
                            next_particles.push(p);
                        }
                    }
                    ParticlePhase::Explosion => {
                        p.vy += 0.008; // Gravity for 30 FPS
                        
                        // Light up nearby stars
                        for star in &mut self.stars {
                            let dx = p.x - star.x;
                            let dy = p.y - star.y;
                            let dist = (dx * dx + dy * dy).sqrt();
                            if dist < 5.0 {
                                star.brightness = (star.brightness + (5.0 - dist) * 0.15).min(1.0);
                            }
                        }

                        if p.age < p.max_age {
                            next_particles.push(p);
                        }
                    }
                }
            }
        }
        self.particles = next_particles;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GlobalConfig;
    use crate::theme::TuiTheme;
    use ratatui::crossterm::event::KeyCode;

    fn mock_app() -> App {
        let screensavers = vec![
            Screensaver {
                name: "Bubbles".to_string(),
                path: PathBuf::from("C:\\Windows\\System32\\bubbles.scr"),
                #[cfg(feature = "downloader")]
                download_url: None,
            },
            Screensaver {
                name: "Mystify".to_string(),
                path: PathBuf::from("C:\\Windows\\System32\\mystify.scr"),
                #[cfg(feature = "downloader")]
                download_url: None,
            },
            Screensaver {
                name: "Ribbons".to_string(),
                path: PathBuf::from("C:\\Windows\\System32\\ribbons.scr"),
                #[cfg(feature = "downloader")]
                download_url: None,
            },
        ];
        let global = GlobalConfig::default();
        let local = LocalConfig::default();
        let theme = TuiTheme::high_contrast(true);
        App::new(screensavers, global, local, theme)
    }

    #[test]
    fn test_is_uninstall() {
        assert!(is_uninstall(std::path::Path::new(
            "C:\\some\\uninstall.exe"
        )));
        assert!(is_uninstall(std::path::Path::new("UNINSTALL_scr.scr")));
        assert!(!is_uninstall(std::path::Path::new("bubbles.scr")));
    }

    #[test]
    fn test_filtered_indices() {
        let mut app = mock_app();

        // No filter -> all indices
        assert_eq!(app.filtered_indices(), vec![0, 1, 2]);

        // Filter bubbles
        app.filter = "bubble".to_string();
        assert_eq!(app.filtered_indices(), vec![0]);

        // Filter by path substring
        app.filter = "system32".to_string();
        assert_eq!(app.filtered_indices(), vec![0, 1, 2]);

        // Filter no match
        app.filter = "none".to_string();
        assert_eq!(app.filtered_indices(), Vec::<usize>::new());

        // Hide stock screensavers
        app.filter = String::new();
        app.local.hide_stock = true;
        assert_eq!(app.filtered_indices(), Vec::<usize>::new());
    }

    #[test]
    fn test_handle_key_navigation_and_focus() {
        let mut app = mock_app();
        assert_eq!(app.focused, FocusedSection::GlobalPrefs);
        assert_eq!(app.global_field, GlobalField::Active);

        // Move down within GlobalPrefs
        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.global_field, GlobalField::Timeout);

        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.global_field, GlobalField::PreventSleep);

        // Move down to CycleTime
        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.global_field, GlobalField::CycleTime);

        // Move down to HideStock
        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.global_field, GlobalField::HideStock);

        // Move down to VanityMode
        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.global_field, GlobalField::VanityMode);

        // Tab cycles focus to SaverList
        app.handle_key(KeyCode::Tab, KeyModifiers::empty());
        assert_eq!(app.focused, FocusedSection::SaverList);

        // SaverList navigation
        assert_eq!(app.highlighted, 0);
        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.highlighted, 1);
    }

    #[test]
    fn test_selection_and_apply() {
        let _lock = crate::config::TEST_LOCK.lock().unwrap();

        // Create a unique temp dir for the test to avoid collisions
        let temp_dir = std::env::temp_dir().join(format!(
            "wsm_test_app_{}",
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

        let mut app = mock_app();
        assert!(app.local.selected_paths.is_empty());

        // Focus SaverList
        app.focused = FocusedSection::SaverList;

        // Toggle selection on the first item (Bubbles)
        app.handle_key(KeyCode::Char(' '), KeyModifiers::empty());
        assert_eq!(app.local.selected_paths.len(), 1);
        assert_eq!(app.local.selected_paths[0], "C:\\Windows\\System32\\bubbles.scr");

        // Toggle selection on the second item (Mystify)
        app.highlighted = 1;
        app.handle_key(KeyCode::Char(' '), KeyModifiers::empty());
        assert_eq!(app.local.selected_paths.len(), 2);
        assert_eq!(app.local.selected_paths[1], "C:\\Windows\\System32\\mystify.scr");

        // Hitting Enter on the list applies the multi-selection.
        // It should set registry/global config active_scr to the path of wsm.exe itself.
        app.handle_key(KeyCode::Enter, KeyModifiers::empty());
        let exe = std::env::current_exe().unwrap_or_default();
        assert_eq!(app.global.active_scr, exe.to_string_lossy().into_owned());

        // Toggle second item again (uncheck Mystify)
        app.handle_key(KeyCode::Char(' '), KeyModifiers::empty());
        assert_eq!(app.local.selected_paths.len(), 1);

        // Apply again, should set global config active_scr to Bubbles directly
        app.handle_key(KeyCode::Enter, KeyModifiers::empty());
        assert_eq!(app.global.active_scr, "C:\\Windows\\System32\\bubbles.scr");

        // Clean up temp dir
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg(feature = "downloader")]
    fn test_registry_merge_and_automated_downloader() {
        let mut app = mock_app();
        app.focused = FocusedSection::SaverList;

        // Verify initial list has 3 items (Bubbles, Mystify, Ribbons)
        assert_eq!(app.screensavers.len(), 3);

        // Manually merge registry entries
        let entries = vec![
            crate::downloader::RegistryEntry {
                name: "Win-beams".to_string(),
                author: "UberMetroid".to_string(),
                description: "Beams screensaver".to_string(),
                download_url: "https://example.com/win-beams.scr".to_string(),
                version: "1.0".to_string(),
            },
        ];
        app.merge_registry_entries(entries);

        // Verify list now has 4 items (alphabetically ordered: Bubbles, Mystify, Ribbons, Win-beams)
        assert_eq!(app.screensavers.len(), 4);
        assert_eq!(app.screensavers[3].name, "Win-beams");
        assert_eq!(app.screensavers[3].download_url.as_deref(), Some("https://example.com/win-beams.scr"));

        // Highlight Win-beams (which is index 3)
        app.highlighted = 0;
        app.move_highlight(1); // moves to Mystify (index 1)
        app.move_highlight(1); // moves to Ribbons (index 2)
        app.move_highlight(1); // moves to Win-beams (index 3)
        assert_eq!(app.highlighted, 3);

        // Pressing space (toggle selection) should trigger background download of Win-beams
        app.toggle_highlighted_selection();
        assert!(app.download_state.is_some());
        assert_eq!(app.pending_action, Some(PendingAction::ToggleSelection));
    }

    #[test]
    fn test_console_modes() {
        println!("TESTING RAW MODES:");
        match ratatui::crossterm::terminal::enable_raw_mode() {
            Ok(_) => println!("  enable_raw_mode: OK"),
            Err(e) => println!("  enable_raw_mode: ERROR: {}", e),
        }
        match ratatui::crossterm::terminal::disable_raw_mode() {
            Ok(_) => println!("  disable_raw_mode: OK"),
            Err(e) => println!("  disable_raw_mode: ERROR: {}", e),
        }
    }
}
