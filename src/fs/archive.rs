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
                .args([
                    "xf",
                    &path.to_string_lossy(),
                    "-C",
                    &dest_dir.to_string_lossy(),
                ])
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

/// Create an archive from a list of source files or directories.
pub fn create_archive(dest_path: &Path, source_paths: &[std::path::PathBuf]) -> Result<(), String> {
    let name = dest_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Invalid destination archive name".to_string())?
        .to_lowercase();

    let kind = detect_kind(&name).ok_or_else(|| {
        "Unsupported archive format. Use .zip, .tar.gz, .tar.xz, or .tar".to_string()
    })?;

    match kind {
        Kind::Zip => create_zip(dest_path, source_paths),
        Kind::Tar => create_tar(dest_path, source_paths),
        Kind::TarGz => create_tar_gz(dest_path, source_paths),
        Kind::TarXz => create_tar_xz(dest_path, source_paths),
        Kind::SevenZ => Err("7z archive creation is not supported; use .zip or .tar.gz".to_string()),
    }
}

fn create_zip(dest: &Path, sources: &[std::path::PathBuf]) -> Result<(), String> {
    let file = File::create(dest).map_err(|e| format!("Failed to create zip: {}", e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for src in sources {
        let file_name = src.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        if src.is_file() {
            zip.start_file(file_name, options)
                .map_err(|e| format!("Failed to add file to zip: {}", e))?;
            let mut f = File::open(src).map_err(|e| format!("Failed to read {}: {}", src.display(), e))?;
            std::io::copy(&mut f, &mut zip)
                .map_err(|e| format!("Failed to write to zip: {}", e))?;
        } else if src.is_dir() {
            add_dir_to_zip(&mut zip, src, Path::new(file_name), options)?;
        }
    }
    zip.finish().map_err(|e| format!("Failed to finalize zip: {}", e))?;
    Ok(())
}

fn add_dir_to_zip<W: std::io::Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    src_dir: &Path,
    prefix: &Path,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    for entry in std::fs::read_dir(src_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let rel = prefix.join(entry.file_name());
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if path.is_file() {
            zip.start_file(&rel_str, options)
                .map_err(|e| format!("Failed to add to zip: {}", e))?;
            let mut f = File::open(&path).map_err(|e| e.to_string())?;
            std::io::copy(&mut f, zip).map_err(|e| e.to_string())?;
        } else if path.is_dir() {
            zip.add_directory(format!("{}/", rel_str), options).ok();
            add_dir_to_zip(zip, &path, &rel, options)?;
        }
    }
    Ok(())
}

fn create_tar_gz(dest: &Path, sources: &[std::path::PathBuf]) -> Result<(), String> {
    let file = File::create(dest).map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;
    let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);

    for src in sources {
        let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("entry");
        if src.is_file() {
            let mut f = File::open(src).map_err(|e| format!("Failed to open {}: {}", src.display(), e))?;
            tar.append_file(name, &mut f).map_err(|e| format!("Failed to add {}: {}", src.display(), e))?;
        } else if src.is_dir() {
            tar.append_dir_all(name, src).map_err(|e| format!("Failed to add dir {}: {}", src.display(), e))?;
        }
    }
    tar.finish().map_err(|e| format!("Failed to finish tar.gz: {}", e))?;
    Ok(())
}

fn create_tar_xz(dest: &Path, sources: &[std::path::PathBuf]) -> Result<(), String> {
    let file = File::create(dest).map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;
    let enc = xz2::write::XzEncoder::new(file, 6);
    let mut tar = tar::Builder::new(enc);

    for src in sources {
        let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("entry");
        if src.is_file() {
            let mut f = File::open(src).map_err(|e| format!("Failed to open {}: {}", src.display(), e))?;
            tar.append_file(name, &mut f).map_err(|e| format!("Failed to add {}: {}", src.display(), e))?;
        } else if src.is_dir() {
            tar.append_dir_all(name, src).map_err(|e| format!("Failed to add dir {}: {}", src.display(), e))?;
        }
    }
    tar.finish().map_err(|e| format!("Failed to finish tar.xz: {}", e))?;
    Ok(())
}

fn create_tar(dest: &Path, sources: &[std::path::PathBuf]) -> Result<(), String> {
    let file = File::create(dest).map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;
    let mut tar = tar::Builder::new(file);

    for src in sources {
        let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("entry");
        if src.is_file() {
            let mut f = File::open(src).map_err(|e| format!("Failed to open {}: {}", src.display(), e))?;
            tar.append_file(name, &mut f).map_err(|e| format!("Failed to add {}: {}", src.display(), e))?;
        } else if src.is_dir() {
            tar.append_dir_all(name, src).map_err(|e| format!("Failed to add dir {}: {}", src.display(), e))?;
        }
    }
    tar.finish().map_err(|e| format!("Failed to finish tar: {}", e))?;
    Ok(())
}
