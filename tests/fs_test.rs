use fenestra::config::{Config, SortColumn, SortDirection};
use fenestra::fs::{format_permissions, format_size, sort_entries, FileEntry, FileKind};
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
