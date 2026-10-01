use cardea::config::{Config, SortColumn, SortDirection};
use cardea::fs::{format_permissions, format_size, sort_entries, FileEntry, FileKind};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

/// Builds a FileEntry for an existing path via the scanner's dentry path.
fn entry_for(path: &Path) -> FileEntry {
    let parent = path.parent().unwrap();
    let name = path.file_name().unwrap();
    std::fs::read_dir(parent)
        .unwrap()
        .flatten()
        .find(|e| e.file_name() == name)
        .and_then(|e| FileEntry::from_dentry(&e))
        .expect("Entry should be parsed")
}

#[test]
fn test_format_size() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(500), "500 B");
    assert_eq!(format_size(1024), "1.0 KB");
    assert_eq!(format_size(1536), "1.5 KB");
    assert_eq!(format_size(1024 * 1024), "1.0 MB");
    assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
}

#[test]
fn test_format_permissions() {
    let dir_perms = format_permissions(0o755, true, false);
    assert_eq!(dir_perms, "drwxr-xr-x");

    let file_perms = format_permissions(0o644, false, false);
    assert_eq!(file_perms, "-rw-r--r--");

    let symlink_perms = format_permissions(0o777, false, true);
    assert_eq!(symlink_perms, "lrwxrwxrwx");
}

#[test]
fn test_natural_sorting() {
    let mut names = vec!["file10.txt", "file2.txt", "file1.txt", "file20.txt"];
    names.sort_by(|a, b| natord::compare(a, b));
    assert_eq!(
        names,
        vec!["file1.txt", "file2.txt", "file10.txt", "file20.txt"]
    );
}

#[test]
fn test_file_entry_from_disk() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("sample.rs");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "fn main() {{}}").unwrap();

    let entry = entry_for(&file_path);
    assert_eq!(entry.name.as_str(), "sample.rs");
    assert_eq!(entry.kind, FileKind::Code);
    assert!(!entry.is_dir);
    assert!(!entry.is_hidden);
    assert!(entry.size > 0);
}

#[test]
fn test_sorting_dirs_first() {
    let dir = tempdir().unwrap();
    let sub_dir = dir.path().join("alpha_dir");
    std::fs::create_dir(&sub_dir).unwrap();

    let file_path = dir.path().join("aaa_file.txt");
    File::create(&file_path).unwrap();

    let mut entries = vec![entry_for(&file_path), entry_for(&sub_dir)];

    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortDirection::Ascending,
        true,
        true,
    );

    // Directory should come first despite file name starting with aaa
    assert_eq!(entries[0].name.as_str(), "alpha_dir");
    assert_eq!(entries[1].name.as_str(), "aaa_file.txt");
}

#[test]
fn test_config_toml_roundtrip() {
    let config = Config::default();
    let serialized = toml::to_string(&config).expect("Must serialize");
    let deserialized: Config = toml::from_str(&serialized).expect("Must deserialize");

    assert_eq!(config.general.show_hidden, deserialized.general.show_hidden);
    assert_eq!(
        config.layout.sidebar_width_percent,
        deserialized.layout.sidebar_width_percent
    );
}

#[test]
fn test_batch_regex_rename() {
    let dir = tempdir().unwrap();
    let f1 = dir.path().join("img_01_raw.png");
    let f2 = dir.path().join("img_02_raw.png");
    File::create(&f1).unwrap();
    File::create(&f2).unwrap();

    let sources = vec![f1.clone(), f2.clone()];
    let res = cardea::fs::ops::batch_regex_rename(&sources, r"img_(\d+)_raw\.png", "photo_$1.png");
    assert!(res.is_ok());

    assert!(!f1.exists());
    assert!(!f2.exists());
    assert!(dir.path().join("photo_01.png").exists());
    assert!(dir.path().join("photo_02.png").exists());
}

#[test]
fn test_archive_creation_tar_gz_and_zip() {
    let dir = tempdir().unwrap();
    let doc = dir.path().join("doc.txt");
    let mut file = File::create(&doc).unwrap();
    writeln!(file, "Archive payload test").unwrap();

    let tar_gz_path = dir.path().join("out.tar.gz");
    let zip_path = dir.path().join("out.zip");

    assert!(cardea::fs::archive::create_archive(&tar_gz_path, std::slice::from_ref(&doc)).is_ok());
    assert!(tar_gz_path.exists());
    assert!(tar_gz_path.metadata().unwrap().len() > 0);

    assert!(cardea::fs::archive::create_archive(&zip_path, &[doc]).is_ok());
    assert!(zip_path.exists());
    assert!(zip_path.metadata().unwrap().len() > 0);

    let preview = cardea::fs::archive::preview_listing(&tar_gz_path);
    assert!(preview.is_some());
    assert!(preview.unwrap().contains("doc.txt"));
}

#[test]
fn test_git_status_detection() {
    let dir = tempdir().unwrap();
    let res = cardea::fs::git::find_git_root(dir.path());
    // In tempdir there is no .git
    assert!(res.is_none());

    // In current repo (cardea)
    let current = std::env::current_dir().unwrap();
    let res = cardea::fs::git::find_git_root(&current);
    assert!(res.is_some());

    let status = cardea::fs::git::query_git_status(&current);
    assert!(status.is_some());
    let status = status.unwrap();
    assert!(!status.branch.is_empty());
}
