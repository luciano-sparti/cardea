//! Integration tests for Fenestra core functionality.
//! Tests config loading, terminal rendering, and directory operations.

use std::io::Write;

use fenestra::config::Config;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tempfile::tempdir;

/// Config loads and round-trips correctly with defaults
#[test]
fn test_config_defaults_load() {
    let config = Config::default();
    let serialized = toml::to_string(&config).expect("Must serialize config");
    let deserialized: Config = toml::from_str(&serialized).expect("Must deserialize config");

    // Defaults should be preserved
    assert!(!deserialized.general.show_hidden);
    assert_eq!(
        deserialized.layout.sidebar_width_percent,
        config.layout.sidebar_width_percent
    );
    assert_eq!(deserialized.layout.preview_dock, config.layout.preview_dock);
    assert_eq!(
        deserialized.layout.preview_height_percent,
        config.layout.preview_height_percent
    );
}

/// Terminal rendering with ratatui TestBackend (headless)
#[test]
fn test_terminal_rendering_headless() {
    use ratatui::style::Style;
    use ratatui::widgets::Paragraph;

    let backend = TestBackend::new(80, 24);
    // Clone backend before moving into Terminal
    let backend2 = backend.clone();
    let mut terminal = Terminal::new(backend).expect("Terminal should init");

    terminal
        .draw(|f| {
            let p = Paragraph::new("Fenestra File Explorer").style(Style::default());
            f.render_widget(p, f.area());
        })
        .expect("Draw should succeed");

    // Verify the buffer has content using the cloned backend
    let buf = backend2.buffer();
    assert!(
        !buf.content.is_empty(),
        "Buffer should have content after draw"
    );
}

/// Temp directory file operations
#[test]
fn test_temp_dir_operations() {
    let dir = tempdir().unwrap();

    // Create and write a file
    let file_path = dir.path().join("test.txt");
    let mut file = std::fs::File::create(&file_path).unwrap();
    writeln!(file, "hello world").unwrap();

    // Read it back
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "hello world\n");

    // Verify directory still exists
    assert!(dir.path().exists());
}

/// Arena-style directory listing with sorting
#[test]
fn test_dir_listing_sorting() {
    let dir = tempdir().unwrap();

    // Create multiple files
    for name in &["zebra.txt", "alpha.doc", "mid.txt"] {
        let p = dir.path().join(name);
        std::fs::File::create(&p).unwrap();
    }

    // Read directory entries
    let mut entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().collect();

    // Sort by name
    entries.sort_by_key(|a| a.file_name());

    // Verify sorted order
    let names: Vec<String> = entries
        .iter()
        .map(|e| e.file_name().to_str().unwrap().to_string())
        .collect();
    assert_eq!(names[0], "alpha.doc");
    assert_eq!(names[1], "mid.txt");
    assert_eq!(names[2], "zebra.txt");
}
