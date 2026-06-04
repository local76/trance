# Screen Saver Manager (SSM)

A lightweight, modern Windows Screen Saver Management TUI dashboard built in Rust. It enables discovering, previewing, and configuring screensavers on mixed-DPI multi-monitor environments.

```
+================================================================+
| SCREEN SAVER MANAGEMENT  (SSM)                                 |
+----------------------------------------------------------------+
|  Global System Preferences                                     |
|  ▶ Active:         ACTIVE                                      |
|    Timeout:        10 minutes                                  |
|    Prevent sleep:  DISABLED (NORMAL)                           |
|    Cycle duration: 30 seconds                                  |
|    Applied:        mystify.scr                                 |
+================================================================+
```

---

## Key Features
* **Modern TUI Dashboard**: Real-time console interface utilizing [ratatui](https://crates.io/crates/ratatui) and [crossterm](https://crates.io/crates/crossterm).
* **Automatic Discovery**: Scans Windows system folders (`System32`, `SysWOW64`, etc.) and a dedicated folder in `%APPDATA%` for `.scr` executables.
* **OS-Level Configuration Sync**: Integrates directly with Windows Registry keys (`HKCU\Control Panel\Desktop`) and calls `SystemParametersInfoW` to propagate settings instantly.
* **Mini-Preview Support**: Transparently forwards `/p <HWND>` commands to child screensaver processes so they display in the Windows settings dialog.
* **Sleep Inhibition**: Temporarily keeps the display/system awake with `SetThreadExecutionState` when activated in preferences.
* **Color Harmonization**: Automatically query Windows high-contrast state, active accent colors, dark mode, and console palettes to dynamically style the TUI to match your OS context.

---

## Subcommands & CLI

SSM acts as both a dashboard and a screensaver command-line handler.

```bash
ssm.exe [OPTIONS] [COMMAND]

Options:
  --theme <THEME>  Force TUI theme: dark, light, high-contrast, no-color
```

### Commands:
* `tui` or `configure` (or no command): Launch the interactive TUI configuration manager (default).
* `run` or `start` or `/s`: Launch the currently active screensaver fullscreen.
* `stop`: Kill all running screensavers discovered on the system.
* `toggle-active`: Toggle whether the screen saver is enabled system-wide.
* `lock`: Lock the Windows workstation first, then immediately launch the active screensaver.
* `preview <HWND>` or `/p:<HWND>`: Render a preview of the active screensaver inside a specific host window (used by Windows Screen Saver Settings).
* `doctor`: Run diagnostic report checking registry readability, file paths, logs, and directory structures.

---

## TUI Keybindings

Navigate and configure your preferences dynamically using the keyboard:

| Key | Action |
| :--- | :--- |
| **`Tab`** / **`BackTab`** | Cycle focus between **Global System Preferences** and **Screen Savers List** |
| **`↑ / ↓`** | Navigate fields in preferences or entries in the screensaver list |
| **`← / →`** | Adjust Screensaver Timeout or Random Cycle Duration |
| **`Space / Enter`** | Toggle preferences (Active state, Prevent sleep) or apply the highlighted screensaver |
| **`/`** | Open filter search input (type to filter screensavers, press `Esc` to clear) |
| **`F5`** | Re-scan the system and `%APPDATA%` directories for new screensavers |
| **`P`** | Launch a full-screen preview of the highlighted screensaver |
| **`q / Esc`** | Quit SSM |

---

## File & Configuration Paths

* **System Preferences**: Read and written to standard registry values under `HKCU\Control Panel\Desktop` (`SCRNSAVE.EXE`, `ScreenSaveActive`, `ScreenSaveTimeOut`).
* **SSM Custom Preferences**: Stored at `%APPDATA%\SSM\config.yaml` (contains last-selected screensaver, prevent-sleep status, and random cycle duration).
* **Screensaver Drop Path**: Put custom `.scr` screensavers in `%APPDATA%\SSM\screensavers` to have SSM discover them.
* **Logs File**: Diagnostics are written to `%APPDATA%\SSM\ssm.log` so they do not clutter raw terminal outputs.

---

## Environment Variables

* **`NO_COLOR`**: Set `NO_COLOR=1` to disable styling colors and fall back to monochromatic black & white.
* **`RUST_LOG`**: Set `RUST_LOG=debug` or `RUST_LOG=trace` to adjust logging verbosity in `ssm.log`.

---

## Build Guide

Ensure you have Rust and Cargo installed.

```bash
# Clone the repository
git clone <repository-url>
cd ssm

# Build debug binary
cargo build

# Build optimized release binary
cargo build --release
```

The optimized binary will be compiled to `target/release/ssm.exe`. You can rename this to `ssm.scr` to install it directly as a Windows screensaver!
