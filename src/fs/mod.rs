use crate::config::{SortColumn, SortDirection};
use chrono::{DateTime, Local};
use compact_str::CompactString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub mod archive;
pub mod ops;
pub mod scanner;
pub mod watcher;
pub mod worker;

/// Free space available to unprivileged users on the filesystem containing `path`.
#[cfg(unix)]
pub fn disk_free_bytes(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: c_path is a valid NUL-terminated string and stat is exclusively owned
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    Some(stat.f_bavail as u64 * stat.f_frsize as u64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Directory,
    Symlink,
    Executable,
    Archive,
    Image,
    Audio,
    Video,
    Document,
    Code,
    Regular,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: CompactString,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub symlink_target: Option<PathBuf>,
    pub size: u64,
    pub modified: Option<DateTime<Local>>,
    pub permissions: u32,
    pub is_hidden: bool,
    pub is_executable: bool,
    pub extension: Option<CompactString>,
    pub kind: FileKind,
}

impl FileEntry {
    /// Builds an entry from a readdir result. Costs at most one stat syscall
    /// per entry; only symlinks require a second (following) call.
    pub fn from_dentry(entry: &std::fs::DirEntry) -> Option<Self> {
        let os_name = entry.file_name();
        let name_str = os_name.to_string_lossy();
        let name = CompactString::new(&name_str);
        let is_hidden = name_str.starts_with('.');

        let file_type = entry.file_type().ok()?;
        let is_symlink = file_type.is_symlink();
        let path = entry.path();

        let symlink_target = if is_symlink {
            std::fs::read_link(&path).ok()
        } else {
            None
        };

        // Follow symlinks to classify them (broken links fall back to link metadata)
        let followed_meta = if is_symlink {
            std::fs::metadata(&path).ok()
        } else {
            None
        };
        let meta = followed_meta.clone().or_else(|| entry.metadata().ok())?;

        let is_dir = if is_symlink {
            followed_meta.is_some_and(|m| m.is_dir())
        } else {
            file_type.is_dir()
        };
        let size = if is_dir { 0 } else { meta.len() };
        let permissions = meta.permissions().mode();
        let is_executable = !is_dir && (permissions & 0o111 != 0);

        let modified: Option<DateTime<Local>> = meta.modified().ok().map(|t| t.into());

        let extension = path
            .extension()
            .map(|e| CompactString::new(e.to_string_lossy()));
        let ext_lower = extension.as_ref().map(|s| s.to_lowercase());

        let kind = if is_dir {
            FileKind::Directory
        } else if is_symlink {
            FileKind::Symlink
        } else if let Some(ref ext) = ext_lower {
            match ext.as_str() {
                "zip" | "tar" | "gz" | "xz" | "7z" | "bz2" | "zst" | "rar" | "tgz" => {
                    FileKind::Archive
                }
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "avif" => {
                    FileKind::Image
                }
                "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus" => FileKind::Audio,
                "mp4" | "mkv" | "webm" | "avi" | "mov" | "flv" | "wmv" => FileKind::Video,
                "pdf" | "doc" | "docx" | "epub" | "txt" | "rtf" | "odt" => FileKind::Document,
                "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "cpp" | "h" | "hpp" | "go"
                | "java" | "sh" | "bash" | "zsh" | "html" | "css" | "scss" | "json" | "toml"
                | "yaml" | "yml" | "md" | "sql" | "vim" | "lua" => FileKind::Code,
                _ if is_executable => FileKind::Executable,
                _ => FileKind::Regular,
            }
        } else if is_executable {
            FileKind::Executable
        } else {
            FileKind::Regular
        };

        Some(Self {
            name,
            path: path.to_path_buf(),
            is_dir,
            is_symlink,
            symlink_target,
            size,
            modified,
            permissions,
            is_hidden,
            is_executable,
            extension,
            kind,
        })
    }

    pub fn formatted_size(&self) -> String {
        if self.is_dir {
            "-".to_string()
        } else {
            format_size(self.size)
        }
    }

    pub fn formatted_modified(&self) -> String {
        match self.modified {
            Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
            None => "-".to_string(),
        }
    }

    pub fn formatted_permissions(&self) -> String {
        format_permissions(self.permissions, self.is_dir, self.is_symlink)
    }
}

pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes < KB {
        format!("{} B", bytes)
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes < TB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    }
}

pub fn format_permissions(mode: u32, is_dir: bool, is_symlink: bool) -> String {
    let prefix = if is_symlink {
        'l'
    } else if is_dir {
        'd'
    } else {
        '-'
    };

    let user_r = if mode & 0o400 != 0 { 'r' } else { '-' };
    let user_w = if mode & 0o200 != 0 { 'w' } else { '-' };
    let user_x = if mode & 0o100 != 0 { 'x' } else { '-' };

    let group_r = if mode & 0o040 != 0 { 'r' } else { '-' };
    let group_w = if mode & 0o020 != 0 { 'w' } else { '-' };
    let group_x = if mode & 0o010 != 0 { 'x' } else { '-' };

    let other_r = if mode & 0o004 != 0 { 'r' } else { '-' };
    let other_w = if mode & 0o002 != 0 { 'w' } else { '-' };
    let other_x = if mode & 0o001 != 0 { 'x' } else { '-' };

    format!(
        "{}{}{}{}{}{}{}{}{}{}",
        prefix, user_r, user_w, user_x, group_r, group_w, group_x, other_r, other_w, other_x
    )
}

pub fn sort_entries(
    entries: &mut [FileEntry],
    column: SortColumn,
    direction: SortDirection,
    dirs_first: bool,
    natural: bool,
) {
    entries.sort_by(|a, b| {
        // Folders first rule
        if dirs_first && a.is_dir != b.is_dir {
            return if a.is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }

        let order = match column {
            SortColumn::Name => {
                if natural {
                    natord::compare(&a.name, &b.name)
                } else {
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                }
            }
            SortColumn::Size => a.size.cmp(&b.size),
            SortColumn::Modified => a.modified.cmp(&b.modified),
            SortColumn::Extension => {
                let ext_a = a.extension.as_deref().unwrap_or("");
                let ext_b = b.extension.as_deref().unwrap_or("");
                ext_a.cmp(ext_b).then_with(|| a.name.cmp(&b.name))
            }
            SortColumn::Permissions => a.permissions.cmp(&b.permissions),
        };

        match direction {
            SortDirection::Ascending => order,
            SortDirection::Descending => order.reverse(),
        }
    });
}
