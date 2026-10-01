use std::path::{Path, PathBuf};

pub fn move_to_trash(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|e| format!("Failed to move {:?} to trash: {}", path, e))
}

pub fn delete_permanently(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
            .map_err(|e| format!("Failed to delete directory {:?}: {}", path, e))
    } else {
        std::fs::remove_file(path).map_err(|e| format!("Failed to delete file {:?}: {}", path, e))
    }
}

pub fn create_directory(parent: &Path, name: &str) -> Result<PathBuf, String> {
    let new_path = parent.join(name);
    if new_path.exists() {
        return Err(format!("Destination already exists: {:?}", new_path));
    }

    std::fs::create_dir(&new_path)
        .map_err(|e| format!("Failed to create folder {:?}: {}", new_path, e))?;

    Ok(new_path)
}

pub fn create_file(parent: &Path, name: &str) -> Result<PathBuf, String> {
    let new_path = parent.join(name);
    if new_path.exists() {
        return Err(format!("Destination already exists: {:?}", new_path));
    }

    std::fs::File::create(&new_path)
        .map_err(|e| format!("Failed to create file {:?}: {}", new_path, e))?;

    Ok(new_path)
}

pub fn rename_entry(from: &Path, to_name: &str) -> Result<PathBuf, String> {
    let parent = from.parent().unwrap_or_else(|| Path::new("/"));
    let target = parent.join(to_name);

    if target.exists() {
        return Err(format!("Target already exists: {:?}", target));
    }

    std::fs::rename(from, &target)
        .map_err(|e| format!("Failed to rename {:?} to {:?}: {}", from, target, e))?;

    Ok(target)
}

pub fn batch_regex_rename(
    sources: &[PathBuf],
    pattern: &str,
    replacement: &str,
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let re = regex::Regex::new(pattern).map_err(|e| format!("Invalid regex pattern: {}", e))?;

    let mut planned = Vec::new();
    let mut targets_seen = std::collections::HashSet::new();

    for src in sources {
        let parent = src.parent().unwrap_or_else(|| Path::new("/"));
        let old_name = src
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("Invalid file name: {:?}", src))?;

        let new_name = re.replace_all(old_name, replacement).to_string();
        if new_name == old_name {
            continue; // No change
        }

        let target = parent.join(&new_name);

        if target.exists() && !sources.contains(&target) {
            return Err(format!("Destination file already exists: {:?}", target));
        }

        if !targets_seen.insert(target.clone()) {
            return Err(format!("Collision: multiple files rename to {:?}", target));
        }

        planned.push((src.clone(), target));
    }

    if planned.is_empty() {
        return Err("No files matched the regex pattern or required changes".to_string());
    }

    // Execute renames
    let mut completed = Vec::new();
    for (src, target) in planned {
        std::fs::rename(&src, &target)
            .map_err(|e| format!("Failed to rename {:?} to {:?}: {}", src, target, e))?;
        completed.push((src, target));
    }

    Ok(completed)
}

pub fn compress_entries(dest_archive: &Path, sources: &[PathBuf]) -> Result<PathBuf, String> {
    crate::fs::archive::create_archive(dest_archive, sources)?;
    Ok(dest_archive.to_path_buf())
}
