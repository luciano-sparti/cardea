# Contributing to Cardea

Thank you for your interest in contributing to **Cardea**! Whether you are reporting a bug, improving documentation, submitting feature ideas, or writing code, your contributions are welcome.

---

## 🛠️ Development Setup

### Prerequisites

- **Linux** (x86_64 or aarch64)
- **Rust 1.78+** (with `cargo`, `rustfmt`, and `clippy`)
- A terminal emulator with truecolor & mouse support (e.g. *Kitty*, *Alacritty*, *WezTerm*, *Ghostty*, *Foot*)

```bash
# Clone the repository
git clone https://github.com/Luciano-Sparti/cardea.git
cd cardea

# Build debug binary
cargo build

# Run Cardea locally
cargo run -- .
```

---

## 🧪 Testing & Verification

Before opening a pull request, ensure your changes pass all local checks:

```bash
# 1. Check code formatting
cargo fmt --check

# 2. Run Clippy linter with strict warnings
cargo clippy --all-targets -- -D warnings

# 3. Run the full automated test suite (unit, integration, property, headless UI)
cargo test --all-targets

# 4. Check benchmark compilation
cargo check --benches
```

### Headless UI Snapshot Tests

Cardea uses `ratatui::backend::TestBackend` and `insta` for deterministic headless UI snapshot testing without needing an active display server or X11/Wayland session. If you change UI rendering and intentionally update snapshots, review them carefully with `cargo insta review`.

---

## 🏛️ Code Architecture & Design

1. **Unidirectional Event-Action-State Flow**:
   - `src/event.rs`: Event loop receiving crossterm inputs, timer ticks (50ms), and background worker channels.
   - `src/app.rs`: Central application state machine, tabs, focus routing, selection state, and dialog stack.
   - `src/ui/`: Pure Ratatui renderers (sidebar tree, virtual details table, breadcrumbs, multimodal preview drawer, dialog modals).
2. **Non-Blocking Background I/O**:
   - Heavy operations (directory scanning, recursive search, file operations, syntect preview parsing) execute off-thread on Tokio blocking threads (`tokio::task::spawn_blocking`). Never perform synchronous filesystem I/O on the main UI thread.
3. **Safety First**:
   - Deletions route through Freedesktop System Trash (`trash-rs`) by default. Permanent deletion requires explicit user confirmation.
   - Custom actions and external tools execute strictly via argument vectors (no shell string interpolation).

---

## 🔀 Pull Request Guidelines

1. **Create a Topic Branch**:
   ```bash
   git checkout -b feature/my-cool-feature
   ```
2. **Keep Commits Clean**:
   - Write clear, concise commit messages explaining *what* and *why*.
3. **Add Tests**:
   - For new functionality, bug fixes, or edge-case handling, add accompanying unit or integration tests in `tests/`.
4. **Format & Lint**:
   - Always run `cargo fmt` and `cargo clippy --all-targets -- -D warnings`.
5. **Open a PR**:
   - Describe the motivation, changes made, and any visual/behavioral changes.
