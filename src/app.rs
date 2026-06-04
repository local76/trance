//! Application state, focus, and key bindings.
//!
//! # Model-Render Split
//! SSM uses a strict Model-Render architectural split:
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
}

impl GlobalField {
    pub const ALL: [GlobalField; 3] = [
        GlobalField::Active,
        GlobalField::Timeout,
        GlobalField::PreventSleep,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Error,
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
        };
        app.update_list_items();
        app
    }

    /// Indices into `self.screensavers` that match the current filter.
    /// Empty filter → all indices, in order.
    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.filter.is_empty() {
            return (0..self.screensavers.len()).collect();
        }
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

        self.list_items = self
            .screensavers
            .iter()
            .map(|s| {
                let is_applied = s
                    .path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .map(str::to_lowercase)
                    .as_ref()
                    == active_filename.as_ref();
                let exists = s.path.exists();
                let mut spans = vec![ratatui::text::Span::styled(
                    format!("{:<22}", crate::ui::truncate(&s.name, 22)),
                    ratatui::style::Style::default().fg(if is_applied {
                        theme.text_main
                    } else {
                        theme.text_dim
                    }),
                )];
                if is_applied {
                    spans.push(ratatui::text::Span::styled(
                        " [Applied]",
                        ratatui::style::Style::default().fg(theme.applied),
                    ));
                } else if !exists {
                    spans.push(ratatui::text::Span::styled(
                        " [Missing]",
                        ratatui::style::Style::default().fg(theme.missing),
                    ));
                }
                ratatui::widgets::ListItem::new(ratatui::text::Line::from(spans))
            })
            .collect();
    }

    pub fn current_screensaver(&self) -> Option<&Screensaver> {
        self.screensavers.get(self.highlighted)
    }

    /// Apply the currently-highlighted screensaver as the system screensaver.
    pub fn apply_highlighted(&mut self) {
        let Some(s) = self.screensavers.get(self.highlighted) else {
            self.status = Some(StatusMessage {
                text: "No screensaver selected.".into(),
                kind: StatusKind::Error,
            });
            return;
        };
        self.global.active_scr = s.path.to_string_lossy().into_owned();
        if let Err(e) = self.global.save() {
            self.status = Some(StatusMessage {
                text: format!("Failed to save: {e}"),
                kind: StatusKind::Error,
            });
            return;
        }
        if let Some(name) = s.path.file_name().and_then(|f| f.to_str()) {
            self.local.last_selected = Some(name.to_string());
            let _ = self.local.save();
        }
        self.status = Some(StatusMessage {
            text: format!("Applied: {}", s.name),
            kind: StatusKind::Info,
        });
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

    /// Re-discover screensavers and refresh the list.
    pub fn refresh_screensavers(&mut self) {
        let mut list = Vec::new();
        if let Some(s) = random_cycle_entry() {
            list.push(s);
        }
        list.extend(crate::preview::discover());
        self.screensavers = list;
        self.resolve_highlight();
        self.status = Some(StatusMessage {
            text: "Refreshed screensavers list.".to_string(),
            kind: StatusKind::Info,
        });
        self.update_list_items();
    }

    /// Spawn the currently-highlighted screensaver fullscreen.
    pub fn preview_highlighted(&mut self) {
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
        let Some(s) = self.current_screensaver() else {
            return;
        };
        let exe = std::env::current_exe().unwrap_or_default();
        if s.path == exe {
            self.status = Some(StatusMessage {
                text: "Random Cycle has no native configuration dialog.".to_string(),
                kind: StatusKind::Info,
            });
            return;
        }
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
                KeyCode::Enter | KeyCode::Char(' ') => {
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
            KeyCode::F(5) => self.refresh_screensavers(),
            KeyCode::Tab => self.cycle_focus(),
            KeyCode::BackTab => self.cycle_focus(),
            KeyCode::Up => self.move_focus(-1),
            KeyCode::Down => self.move_focus(1),
            KeyCode::Left => self.on_left(),
            KeyCode::Right => self.on_right(),
            KeyCode::Char(' ') | KeyCode::Enter => self.on_activate(),
            KeyCode::Char('p') | KeyCode::Char('P') | KeyCode::Char('t') | KeyCode::Char('T') => {
                self.preview_highlighted()
            }
            KeyCode::Char('c') | KeyCode::Char('C') => self.configure_highlighted(),
            _ => {}
        }
        self.should_quit
    }

    fn on_left(&mut self) {
        if self.focused == FocusedSection::GlobalPrefs && self.global_field == GlobalField::Timeout {
            self.adjust_timeout(-1);
        }
    }

    fn on_right(&mut self) {
        if self.focused == FocusedSection::GlobalPrefs && self.global_field == GlobalField::Timeout {
            self.adjust_timeout(1);
        }
    }

    fn on_activate(&mut self) {
        match self.focused {
            FocusedSection::GlobalPrefs => match self.global_field {
                GlobalField::Active => self.toggle_active(),
                GlobalField::PreventSleep => self.toggle_prevent_sleep(),
                GlobalField::Timeout => {}
            },
            FocusedSection::SaverList => self.apply_highlighted(),
        }
    }
}

pub use ratatui::crossterm::event::{KeyCode, KeyModifiers};

/// Build a "Random Cycle" screensaver entry pointing at the current
/// executable.
pub fn random_cycle_entry() -> Option<Screensaver> {
    let path = std::env::current_exe().ok()?;
    let name = "Random Cycle".to_string();
    Some(Screensaver { name, path })
}

/// Convenience: kick off the random cycle and return when it finishes.
pub fn run_random_cycle() {
    let discovered = crate::preview::discover();
    let exe = std::env::current_exe().ok();

    let candidates: Vec<PathBuf> = discovered
        .into_iter()
        .map(|s| s.path)
        .filter(|p| !is_self(p, exe.as_ref()) && !is_uninstall(p))
        .collect();

    if candidates.is_empty() {
        return;
    }

    let local_config = LocalConfig::load();
    let cycle_duration = std::time::Duration::from_secs(local_config.random_cycle_secs as u64);

    let mut seed: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

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
            },
            Screensaver {
                name: "Mystify".to_string(),
                path: PathBuf::from("C:\\Windows\\System32\\mystify.scr"),
            },
            Screensaver {
                name: "Ribbons".to_string(),
                path: PathBuf::from("C:\\Windows\\System32\\ribbons.scr"),
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

        // Tab cycles focus to SaverList
        app.handle_key(KeyCode::Tab, KeyModifiers::empty());
        assert_eq!(app.focused, FocusedSection::SaverList);

        // SaverList navigation
        assert_eq!(app.highlighted, 0);
        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.highlighted, 1);
    }
}
