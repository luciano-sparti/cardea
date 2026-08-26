use crate::event::AppEvent;
use crate::fs::FileEntry;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info};

pub struct AsyncScanner {
    event_tx: UnboundedSender<AppEvent>,
    current_scan_id: Arc<AtomicU64>,
    current_search_id: Arc<AtomicU64>,
}

impl AsyncScanner {
    pub fn new(event_tx: UnboundedSender<AppEvent>) -> Self {
        Self {
            event_tx,
            current_scan_id: Arc::new(AtomicU64::new(0)),
            current_search_id: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn scan_directory(&self, path: PathBuf) -> u64 {
        let scan_id = self.current_scan_id.fetch_add(1, Ordering::SeqCst) + 1;
        let event_tx = self.event_tx.clone();
        let current_scan_id = self.current_scan_id.clone();
        let target_path = path.clone();

        tokio::task::spawn_blocking(move || {
            let mut read_dir = match std::fs::read_dir(&target_path) {
                Ok(rd) => rd,
                Err(e) => {
                    error!("Failed to read directory {:?}: {}", target_path, e);
                    let _ = event_tx.send(AppEvent::DirectoryScanFailed {
                        path: target_path,
                        error: e.to_string(),
                    });
                    return;
                }
            };

            let mut batch = Vec::with_capacity(256);
            const BATCH_SIZE: usize = 256;

            for entry_res in &mut read_dir {
                // Check if this scan has been cancelled by a newer scan request
                if current_scan_id.load(Ordering::Relaxed) != scan_id {
                    info!("Scan #{} for {:?} was cancelled", scan_id, target_path);
                    return;
                }

                if let Ok(entry) = entry_res {
                    if let Some(file_entry) = FileEntry::from_dentry(&entry) {
                        batch.push(file_entry);
                        if batch.len() >= BATCH_SIZE {
                            let _ = event_tx.send(AppEvent::DirectoryScannedChunk {
                                scan_id,
                                path: target_path.clone(),
                                entries: std::mem::replace(
                                    &mut batch,
                                    Vec::with_capacity(BATCH_SIZE),
                                ),
                                is_final: false,
                            });
                        }
                    }
                }
            }

            // Check cancellation once more before sending final batch
            if current_scan_id.load(Ordering::Relaxed) == scan_id {
                let _ = event_tx.send(AppEvent::DirectoryScannedChunk {
                    scan_id,
                    path: target_path,
                    entries: batch,
                    is_final: true,
                });
            }
        });

        scan_id
    }

    pub fn scan_tree_children(&self, path: PathBuf) {
        let event_tx = self.event_tx.clone();
        tokio::task::spawn_blocking(move || {
            let read_dir = match std::fs::read_dir(&path) {
                Ok(rd) => rd,
                Err(_) => return,
            };

            let mut subdirs = Vec::new();
            for entry in read_dir.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = p.file_name().unwrap_or_default().to_string_lossy();
                    if !name.starts_with('.') {
                        subdirs.push(p);
                    }
                }
            }

            subdirs.sort_by(|a, b| {
                natord::compare(
                    &a.file_name().unwrap_or_default().to_string_lossy(),
                    &b.file_name().unwrap_or_default().to_string_lossy(),
                )
            });

            let _ = event_tx.send(AppEvent::TreeChildrenLoaded {
                path,
                children: subdirs,
            });
        });
    }

    /// Loads a text preview in the background without blocking the UI thread.
    /// `text` is `Some(None)` when the file is unreadable or not valid UTF-8.
    /// Image files are decoded instead (graphics-protocol previews), and text
    /// is syntax-highlighted here (off the UI thread) when a matching syntect
    /// definition exists for the path.
    pub fn load_preview(&self, path: PathBuf, max_bytes: u64) {
        let event_tx = self.event_tx.clone();
        tokio::task::spawn_blocking(move || {
            use std::io::Read;

            // MIME + SHA-256 for the metadata inspector (all file kinds)
            let meta = preview_meta(&path);

            // Archive files: list contents without extracting anything
            if crate::fs::archive::is_archive(&path) {
                let text = crate::fs::archive::preview_listing(&path);
                let _ = event_tx.send(AppEvent::PreviewLoaded {
                    path,
                    text,
                    styled: None,
                    image: None,
                    hex_dump: None,
                    meta,
                });
                return;
            }

            // Image files: decode off-thread; failure falls through to text
            let image = if is_image_path(&path) {
                image::ImageReader::open(&path)
                    .ok()
                    .and_then(|r| r.with_guessed_format().ok())
                    .and_then(|r| r.decode().ok())
            } else {
                None
            };

            let mut total_bytes = 0u64;
            let text = if image.is_some() {
                None
            } else {
                std::fs::File::open(&path).ok().and_then(|file| {
                    if file.metadata().ok().is_some_and(|m| m.is_dir()) {
                        return None;
                    }
                    let mut buf = Vec::new();
                    file.take(max_bytes).read_to_end(&mut buf).ok()?;
                    total_bytes = buf.len() as u64;
                    // Git-style heuristic: a NUL byte marks binary content
                    if buf.contains(&0) {
                        return None;
                    }
                    // Truncation can split a multi-byte UTF-8 char; trim up to 3 bytes
                    for trim in 0..4 {
                        let end = buf.len().saturating_sub(trim);
                        if let Ok(s) = std::str::from_utf8(&buf[..end]) {
                            return Some(s.to_string());
                        }
                    }
                    None
                })
            };

            let styled = text
                .as_deref()
                .and_then(|t| crate::theme::syntax::highlight(&path, t));

            // Non-UTF-8 (binary) content gets an xxd-style hex dump instead
            let hex_dump = if text.is_none() && image.is_none() {
                binary_hex_dump(&path, HEX_DUMP_BYTES)
            } else {
                None
            };

            let _ = event_tx.send(AppEvent::PreviewLoaded {
                path,
                text,
                styled,
                image,
                hex_dump,
                meta,
            });
        });
    }

    /// Cancels any in-flight recursive search (late chunks are dropped by the app).
    pub fn cancel_search(&self) {
        self.current_search_id.fetch_add(1, Ordering::SeqCst);
    }

    /// Non-blocking recursive search streaming matches in chunks.
    /// Symlink cycles are guarded by a visited-set of canonical dir paths.
    pub fn search_recursive(&self, root: PathBuf, query: String, include_hidden: bool) -> u64 {
        let search_id = self.current_search_id.fetch_add(1, Ordering::SeqCst) + 1;
        let event_tx = self.event_tx.clone();
        let current_search_id = self.current_search_id.clone();

        tokio::task::spawn_blocking(move || {
            const BATCH_SIZE: usize = 128;
            let needle = query.to_lowercase();
            let mut batch = Vec::with_capacity(BATCH_SIZE);
            let mut visited: HashSet<PathBuf> = HashSet::new();
            let mut stack = vec![root];

            while let Some(dir) = stack.pop() {
                if current_search_id.load(Ordering::Relaxed) != search_id {
                    info!("Search #{} was cancelled", search_id);
                    return;
                }

                // Canonicalize to detect symlink cycles
                if let Ok(canon) = dir.canonicalize() {
                    if !visited.insert(canon) {
                        continue;
                    }
                }

                let Ok(read_dir) = std::fs::read_dir(&dir) else {
                    continue; // permission-denied or vanished mid-search
                };

                for entry in read_dir.flatten() {
                    if current_search_id.load(Ordering::Relaxed) != search_id {
                        return;
                    }

                    let Ok(ft) = entry.file_type() else { continue };
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = ft.is_dir()
                        || (ft.is_symlink()
                            && entry.metadata().map(|m| m.is_dir()).unwrap_or(false));

                    if is_dir && !name.starts_with('.') {
                        stack.push(entry.path());
                    }

                    if !include_hidden && name.starts_with('.') {
                        continue;
                    }
                    if !name.to_lowercase().contains(&needle) {
                        continue;
                    }
                    if let Some(file_entry) = FileEntry::from_dentry(&entry) {
                        batch.push(file_entry);
                        if batch.len() >= BATCH_SIZE {
                            let _ = event_tx.send(AppEvent::SearchResultsChunk {
                                search_id,
                                matches: std::mem::replace(
                                    &mut batch,
                                    Vec::with_capacity(BATCH_SIZE),
                                ),
                                is_final: false,
                            });
                        }
                    }
                }
            }

            if current_search_id.load(Ordering::Relaxed) == search_id {
                let _ = event_tx.send(AppEvent::SearchResultsChunk {
                    search_id,
                    matches: batch,
                    is_final: true,
                });
            }
        });

        search_id
    }
}

/// Extensions decoded as images for graphics-protocol previews.
fn is_image_path(path: &std::path::Path) -> bool {
    const IMAGE_EXTS: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "bmp", "webp", "ico", "tif", "tiff",
    ];
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Bytes covered by the binary hex dump preview
pub(crate) const HEX_DUMP_BYTES: usize = 2048;

/// Files above this size skip SHA-256 hashing (preview must stay snappy)
const MAX_HASH_BYTES: u64 = 128 * 1024 * 1024;

/// MIME type (from extension) and SHA-256 digest for the metadata panel.
/// Runs on the blocking pool; hashing is skipped for very large files.
fn preview_meta(path: &std::path::Path) -> Option<crate::event::PreviewMeta> {
    if path.is_dir() {
        return None;
    }
    let mime = mime_guess::from_path(path).first_raw().map(str::to_string);

    let sha256 = if std::fs::metadata(path)
        .ok()
        .is_some_and(|m| m.len() <= MAX_HASH_BYTES)
    {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        std::fs::File::open(path).ok().and_then(|mut file| {
            let mut hasher = Sha256::new();
            let mut buf = [0u8; 64 * 1024];
            loop {
                match file.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => hasher.update(&buf[..n]),
                    Err(_) => return None,
                }
            }
            Some(format!("{:x}", hasher.finalize()))
        })
    } else {
        None
    };

    if mime.is_none() && sha256.is_none() {
        return None;
    }
    Some(crate::event::PreviewMeta { mime, sha256 })
}

/// Builds an `xxd`-style dump of up to `max_bytes`: offset column, hex bytes,
/// ASCII gutter. `None` when the file is unreadable, a directory, or empty.
fn binary_hex_dump(path: &std::path::Path, max_bytes: usize) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok().is_some_and(|m| m.is_dir()) {
        return None;
    }
    let total = file.metadata().ok()?.len();
    if total == 0 {
        return None;
    }

    use std::io::Read;
    let mut buf = vec![0u8; max_bytes];
    let mut handle = file.take(max_bytes as u64);
    let read = handle.read(&mut buf).ok()?;
    buf.truncate(read);

    let mut out = format!(
        "Binary content — hex dump of first {} of {} bytes\n\n",
        crate::fs::format_size(read as u64),
        crate::fs::format_size(total)
    );
    for (chunk_idx, chunk) in buf.chunks(16).enumerate() {
        let mut hex = String::with_capacity(16 * 3);
        let mut ascii = String::with_capacity(16);
        for (i, b) in chunk.iter().enumerate() {
            if i == 8 {
                hex.push(' ');
            }
            hex.push_str(&format!("{:02x} ", b));
            ascii.push(if b.is_ascii_graphic() || *b == b' ' {
                *b as char
            } else {
                '.'
            });
        }
        // Pad the hex field so the gutter stays aligned on the last row
        while hex.len() < 49 {
            hex.push(' ');
        }
        out.push_str(&format!("{:08x}  {}|{}|\n", chunk_idx * 16, hex, ascii));
    }
    Some(out)
}
