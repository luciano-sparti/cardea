use cardea::config::Config;
use cardea::fs::{FileEntry, FileKind};
use cardea::icons::{self, IconStyle};

fn entry(name: &str, kind: FileKind, is_dir: bool) -> FileEntry {
    let ext = std::path::Path::new(name)
        .extension()
        .map(|e| compact_str::CompactString::new(e.to_string_lossy()));
    FileEntry {
        name: compact_str::CompactString::new(name),
        path: std::path::PathBuf::from(name),
        is_dir,
        is_symlink: matches!(kind, FileKind::Symlink),
        symlink_target: None,
        size: 0,
        modified: None,
        permissions: 0o644,
        is_hidden: name.starts_with('.'),
        is_executable: false,
        extension: ext,
        kind,
    }
}

/// True when the glyph lives in a Unicode Private Use Area (Nerd Fonts).
fn is_pua(icon: &str) -> bool {
    icon.chars()
        .next()
        .map(|c| ('\u{E000}'..='\u{F8FF}').contains(&c) || ('\u{F0000}'..='\u{FFFFD}').contains(&c))
        .unwrap_or(false)
}

#[test]
fn test_icon_style_parse() {
    assert_eq!(IconStyle::parse("nerd"), Some(IconStyle::Nerd));
    assert_eq!(IconStyle::parse("Nerd-Font"), Some(IconStyle::Nerd));
    assert_eq!(IconStyle::parse("unicode"), Some(IconStyle::Unicode));
    assert_eq!(IconStyle::parse("ASCII"), Some(IconStyle::Ascii));
    assert_eq!(IconStyle::parse("emoji"), None);
    assert_eq!(IconStyle::resolve_config("ascii"), IconStyle::Ascii);
    // Unknown values fall back to Nerd rather than failing
    assert_eq!(IconStyle::resolve_config("bogus"), IconStyle::Nerd);
}

#[test]
fn test_file_icon_by_style() {
    let dir = entry("src", FileKind::Directory, true);
    assert_eq!(icons::file_icon(&dir, IconStyle::Ascii), "/");
    assert_eq!(icons::folder_icon(true, IconStyle::Ascii), "/");
    assert!(is_pua(icons::file_icon(&dir, IconStyle::Nerd)));
    assert_ne!(
        icons::file_icon(&dir, IconStyle::Nerd),
        icons::file_icon(&dir, IconStyle::Unicode)
    );

    let sym = entry("link", FileKind::Symlink, false);
    assert_eq!(icons::file_icon(&sym, IconStyle::Ascii), "@");
    assert_eq!(icons::file_icon(&sym, IconStyle::Unicode), "\u{1F517}");

    let exec = FileEntry {
        is_executable: true,
        ..entry("tool", FileKind::Regular, false)
    };
    assert_eq!(icons::file_icon(&exec, IconStyle::Ascii), "*");
    assert!(is_pua(icons::file_icon(&exec, IconStyle::Nerd)));
}

#[test]
fn test_file_icon_special_filenames() {
    let cargo_toml = entry("Cargo.toml", FileKind::Code, false);
    assert!(is_pua(icons::file_icon(&cargo_toml, IconStyle::Nerd)));

    let readme = entry("README.md", FileKind::Document, false);
    assert_eq!(icons::file_icon(&readme, IconStyle::Unicode), "\u{1F4D6}");

    let gitignore = entry(".gitignore", FileKind::Regular, false);
    assert_eq!(icons::file_icon(&gitignore, IconStyle::Ascii), "@");
}

#[test]
fn test_file_icon_by_extension() {
    let rs = entry("main.rs", FileKind::Code, false);
    assert_ne!(
        icons::file_icon(&rs, IconStyle::Nerd),
        icons::file_icon(&rs, IconStyle::Ascii)
    );

    let zip = entry("bundle.zip", FileKind::Archive, false);
    assert_eq!(icons::file_icon(&zip, IconStyle::Ascii), "#");

    let unknown = entry("blob.xyzzy", FileKind::Regular, false);
    assert_eq!(icons::file_icon(&unknown, IconStyle::Ascii), " ");

    let hidden = entry(".profile", FileKind::Regular, false);
    assert_eq!(icons::file_icon(&hidden, IconStyle::Ascii), ".");
}

#[test]
fn test_ls_colors_mapping() {
    use ratatui::style::{Color, Modifier};
    use std::path::Path;

    let lsc = lscolors::LsColors::from_string("*.tar=01;31:di=04;34");

    // Extension rule with bold + red (ANSI 31 → Indexed 1)
    let tar = icons::ls_colors_style_for(&lsc, Path::new("a/b.tar"), false, false).unwrap();
    assert_eq!(tar.fg, Some(Color::Indexed(1)));
    assert!(tar.add_modifier.contains(Modifier::BOLD));

    // Directory indicator fallback when no suffix matches
    let dir = icons::ls_colors_style_for(&lsc, Path::new("a/unknown-dir"), true, false).unwrap();
    assert_eq!(dir.fg, Some(Color::Indexed(4)));
    assert!(dir.add_modifier.contains(Modifier::UNDERLINED));

    // Executables resolve via their indicator
    let lsc2 = lscolors::LsColors::from_string("ex=01;32");
    let ex = icons::ls_colors_style_for(&lsc2, Path::new("script-thing"), false, true).unwrap();
    assert_eq!(ex.fg, Some(Color::Indexed(2)));

    // No matching rule → None so callers keep theme styling
    assert!(icons::ls_colors_style_for(&lsc, Path::new("plain.txt"), false, false).is_none());
}

#[test]
fn test_config_defaults_and_roundtrip() {
    let cfg = Config::default();
    assert_eq!(cfg.general.icon_style, "nerd");
    assert!(cfg.general.ls_colors_enabled);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut custom = Config::default();
    custom.general.icon_style = "unicode".to_string();
    custom.general.ls_colors_enabled = false;
    custom.save_to(&path).unwrap();

    let loaded = Config::load_from(&path);
    assert_eq!(loaded.general.icon_style, "unicode");
    assert!(!loaded.general.ls_colors_enabled);

    // Legacy files without the new fields load via serde defaults
    let legacy = dir.path().join("legacy.toml");
    std::fs::write(&legacy, "[general]\ntheme = \"nord\"\n").unwrap();
    let loaded = Config::load_from(&legacy);
    assert_eq!(loaded.general.theme, "nord");
    assert_eq!(loaded.general.icon_style, "nerd");
}
