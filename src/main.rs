use std::io::stdout;
use std::path::PathBuf;

use cardea::app::App;
use cardea::config::Config;
use cardea::event::{AppEvent, EventHandler};
use cardea::theme::watcher::ThemeWatcher;
use cardea::theme::Theme;
use cardea::ui;
use clap::{CommandFactory, Parser};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

#[derive(Parser, Debug)]
#[command(name = "cardea", author, version, about = "A desktop-style terminal file explorer", long_about = None)]
struct Cli {
    /// Initial path to open
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,

    /// Show hidden and dotfiles by default
    #[arg(short = 'a', long = "all")]
    all: bool,

    /// Color theme (catppuccin-mocha, tokyo-night, gruvbox, nord, ansi, omarchy)
    #[arg(short = 't', long = "theme")]
    theme: Option<String>,

    /// Start with file preview panel open
    #[arg(short = 'p', long = "preview")]
    preview: bool,

    /// Hide sidebar places and tree panel
    #[arg(long = "no-sidebar")]
    no_sidebar: bool,

    /// Generate shell completion script and exit (bash, zsh, fish, elvish, powershell)
    #[arg(long = "generate-completions", value_name = "SHELL")]
    generate_completions: Option<clap_complete::Shell>,

    /// Generate man page to stdout and exit
    #[arg(long = "generate-manpage")]
    generate_manpage: bool,
}

fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(panic_info);
    }));
}

fn init_logging() {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };
    let log_dir = PathBuf::from(home).join(".local/state/cardea");
    if std::fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    let log_file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("cardea.log"))
    {
        Ok(f) => f,
        Err(_) => return,
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(move || log_file.try_clone().expect("log file clone"))
        .with_ansi(false)
        .try_init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if let Some(shell) = cli.generate_completions {
        let mut cmd = Cli::command();
        clap_complete::generate(shell, &mut cmd, "cardea", &mut std::io::stdout());
        return Ok(());
    }

    if cli.generate_manpage {
        let cmd = Cli::command();
        let man = clap_mangen::Man::new(cmd);
        man.render(&mut std::io::stdout())?;
        return Ok(());
    }

    // Install panic hook for clean terminal teardown on crash
    install_panic_hook();
    init_logging();

    let mut config = Config::load();

    // CLI overrides
    if cli.all {
        config.general.show_hidden = true;
    }
    if cli.preview {
        config.layout.show_preview = true;
    }
    if cli.no_sidebar {
        config.layout.show_sidebar = false;
    }
    if let Some(t) = cli.theme {
        config.general.theme = t;
    }

    // Determine initial theme (try Omarchy first if set or configured, else fallback),
    // then layer custom TOML overrides and environment degradation on top.
    let mut current_theme = if config.general.theme == "omarchy" {
        Theme::try_load_omarchy().unwrap_or_else(|| config.resolved_theme())
    } else if let Some(custom) = &config.custom_theme {
        custom
            .resolve(&config.general.theme)
            .unwrap_or_else(|errors| {
                for e in errors {
                    tracing::error!("custom_theme: {}", e);
                }
                tracing::warn!("custom_theme ignored due to errors; using built-in palette");
                Theme::from_name(&config.general.theme)
            })
    } else {
        Theme::from_name(&config.general.theme)
    };
    current_theme = Theme::effective(current_theme, Theme::no_color_enabled());

    // Probe terminal graphics protocol + font size BEFORE raw mode / the
    // alternate screen (the query needs the real stdin/stdout). Falls back
    // to a heuristic font size with unicode-halfblock rendering.
    let image_picker = ratatui_image::picker::Picker::from_query_stdio()
        .unwrap_or_else(|_| ratatui_image::picker::Picker::from_fontsize((8, 18)));

    // Initialize terminal
    enable_raw_mode()?;
    let mut stdout_handle = stdout();
    execute!(stdout_handle, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout_handle);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Initialize Event Loop (50ms tick rate for smooth animations)
    let mut event_handler = EventHandler::new(50);
    let event_tx = event_handler.tx.clone();

    // Start background theme watcher (watches ~/.config/omarchy/current/theme)
    let _theme_watcher = ThemeWatcher::start(event_tx.clone());

    // Initialize App State
    let mut app = App::new(cli.path, &config, event_tx);
    app.set_image_picker(image_picker);

    // Main Event Loop
    while !app.should_quit {
        app.maybe_request_preview();
        terminal.draw(|f| {
            ui::render(f, &mut app, &current_theme);
        })?;
        app.drain_image_encoding();

        if let Some(event) = event_handler.rx.recv().await {
            match event {
                AppEvent::Key(key) => {
                    app.handle_key_event(key);
                }
                AppEvent::Mouse(mouse) => {
                    if config.general.mouse_enabled {
                        app.handle_mouse_event(mouse);
                    }
                }
                AppEvent::Resize(_, _) => {}
                AppEvent::Tick => {
                    app.tick();
                }
                AppEvent::DirectoryScannedChunk {
                    scan_id,
                    path,
                    entries,
                    is_final,
                } => {
                    app.apply_scan_chunk(scan_id, &path, entries, is_final);
                }
                AppEvent::DirectoryScanFailed { path, error } => {
                    if path == app.tab().current_dir {
                        app.set_status_error(format!("Failed to scan directory: {}", error));
                    }
                }
                AppEvent::DirectoryChanged { path } => {
                    app.mark_fs_dirty(&path);
                }
                AppEvent::PreviewLoaded {
                    path,
                    text,
                    styled,
                    image,
                    hex_dump,
                    meta,
                } => {
                    app.on_preview_loaded(path, text, styled, image, hex_dump, meta);
                }
                AppEvent::SearchResultsChunk {
                    search_id,
                    matches,
                    is_final,
                } => {
                    app.apply_search_chunk(search_id, matches, is_final);
                }
                AppEvent::OpsProgress {
                    job_id,
                    done,
                    total,
                    current,
                } => {
                    app.on_ops_progress(job_id, done, total, current);
                }
                AppEvent::OpsFinished {
                    job_id,
                    label,
                    succeeded,
                    skipped,
                    errors,
                    dest,
                    cancelled,
                } => {
                    app.on_ops_finished(cardea::app::OpsOutcome::from_event(
                        job_id, label, succeeded, skipped, errors, dest, cancelled,
                    ));
                }
                AppEvent::ThemeChanged(new_theme) => {
                    // Hot-reloaded palettes get the same environment
                    // degradation (NO_COLOR / non-truecolor) as startup
                    current_theme = Theme::effective(*new_theme, Theme::no_color_enabled());
                    app.set_status_info(format!("Applied theme: {}", current_theme.name));
                }
                AppEvent::StatusMessage { text, is_error } => {
                    if is_error {
                        app.set_status_error(text);
                    } else {
                        app.set_status_info(text);
                    }
                }
                AppEvent::TreeChildrenLoaded { path, children } => {
                    app.on_tree_children_loaded(path, children);
                }
            }
        }
    }

    // Terminal Cleanup
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
