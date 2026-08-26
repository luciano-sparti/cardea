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
