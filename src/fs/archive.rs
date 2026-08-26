use crate::fs::format_size;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// How many entries to include before truncating the listing
const MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Zip,
    Tar,
    TarGz,
    TarXz,
    SevenZ,
}

/// Detects the archive kind from the full file name (lowercased). Extension
/// alone is not enough for `.tar.gz` / `.tar.xz`.
fn detect_kind(file_name: &str) -> Option<Kind> {
    if file_name.ends_with(".zip") {
        Some(Kind::Zip)
    } else if file_name.ends_with(".7z") {
        Some(Kind::SevenZ)
    } else if file_name.ends_with(".tar") {
        Some(Kind::Tar)
    } else if file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") {
        Some(Kind::TarGz)
    } else if file_name.ends_with(".tar.xz") || file_name.ends_with(".txz") {
        Some(Kind::TarXz)
    } else {
        None
    }
}

pub fn is_archive(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(detect_kind)
        .is_some()
}

/// Builds a plain-text content listing for an archive without extracting
/// anything to disk. Returns `None` when the path is not a supported archive;
/// corrupt or unreadable archives yield an error message so the preview shows
/// feedback instead of binary garbage.
pub fn preview_listing(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?.to_lowercase();
    let kind = detect_kind(&name)?;

    let rows = match kind {
        Kind::Zip => list_zip(path),
        Kind::Tar | Kind::TarGz | Kind::TarXz => list_tar(path, kind),
        Kind::SevenZ => list_7z(path),
    };

    let rows = match rows {
        Ok(rows) => rows,
        Err(e) => return Some(format!("Could not read archive: {}\n({})", name, e)),
    };
    if rows.is_empty() {
        return Some(format!("Empty archive: {}", name));
    }

    let mut sorted = rows;
    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = String::new();
    out.push_str(&format!("Archive: {} — {} item(s)\n\n", name, sorted.len()));
    out.push_str(&format!("{:<50} SIZE\n", " ENTRY"));
    for row in sorted.iter().take(MAX_ENTRIES) {
        out.push_str(&format!(
            "{:<50} {}\n",
            format!(" {}", truncate_name(&row.0, 47)),
            format_size(row.1)
        ));
    }
    if sorted.len() > MAX_ENTRIES {
        out.push_str(&format!(
            "\n… and {} more entries",
            sorted.len() - MAX_ENTRIES
        ));
    }
    Some(out)
}

fn truncate_name(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_string();
    }
    let mut s: String = name.chars().take(max - 3).collect();
    s.push_str("...");
    s.replace('\n', " ")
}

type Row = (String, u64);

fn list_zip(path: &Path) -> Result<Vec<Row>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("invalid zip: {}", e))?;
    let mut rows = Vec::with_capacity(archive.len());
    // by_index_raw reads only the central directory: no decompression
    for i in 0..archive.len() {
        match archive.by_index_raw(i) {
            Ok(entry) => rows.push((entry.name().to_string(), entry.size())),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(rows)
}

fn list_tar(path: &Path, kind: Kind) -> Result<Vec<Row>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let reader: Box<dyn Read> = match kind {
        Kind::Tar => Box::new(file),
        Kind::TarGz => Box::new(flate2::read::GzDecoder::new(file)),
        Kind::TarXz => Box::new(xz2::read::XzDecoder::new(file)),
        _ => unreachable!("tar dispatcher called with non-tar kind"),
    };

    let mut archive = tar::Archive::new(reader);
    let mut rows = Vec::new();
    for entry in archive
        .entries()
        .map_err(|e| format!("corrupt tar: {}", e))?
    {
        let entry = entry.map_err(|e| format!("corrupt tar: {}", e))?;
        let name = entry
            .path()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_string();
        let size = entry
            .header()
            .entry_size()
            .unwrap_or_else(|_| entry.header().size().unwrap_or(0));
        rows.push((name.replace('\n', " "), size));
    }
    Ok(rows)
}

fn list_7z(path: &Path) -> Result<Vec<Row>, String> {
    let reader = sevenz_rust::SevenZReader::open(path, sevenz_rust::Password::empty())
        .map_err(|e| format!("invalid 7z: {}", e))?;
    // Header metadata only: no entry data is decoded
    Ok(reader
        .archive()
        .files
        .iter()
        .map(|e| (e.name.clone(), e.size))
        .collect())
}

/// Extracts an archive into `dest_dir` using external tools (tar/unzip/7z).
/// Commands are spawned as argument vectors — never through a shell — so paths
/// with spaces or special characters are safe. Returns `Err` with a
/// user-readable message when the required tool is not installed or the
/// extraction fails.
pub fn extract_archive(path: &Path, dest_dir: &Path) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("Invalid archive path: {:?}", path))?
        .to_lowercase();

    let kind = detect_kind(&name).ok_or_else(|| format!("Not a supported archive: {}", name))?;

    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create destination: {}", e))?;

    let result = match kind {
        Kind::Tar => {
            let output = std::process::Command::new("tar")
                .args(["xf", &path.to_string_lossy(), "-C", &dest_dir.to_string_lossy()])
                .output()
                .map_err(|e| format!("Failed to run tar: {} (is tar installed?)", e))?;
            output
        }
        Kind::TarGz => {
            let output = std::process::Command::new("tar")
                .args([
                    "xzf",
                    &path.to_string_lossy(),
                    "-C",
                    &dest_dir.to_string_lossy(),
                ])
                .output()
                .map_err(|e| format!("Failed to run tar: {} (is tar installed?)", e))?;
            output
        }
        Kind::TarXz => {
            let output = std::process::Command::new("tar")
                .args([
                    "xJf",
                    &path.to_string_lossy(),
                    "-C",
                    &dest_dir.to_string_lossy(),
                ])
                .output()
                .map_err(|e| format!("Failed to run tar: {} (is tar installed?)", e))?;
            output
        }
        Kind::Zip => {
            let output = std::process::Command::new("unzip")
                .args([
                    "-o",
                    &path.to_string_lossy(),
                    "-d",
                    &dest_dir.to_string_lossy(),
                ])
                .output()
                .map_err(|e| format!("Failed to run unzip: {} (is unzip installed?)", e))?;
            output
        }
        Kind::SevenZ => {
            let dest_str = format!("-o{}", dest_dir.to_string_lossy());
            let output = std::process::Command::new("7z")
                .args(["x", &path.to_string_lossy(), &dest_str, "-y"])
                .output()
                .map_err(|e| format!("Failed to run 7z: {} (is p7zip installed?)", e))?;
            output
        }
    };

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!(
            "Extraction failed (exit {}): {}",
            result.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }

    Ok(())
}
