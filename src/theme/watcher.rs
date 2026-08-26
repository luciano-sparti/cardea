use crate::event::AppEvent;
use crate::theme::Theme;
use notify::{
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info};

pub struct ThemeWatcher {
    _watcher: Option<RecommendedWatcher>,
}

impl ThemeWatcher {
    pub fn start(event_tx: UnboundedSender<AppEvent>) -> Self {
        let Ok(home) = std::env::var("HOME") else {
            return Self { _watcher: None };
        };

        // Candidate locations holding the active Omarchy theme:
        // v4 (state dir): current/theme.name + current/theme/colors.toml
        // legacy (config dir): current/theme with a palette name
        let watched_dirs = [
            PathBuf::from(&home).join(".local/state/omarchy/current"),
            PathBuf::from(&home).join(".config/omarchy/current"),
        ];
        let existing: Vec<PathBuf> = watched_dirs.into_iter().filter(|d| d.exists()).collect();
        if existing.is_empty() {
            return Self { _watcher: None };
        }

        let (std_tx, std_rx) = mpsc::channel();

        let watcher_res = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = std_tx.send(event);
                }
            },
            NotifyConfig::default().with_poll_interval(Duration::from_secs(2)),
        );

        let mut watcher = match watcher_res {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to initialize ThemeWatcher: {}", e);
                return Self { _watcher: None };
            }
        };

        // Watch the dirs themselves plus the nested theme subdir (v4 keeps
        // colors.toml one level down; NonRecursive misses it otherwise)
        for dir in &existing {
            if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                error!("Failed to watch {:?}: {}", dir, e);
            }
            let nested = dir.join("theme");
            if nested.is_dir() {
                if let Err(e) = watcher.watch(&nested, RecursiveMode::NonRecursive) {
                    error!("Failed to watch {:?}: {}", nested, e);
                }
            }
        }

        tokio::spawn(async move {
            while let Ok(event) = std_rx.recv() {
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) => {
                        if !event.paths.iter().any(|p| is_theme_related(p)) {
                            continue;
                        }
                        // Debounce: writers often touch several files in bursts
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        match Theme::try_load_omarchy() {
                            Some(theme) => {
                                info!("Omarchy theme changed to: {}", theme.name);
                                let _ = event_tx.send(AppEvent::ThemeChanged(Box::new(theme)));
                            }
                            None => {
                                error!("Omarchy theme change unreadable; keeping current palette");
                            }
                        }
                    }
                    _ => {}
                }
            }
        });

        Self {
            _watcher: Some(watcher),
        }
    }
}

fn is_theme_related(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name == "theme" || name == "theme.name" || name == "colors.toml" || name.ends_with(".toml")
}
