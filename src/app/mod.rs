//! Application state, focus, and key bindings.
//!
//! **Taxonomy Classification**: Interface (TUI / State Coordination).

use crate::config::{GlobalConfig, LocalConfig};
use crate::backend::preview::Screensaver;
use crate::theme::TuiTheme;

#[cfg(feature = "downloader")]
use crate::backend::downloader;
use crate::backend::preview;

pub mod actions;
pub mod cycle;
pub mod keys;

pub use ratatui::crossterm::event::{KeyCode, KeyModifiers};
pub use cycle::run_random_cycle;

const README_CONTENT: &str = include_str!("../../README.md");
const SUPPORT_CONTENT: &str = include_str!("../../SUPPORT.md");
const LICENSE_CONTENT: &str = include_str!("../../LICENSE.md");
const COPYRIGHT_CONTENT: &str = include_str!("../../COPYRIGHT.md");
const PRIVACY_CONTENT: &str = include_str!("../../PRIVACY.md");
const SECURITY_CONTENT: &str = include_str!("../../SECURITY.md");
const CONTRIBUTING_CONTENT: &str = include_str!("../../CONTRIBUTING.md");

/// Focused section in the TUI dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedSection {
    /// Global preferences config pane.
    GlobalPrefs,
    /// Screensaver list selection.
    SaverList,
}

/// Dynamic global config fields in the TUI dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalField {
    /// Active screensaver enabled/disabled state.
    Active,
    /// Timeout length of the screensaver.
    Timeout,
    /// Prevent system sleep state.
    PreventSleep,
    /// Cycling interval duration.
    CycleTime,
    /// Hide stock Windows screensavers.
    HideStock,
}

impl GlobalField {
    /// Helper to cycle focus across preferences.
    pub const ALL: &'static [GlobalField] = &[
        GlobalField::Active,
        GlobalField::Timeout,
        GlobalField::PreventSleep,
        GlobalField::CycleTime,
        GlobalField::HideStock,
    ];
}

/// Status message type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    /// Normal information status.
    Info,
    /// Error status.
    Error,
}

/// Screen saver download pending action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "downloader")]
pub enum PendingAction {
    /// Apply active screensaver.
    Apply,
    /// Toggle cycling list selection.
    ToggleSelection,
    /// Run screensaver fullscreen preview.
    Preview,
    /// Open screensaver configuration.
    Configure,
    /// Toggle screensaver selection and apply immediately.
    ToggleAndApply,
}

/// Status message displayed on the TUI status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusMessage {
    /// Status message text.
    pub text: String,
    /// Semantic type of the status.
    pub kind: StatusKind,
}

/// Main application state struct.
pub struct App {
    /// Discovered/registered screensavers.
    pub screensavers: Vec<Screensaver>,
    /// Highlighted list index.
    pub highlighted: usize,
    /// Current focused panel.
    pub focused: FocusedSection,
    /// Current focused preference field.
    pub global_field: GlobalField,
    /// Global screensaver registry config.
    pub global: GlobalConfig,
    /// Local user config.
    pub local: LocalConfig,
    /// TUI theme colors.
    pub theme: TuiTheme,
    /// Status message state.
    pub status: Option<StatusMessage>,
    /// Quit flag signaling render loop exit.
    pub should_quit: bool,
    /// List display offset.
    pub list_offset: usize,
    /// Cached list items for rendering the screensavers list.
    pub list_items: Vec<ratatui::widgets::ListItem<'static>>,
    /// Visual progress bar interpolation value.
    pub visual_progress: f64,
    /// Active download worker state.
    #[cfg(feature = "downloader")]
    pub download_state: Option<std::sync::Arc<std::sync::Mutex<downloader::DownloadState>>>,
    /// Registry feed worker fetch results.
    #[cfg(feature = "downloader")]
    pub registry_results: Option<std::sync::Arc<std::sync::Mutex<Option<Vec<downloader::RegistryEntry>>>>>,
    /// Curated registry catalog items.
    #[cfg(feature = "downloader")]
    pub registry_entries: Vec<downloader::RegistryEntry>,
    /// Action to execute once download succeeds.
    #[cfg(feature = "downloader")]
    pub pending_action: Option<PendingAction>,
    /// Help overlay visibility.
    pub show_help: bool,
    /// Selection column/row start bounds.
    pub selection_start: Option<(u16, u16)>,
    /// Selection column/row end bounds.
    pub selection_end: Option<(u16, u16)>,
    /// Selection copy-to-clipboard trigger.
    pub selection_pending_copy: bool,
    /// Opened markdown document name.
    pub show_markdown: Option<String>,
    /// Rendered lines of the markdown document.
    pub markdown_lines: Vec<ratatui::text::Line<'static>>,
    /// Scroll offset of the markdown document.
    pub markdown_scroll: usize,
    /// Loaded terminal character fallbacks (Adaptive Emoji/Glyph fallback)
    pub glyphs: crate::win32::GlyphMap,
    /// Whether the computer is currently running on battery power (Throttling)
    pub on_battery: bool,
    /// Last Instant the power/battery status was queried
    pub last_power_check: std::time::Instant,
    /// Shutdown button screen bounds.
    pub quit_btn_bounds: Option<(u16, u16, u16)>,
    /// Help button screen bounds.
    pub help_btn_bounds: Option<(u16, u16, u16)>,
    /// Custom TUI window dragging state.
    pub drag_active: bool,
    /// Cursor coordinates on drag start.
    pub drag_start_cursor: Option<(i32, i32)>,
    /// Console window coordinates on drag start.
    pub drag_start_window: Option<(i32, i32)>,
    pub username: String,
    pub hostname: String,
    pub os_version: String,
}

impl App {
    /// Create a new App state.
    pub fn new(
        screensavers: Vec<Screensaver>,
        global: GlobalConfig,
        local: LocalConfig,
        theme: TuiTheme,
    ) -> Self {
        let mut local = local;
        if local.hide_stock {
            let orig_len = local.selected_paths.len();
            local.selected_paths.retain(|p| {
                !preview::is_stock_screensaver(std::path::Path::new(p))
            });
            if local.selected_paths.len() != orig_len {
                let _ = local.save();
            }
        }

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

        #[cfg(feature = "downloader")]
        let registry_results = {
            let state = std::sync::Arc::new(std::sync::Mutex::new(None));
            let thread_state = state.clone();
            let feed_urls = local.feed_urls.clone();
            if !library::lifecycle::foreground::tui_bootstrap::is_app_shutting_down() {
                std::thread::spawn(move || {
                    if library::lifecycle::foreground::tui_bootstrap::is_app_shutting_down() {
                        return;
                    }
                    let mut all_entries = Vec::new();
                    if let Ok(local_entries) = downloader::load_local_registry() {
                        for entry in local_entries {
                            if library::lifecycle::foreground::tui_bootstrap::is_app_shutting_down() {
                                return;
                            }
                            if !all_entries.iter().any(|e: &downloader::RegistryEntry| e.name.eq_ignore_ascii_case(&entry.name)) {
                                all_entries.push(entry);
                            }
                        }
                    }
                    for url in feed_urls {
                        if library::lifecycle::foreground::tui_bootstrap::is_app_shutting_down() {
                            return;
                        }
                        if let Ok(entries) = downloader::fetch_registry(&url) {
                            for entry in entries {
                                if library::lifecycle::foreground::tui_bootstrap::is_app_shutting_down() {
                                    return;
                                }
                                if !all_entries.iter().any(|e: &downloader::RegistryEntry| e.name.eq_ignore_ascii_case(&entry.name)) {
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
            }
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
            list_offset: 0,
            list_items: Vec::new(),
            visual_progress: 0.0,
            #[cfg(feature = "downloader")]
            download_state: None,
            #[cfg(feature = "downloader")]
            registry_results,
            #[cfg(feature = "downloader")]
            registry_entries: Vec::new(),
            #[cfg(feature = "downloader")]
            pending_action: None,
            selection_start: None,
            selection_end: None,
            selection_pending_copy: false,
            show_help: false,
            show_markdown: None,
            markdown_lines: Vec::new(),
            markdown_scroll: 0,
            glyphs: crate::win32::GlyphMap::load(),
            on_battery: !crate::win32::query_power_status().ac_online,
            last_power_check: std::time::Instant::now(),
            quit_btn_bounds: None,
            help_btn_bounds: None,
            drag_active: false,
            drag_start_cursor: None,
            drag_start_window: None,
            username: std::env::var("USERNAME").unwrap_or_else(|_| std::env::var("USER").unwrap_or_else(|_| "user".to_string())),
            hostname: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "localhost".to_string()),
            os_version: crate::win32::query_os_version(),
        };
        app.update_list_items();
        app
    }

    /// Indices into `self.screensavers` that match the current filter.
    pub fn filtered_indices(&self) -> Vec<usize> {
        let indices: Vec<usize> = (0..self.screensavers.len()).collect();
        if self.local.hide_stock {
            indices
                .into_iter()
                .filter(|&i| !preview::is_stock_screensaver(&self.screensavers[i].path))
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
        if let Some(pos) = indices.iter().position(|&i| i == self.highlighted) {
            self.highlighted = indices[pos];
        } else {
            self.highlighted = indices[0];
        }
    }

    /// Update the cached ListItem widgets in `self.list_items`.
    pub fn update_list_items(&mut self) {
        let theme = self.theme;
        self.list_items = self
            .screensavers
            .iter()
            .map(|s| {
                let is_checked = self.local.selected_paths.contains(&s.path.to_string_lossy().into_owned());
                let exists = s.path.exists();
                let is_online = s.download_url.is_some() && !exists;
                let is_stock = preview::is_stock_screensaver(&s.path);

                let active_str = if is_checked { "yes" } else { "no" };
                let active_color = if is_checked { theme.applied } else { theme.text_dim };

                let name = crate::ui::truncate(&s.name, 28);
                let name_str = format!("{:<30}  ", name);
                let name_color = if is_online {
                    theme.accent_primary
                } else if is_checked {
                    theme.text_main
                } else {
                    theme.text_dim
                };

                let type_str = if is_stock {
                    "Stock"
                } else {
                    "Custom"
                };
                let type_color = if is_stock {
                    theme.text_dim
                } else {
                    theme.accent_secondary
                };

                let spans = vec![
                    ratatui::text::Span::styled(
                        format!("{:<8}  ", active_str),
                        ratatui::style::Style::default().fg(active_color),
                    ),
                    ratatui::text::Span::styled(
                        name_str,
                        ratatui::style::Style::default().fg(name_color),
                    ),
                    ratatui::text::Span::styled(
                        type_str.to_string(),
                        ratatui::style::Style::default().fg(type_color),
                    ),
                ];
                ratatui::widgets::ListItem::new(ratatui::text::Line::from(spans))
            })
            .collect();
    }

    /// Return the currently highlighted screensaver object.
    pub fn current_screensaver(&self) -> Option<&Screensaver> {
        self.screensavers.get(self.highlighted)
    }

    /// Load and open an embedded markdown document in the viewer modal.
    pub fn open_embedded_markdown(&mut self, title: &str, content: &str) {
        self.markdown_lines = cycle::parse_markdown_to_lines(content, &self.theme);
        self.show_markdown = Some(title.to_string());
        self.markdown_scroll = 0;
        self.status = Some(StatusMessage {
            text: format!("Opened document: {}", title),
            kind: StatusKind::Info,
        });
    }

    /// Checks system power status periodically and adjusts throttling state.
    pub fn sync_power_status_if_needed(&mut self) {
        if self.last_power_check.elapsed() > std::time::Duration::from_millis(5000) {
            self.last_power_check = std::time::Instant::now();
            let power = crate::win32::query_power_status();
            let current_on_battery = !power.ac_online;
            if current_on_battery != self.on_battery {
                self.on_battery = current_on_battery;
                let state = if current_on_battery {
                    "Battery (Power-Saving Throttling Enabled)"
                } else {
                    "AC Power (Full Speed)"
                };
                tracing::info!("Power source changed. Status: {}", state);
                self.status = Some(StatusMessage {
                    text: format!("Power Source Changed: {}", state),
                    kind: StatusKind::Info,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
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
        assert!(cycle::is_uninstall(std::path::Path::new(
            "C:\\some\\uninstall.exe"
        )));
        assert!(cycle::is_uninstall(std::path::Path::new("UNINSTALL_scr.scr")));
        assert!(!cycle::is_uninstall(std::path::Path::new("bubbles.scr")));
    }

    #[test]
    fn test_filtered_indices() {
        let mut app = mock_app();
        assert_eq!(app.filtered_indices(), vec![0, 1, 2]);
        app.local.hide_stock = true;
        assert_eq!(app.filtered_indices(), Vec::<usize>::new());
    }

    #[test]
    fn test_handle_key_navigation_and_focus() {
        let mut app = mock_app();
        assert_eq!(app.focused, FocusedSection::GlobalPrefs);
        assert_eq!(app.global_field, GlobalField::Active);

        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.global_field, GlobalField::Timeout);

        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.global_field, GlobalField::PreventSleep);

        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.global_field, GlobalField::CycleTime);

        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.global_field, GlobalField::HideStock);

        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.global_field, GlobalField::Active);

        app.handle_key(KeyCode::Tab, KeyModifiers::empty());
        assert_eq!(app.focused, FocusedSection::SaverList);

        assert_eq!(app.highlighted, 0);
        app.handle_key(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(app.highlighted, 1);
    }

    #[test]
    fn test_selection_and_apply() {
        let _lock = crate::config::TEST_LOCK.lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "trance_test_app_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        unsafe {
            std::env::set_var("APPDATA", &temp_dir);
        }

        let mut app = mock_app();
        assert!(app.local.selected_paths.is_empty());

        app.focused = FocusedSection::SaverList;

        app.handle_key(KeyCode::Char(' '), KeyModifiers::empty());
        assert_eq!(app.local.selected_paths.len(), 1);
        assert_eq!(app.local.selected_paths[0], "C:\\Windows\\System32\\bubbles.scr");

        app.highlighted = 1;
        app.handle_key(KeyCode::Char(' '), KeyModifiers::empty());
        assert_eq!(app.local.selected_paths.len(), 2);
        assert_eq!(app.local.selected_paths[1], "C:\\Windows\\System32\\mystify.scr");

        let exe = std::env::current_exe().unwrap_or_default();
        assert_eq!(app.global.active_scr, exe.to_string_lossy().into_owned());

        app.handle_key(KeyCode::Enter, KeyModifiers::empty());
        assert_eq!(app.local.selected_paths.len(), 1);
        assert_eq!(app.global.active_scr, "C:\\Windows\\System32\\bubbles.scr");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg(feature = "downloader")]
    fn test_registry_merge_and_automated_downloader() {
        let _lock = crate::config::TEST_LOCK.lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "trance_test_app_downloader_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        unsafe {
            std::env::set_var("APPDATA", &temp_dir);
        }

        let mut app = mock_app();
        app.focused = FocusedSection::SaverList;
        assert_eq!(app.screensavers.len(), 3);

        let entries = vec![
            downloader::RegistryEntry {
                name: "beams".to_string(),
                author: "UberMetroid".to_string(),
                description: "Beams screensaver".to_string(),
                download_url: Some("https://example.com/beams.scr".to_string()),
                downloads: None,
                version: "2.0".to_string(),
            },
        ];
        app.merge_registry_entries(entries.clone());

        assert_eq!(app.screensavers.len(), 4);
        assert_eq!(app.screensavers[0].name, "beams");
        assert_eq!(app.screensavers[0].download_url.as_deref(), Some("https://example.com/beams.scr"));

        app.highlighted = 0;
        assert_eq!(app.highlighted, 0);

        app.toggle_highlighted_selection();
        assert!(app.download_state.is_some());
        assert_eq!(app.pending_action, Some(PendingAction::ToggleSelection));

        let _ = std::fs::remove_dir_all(&temp_dir);
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
