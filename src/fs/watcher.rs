use crate::event::AppEvent;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc::UnboundedSender;
use tracing::error;

/// Watches the current directory and forwards raw change events to the app.
/// Event coalescing (debounce) is handled by `App::tick` so that sustained
/// activity bursts still trigger exactly one refresh after they settle.
pub struct DirectoryWatcher {
    watcher: Option<RecommendedWatcher>,
    current_path: Option<PathBuf>,
}

impl Default for DirectoryWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryWatcher {
    pub fn new() -> Self {
        Self {
            watcher: None,
            current_path: None,
        }
    }

    pub fn watch(&mut self, path: &Path, event_tx: UnboundedSender<AppEvent>) {
        if self.current_path.as_deref() == Some(path) {
            return;
        }

        // Dropping the previous watcher closes its channel and stops delivery
        self.watcher = None;
        self.current_path = Some(path.to_path_buf());

        let watched_dir = path.to_path_buf();
        let result = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => {
                            // UnboundedSender::send is sync-safe from this callback
                            let _ = event_tx.send(AppEvent::DirectoryChanged {
                                path: watched_dir.clone(),
                            });
                        }
                        _ => {}
                    }
                }
            },
            Config::default(),
        );

        let mut watcher = match result {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to create watcher for {:?}: {}", path, e);
                return;
            }
        };

        if let Err(e) = watcher.watch(path, RecursiveMode::NonRecursive) {
            error!("Failed to watch directory {:?}: {}", path, e);
            return;
        }

        self.watcher = Some(watcher);
    }
}
