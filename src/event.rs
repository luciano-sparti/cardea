use crate::fs::FileEntry;
use crate::theme::Theme;
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEvent, MouseEvent};
use futures::StreamExt;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tracing::error;

/// Auxiliary metadata computed alongside previews (off-thread).
#[derive(Debug, Clone)]
pub struct PreviewMeta {
    /// Best-effort MIME type from the file extension
    pub mime: Option<String>,
    /// Full SHA-256 hex digest (files up to 128 MB only)
    pub sha256: Option<String>,
}

#[derive(Debug)]
pub enum AppEvent {
    // Inputs
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    Tick,

    // Filesystem Scanner Events
    DirectoryScannedChunk {
        scan_id: u64,
        path: PathBuf,
        entries: Vec<FileEntry>,
        is_final: bool,
    },
    DirectoryScanFailed {
        path: PathBuf,
        error: String,
    },
    DirectoryChanged {
        path: PathBuf,
    },
    TreeChildrenLoaded {
        path: PathBuf,
        children: Vec<PathBuf>,
    },

    // Preview Events
    PreviewLoaded {
        path: PathBuf,
        /// `None` means unreadable or binary; `Some(text)` is preview content
        text: Option<String>,
        /// Syntax-highlighted rendering of the same content; `None` when no
        /// syntax definition matched (plain fallback)
        styled: Option<Vec<ratatui::text::Line<'static>>>,
        /// Decoded image for graphics-protocol previews; `None` for non-images
        image: Option<image::DynamicImage>,
        /// `xxd`-style hex dump for binary (non-UTF-8) files
        hex_dump: Option<String>,
        /// MIME type + SHA-256 for the metadata inspector
        meta: Option<PreviewMeta>,
    },

    // Recursive Search Events
    SearchResultsChunk {
        search_id: u64,
        matches: Vec<FileEntry>,
        is_final: bool,
    },

    // File Operations Worker Events
    OpsProgress {
        job_id: u64,
        done: usize,
        total: usize,
        current: String,
    },
    OpsFinished {
        job_id: u64,
        label: String,
        succeeded: usize,
        skipped: usize,
        /// Per-item failures: (source path, error message)
        errors: Vec<(PathBuf, String)>,
        /// Destination directory (copy/move jobs only)
        dest: Option<PathBuf>,
        /// True when the job stopped early due to a cancel request
        cancelled: bool,
    },

    // Theme Events
    ThemeChanged(Box<Theme>),

    // Status Messages
    StatusMessage {
        text: String,
        is_error: bool,
    },
}

pub struct EventHandler {
    pub tx: UnboundedSender<AppEvent>,
    pub rx: UnboundedReceiver<AppEvent>,
}

impl EventHandler {
    pub fn new(tick_rate_ms: u64) -> Self {
        let (tx, rx) = unbounded_channel();
        let event_tx = tx.clone();

        tokio::spawn(async move {
            let mut reader = EventStream::new();
            let mut tick_interval = tokio::time::interval(Duration::from_millis(tick_rate_ms));

            loop {
                let tick_delay = tick_interval.tick();
                let crossterm_event = reader.next();

                tokio::select! {
                    _ = tick_delay => {
                        if event_tx.send(AppEvent::Tick).is_err() {
                            break;
                        }
                    }
                    maybe_event = crossterm_event => {
                        match maybe_event {
                            Some(Ok(evt)) => {
                                match evt {
                                    CrosstermEvent::Key(key) if event_tx.send(AppEvent::Key(key)).is_err() => {
                                        break;
                                    }
                                    CrosstermEvent::Mouse(mouse) if event_tx.send(AppEvent::Mouse(mouse)).is_err() => {
                                        break;
                                    }
                                    CrosstermEvent::Resize(w, h) if event_tx.send(AppEvent::Resize(w, h)).is_err() => {
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                            Some(Err(e)) => {
                                error!("Error reading crossterm event: {}", e);
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        Self { tx, rx }
    }
}
