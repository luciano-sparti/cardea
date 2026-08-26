use crate::event::AppEvent;
use crate::fs::ops::{delete_permanently, move_to_trash};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Copy,
    Move,
}

/// How to resolve a destination entry that already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionMode {
    /// Never overwrite: derive "name (2).ext"
    AutoRename,
    /// Remove the existing entry, then copy/move over it
    Overwrite,
    /// Leave the existing entry untouched
    Skip,
}

/// A batch file operation executed on the blocking thread pool. Sources are
/// snapshotted at submission time.
#[derive(Debug)]
pub struct OpJob {
    pub kind: OpKind,
    pub collision: CollisionMode,
    pub sources: Vec<PathBuf>,
    pub dest_dir: PathBuf,
}

/// Spawns background batch operations and streams progress/completion back
/// through the app event channel (same pattern as `AsyncScanner`).
/// Each job carries an `Arc<AtomicBool>` cancel flag checked between items,
/// so a cancel request stops the loop at the next item boundary.
pub struct OpsWorker {
    event_tx: UnboundedSender<AppEvent>,
    next_id: AtomicU64,
    cancel_flags: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

impl OpsWorker {
    pub fn new(event_tx: UnboundedSender<AppEvent>) -> Self {
        Self {
            event_tx,
            next_id: AtomicU64::new(1),
            cancel_flags: Mutex::new(HashMap::new()),
        }
    }

    /// Requests cooperative cancellation of a running job. Returns `false`
    /// when the job id is unknown (already finished or never submitted).
    pub fn cancel(&self, job_id: u64) -> bool {
        let flags = self.cancel_flags.lock().unwrap();
        match flags.get(&job_id) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// Drops the cancel flag for a finished job (called from the UI thread
    /// when `OpsFinished` arrives).
    pub fn cleanup(&self, job_id: u64) {
        self.cancel_flags.lock().unwrap().remove(&job_id);
    }

    pub fn copy(&self, sources: Vec<PathBuf>, dest_dir: PathBuf) -> u64 {
        self.submit(OpJob {
            kind: OpKind::Copy,
            collision: CollisionMode::AutoRename,
            sources,
            dest_dir,
        })
    }

    pub fn copy_with(
        &self,
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
        collision: CollisionMode,
    ) -> u64 {
        self.submit(OpJob {
            kind: OpKind::Copy,
            collision,
            sources,
            dest_dir,
        })
    }

    pub fn move_entries(&self, sources: Vec<PathBuf>, dest_dir: PathBuf) -> u64 {
        self.submit(OpJob {
            kind: OpKind::Move,
            collision: CollisionMode::AutoRename,
            sources,
            dest_dir,
        })
    }

    pub fn move_with(
        &self,
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
        collision: CollisionMode,
    ) -> u64 {
        self.submit(OpJob {
            kind: OpKind::Move,
            collision,
            sources,
            dest_dir,
        })
    }

    pub fn trash(&self, paths: Vec<PathBuf>) -> u64 {
        let (id, cancel) = self.new_job();
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            tokio::task::spawn_blocking(move || run_trash_delete(id, paths, true, &tx, &cancel));
        });
        id
    }

    pub fn delete(&self, paths: Vec<PathBuf>) -> u64 {
        let (id, cancel) = self.new_job();
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            tokio::task::spawn_blocking(move || run_trash_delete(id, paths, false, &tx, &cancel));
        });
        id
    }

    fn submit(&self, job: OpJob) -> u64 {
        let (id, cancel) = self.new_job();
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            tokio::task::spawn_blocking(move || run_copy_move(id, job, &tx, &cancel));
        });
        id
    }

    /// Allocates a job id and registers its cancel flag.
    fn new_job(&self) -> (u64, Arc<AtomicBool>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let flag = Arc::new(AtomicBool::new(false));
        self.cancel_flags
            .lock()
            .unwrap()
            .insert(id, Arc::clone(&flag));
        (id, flag)
    }
}

fn report_progress(
    tx: &UnboundedSender<AppEvent>,
    job_id: u64,
    done: usize,
    total: usize,
    path: &Path,
) {
    let _ = tx.send(AppEvent::OpsProgress {
        job_id,
        done,
        total,
        current: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    });
}

fn run_trash_delete(
    job_id: u64,
    paths: Vec<PathBuf>,
    use_trash: bool,
    tx: &UnboundedSender<AppEvent>,
    cancelled: &AtomicBool,
) {
    let total = paths.len();
    let mut succeeded = 0;
    let mut errors: Vec<(PathBuf, String)> = Vec::new();

    for (done, p) in paths.iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            let _ = tx.send(AppEvent::OpsFinished {
                job_id,
                label: if use_trash {
                    "Moved to Trash"
                } else {
                    "Permanently deleted"
                }
                .to_string(),
                succeeded,
                skipped: 0,
                errors,
                dest: None,
                cancelled: true,
            });
            return;
        }
        report_progress(tx, job_id, done, total, p);
        if !p.exists() {
            continue; // stale selection
        }
        let result = if use_trash {
            move_to_trash(p)
        } else {
            delete_permanently(p)
        };
        match result {
            Ok(_) => succeeded += 1,
            Err(e) => errors.push((p.clone(), e)),
        }
    }

    let _ = tx.send(AppEvent::OpsFinished {
        job_id,
        label: if use_trash {
            "Moved to Trash"
        } else {
            "Permanently deleted"
        }
        .to_string(),
        succeeded,
        skipped: 0,
        errors,
        dest: None,
        cancelled: false,
    });
}

fn run_copy_move(job_id: u64, job: OpJob, tx: &UnboundedSender<AppEvent>, cancelled: &AtomicBool) {
    let verb = match job.kind {
        OpKind::Copy => "Copied",
        OpKind::Move => "Moved",
    };
    let total = job.sources.len();
    let mut succeeded = 0;
    let mut skipped = 0;
    let mut errors: Vec<(PathBuf, String)> = Vec::new();

    for (done, src) in job.sources.iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            let _ = tx.send(AppEvent::OpsFinished {
                job_id,
                label: verb.to_string(),
                succeeded,
                skipped,
                errors,
                dest: Some(job.dest_dir),
                cancelled: true,
            });
            return;
        }
        report_progress(tx, job_id, done, total, src);
        if !src.exists() {
            skipped += 1; // stale clipboard entry
            continue;
        }
        let name = src.file_name().unwrap_or_default();
        let plain_dest = job.dest_dir.join(name);

        // Collision handling happens before the transfer starts
        let result = if plain_dest.exists() {
            match job.collision {
                CollisionMode::AutoRename => {
                    copy_or_move(job.kind, src, &unique_destination(&plain_dest))
                }
                CollisionMode::Skip => Ok(None), // counted as skipped below
                CollisionMode::Overwrite => remove_existing(&plain_dest)
                    .and_then(|_| copy_or_move(job.kind, src, &plain_dest)),
            }
        } else {
            copy_or_move(job.kind, src, &plain_dest)
        };

        match result {
            Ok(Some(())) => succeeded += 1,
            Ok(None) => skipped += 1,
            Err(e) => errors.push((src.clone(), e)),
        }
    }

    let _ = tx.send(AppEvent::OpsFinished {
        job_id,
        label: verb.to_string(),
        succeeded,
        skipped,
        errors,
        dest: Some(job.dest_dir),
        cancelled: false,
    });
}

fn copy_or_move(kind: OpKind, from: &Path, to: &Path) -> Result<Option<()>, String> {
    match kind {
        OpKind::Copy => copy_entry(from, to).map(|_| Some(())),
        OpKind::Move => move_entry(from, to).map(|_| Some(())),
    }
}

fn remove_existing(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| format!("Failed to replace {:?}: {}", path, e))
    } else {
        std::fs::remove_file(path).map_err(|e| format!("Failed to replace {:?}: {}", path, e))
    }
}

fn copy_entry(from: &Path, to: &Path) -> Result<(), String> {
    if from.is_dir() {
        std::fs::create_dir_all(to).map_err(|e| format!("Failed to create {:?}: {}", to, e))?;
        for entry in
            std::fs::read_dir(from).map_err(|e| format!("Failed to read {:?}: {}", from, e))?
        {
            let entry = entry.map_err(|e| format!("Failed to read {:?}: {}", from, e))?;
            copy_entry(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to)
            .map(|_| ())
            .map_err(|e| format!("Failed to copy {:?}: {}", from, e))
    }
}

fn move_entry(from: &Path, to: &Path) -> Result<(), String> {
    match std::fs::rename(from, to) {
        Ok(_) => Ok(()),
        Err(_) => {
            // Cross-device move: fall back to copy + delete
            copy_entry(from, to)?;
            if from.is_dir() {
                std::fs::remove_dir_all(from)
                    .map_err(|e| format!("Failed to remove {:?}: {}", from, e))
            } else {
                std::fs::remove_file(from)
                    .map_err(|e| format!("Failed to remove {:?}: {}", from, e))
            }
        }
    }
}

/// Collision resolution for paste/move: never overwrite — derive a free name
/// like "report (2).txt". Full conflict dialogs land in a later M2 pass.
fn unique_destination(dest: &Path) -> PathBuf {
    if !dest.exists() {
        return dest.to_path_buf();
    }
    let stem = dest
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = dest
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for i in 2..10_000u32 {
        let candidate = dest.with_file_name(format!("{} ({}){}", stem, i, ext));
        if !candidate.exists() {
            return candidate;
        }
    }
    dest.to_path_buf()
}
