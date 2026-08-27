# Changelog

All notable changes to **Cardea** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [1.0.3] - 2026-08-27

### Added
- **Copy File Path Context Menu Action**: Added a dedicated "Copy file path" action to the right-click and keyboard context menu (`m`) allowing users to copy the absolute filesystem path of selected files/folders directly to the system clipboard (Wayland `wl-copy`, X11 `xclip`/`xsel`, macOS `pbcopy`, Windows `clip.exe`, and terminal OSC 52 sequence).
- **Crate Assets Inclusion**: Added `assets/**/*` to `Cargo.toml` package inclusions.

## [1.0.0] - 2026-08-27

### Initial Release
- **Cardea**: Renamed from *Fenestra* (named after the Roman goddess of thresholds, hinges, and doorways).
- **Desktop-Style Shell**: Hierarchical collapsible tree sidebar, sortable details table with virtual scrolling for 100k+ files, and clickable breadcrumb chips with sibling directory dropdown popovers.
- **Hybrid Input**: Seamless mouse ergonomics (click, double-click, drag & drop, column resize) paired with complete Vim/arrow keyboard controls.
- **Multimodal File Previews**:
  - Syntax highlighting via `syntect` for code and formatted documents.
  - Formatted Markdown renderer (`src/ui/markdown.rs`).
  - Terminal image previews (Kitty graphics, Sixel, iTerm2, and unicode halfblock fallback via `ratatui-image`).
  - Read-only archive content listings (`.zip`, `.tar`, `.tar.gz`, `.tar.xz`, `.7z`).
  - Built-in `xxd`-style binary hex dump viewer.
  - Off-thread MIME guessing and SHA-256 digest computation.
- **Safe Async Operations**:
  - Non-blocking batch copy, move, cut, and paste on dedicated Tokio worker threads.
  - Safe deletion default via Freedesktop System Trash (`trash-rs`) with confirmed permanent delete (`Shift+Delete`).
  - Conflict resolution prompts (Overwrite, Skip, Auto-Rename) and background job queue (`Ctrl+J`) with cooperative cancellation.
- **Multi-Tab & Dual-Pane**:
  - Multi-tab management (`Ctrl+T`, `Ctrl+W`, `Ctrl+Tab`, `Alt+1..9`) with independent directory and navigation state.
  - Split Commander dual-pane mode (`F3`) with cross-pane transfer shortcuts (`F5`, `F6`).
- **Dynamic Theming**:
  - Live inotify hot-reload watching Omarchy system theme (`~/.config/omarchy/current/theme`).
  - Curated built-in palettes: Catppuccin Mocha/Latte, Tokyo Night, Gruvbox Dark, Nord, Solarized, and ANSI fallback.
  - Support for `$NO_COLOR`, automatic RGB degradation to 16 ANSI colors on non-truecolor terminals, and custom TOML hex color overrides.
- **CLI & Discovery**:
  - Interactive keybinding cheat sheet (`?` / `F1`) and modifier discovery popup on incomplete chords.
  - Shell completion generation (`--generate-completions [bash|zsh|fish|elvish|powershell]`).
  - Automated man page generation (`--generate-manpage`).
