# Changelog

All notable changes to this project will be documented in this file.

## [2026.6.9] - 2026-06-09

### Renamed
- **Project rename**: `trance` was previously `trance-tui` / `rIdle`. The Cargo package name, binary name, file paths, registry keys, AppData paths, and docs are now lowercase `trance`. Behavior and features are unchanged.

### Refactored
- **TUI Blueprint alignment**: Re-architected directory and module tree to standard TUI layout. Renamed `src/ui/panels.rs` to `src/ui/widgets.rs`. Created `src/backend/` directory, moving `src/runner.rs` to `src/backend/mod.rs`, `src/preview.rs` to `src/backend/preview.rs`, the Windows and mock screensaver modules to `src/backend/saver/`, and the downloader modules to `src/backend/downloader/`.

### Changed
- README rewritten in the new register: screensaver picker feature list, install matrix, CLI flags, configuration, build instructions, license.
- Drop the legacy "r*" and "Local freedom" branding throughout.
- Drop the per-repo `rApps` umbrella and `build_all.ps1` from this repo; build orchestration lives in [`toolkit`](https://github.com/local76/toolkit).
- The `registry.json` entries now reference lowercase scene names (`glyphs`, `flame`, `cosmos`, etc.) and the new GitHub release URLs for [`screensavers`](https://github.com/local76/screensavers).

## [2.6.6] - 2026-06-08

### Refactored
- Refactored monolithic `src/app.rs` into modular sub-files under `src/app/` (`mod.rs`, `actions.rs`, `keys.rs`, `cycle.rs`), keeping all source files under 500 lines.
- Refactored monolithic `src/ui.rs` into modular sub-files under `src/ui/` (`mod.rs`, `panels.rs`, `utils.rs`), keeping all source files under 500 lines.
- Extracted Win32 API declarations from `src/main.rs` into `src/win32.rs`.
- Resolved all compiler and Clippy warnings across the codebase.
- **Renamed project** from `trance` to `trance-tui`. The GitHub repository, Cargo package name, binary name, and all user-facing labels now use the `-tui` suffix to make the program's role as a terminal user interface explicit (matching `template-tui`).
  - Repository: `local76/trance` → `local76/trance-tui`
  - Crate/binary: `trance` → `trance-tui`
  - Config file: `%APPDATA%\trance\config.yaml` → `%APPDATA%\trance-tui\config.yaml`
  - Downloader cache: `%APPDATA%\trance\screensavers\` → `%APPDATA%\trance-tui\screensavers\`
  - Linux package names: `trance` → `trance-tui`

## [2.6.4] - 2026-06-06
### Changed
- Reorganized repository file layout to align with ARCHITECTURE.md.
- TUI header title updated to "trance - Screensaver Manager".
- Disabled borderless console mode.
- Embedded app icon into Windows installer package.

## [3.0.1] - 2026-06-06
### Added
- Added author and maintainer metadata for packaging.

## [3.0.0] - 2026-06-06
### Changed
- Renamed organization to `local76`.
- Renamed executable from `rtem` to `trance`.
- Reorganized directory structure to group packaging files inside `dist/packages/`.