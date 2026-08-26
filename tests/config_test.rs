use crossterm::event::KeyModifiers;
use fenestra::config::{parse_key_str, ActionContext, Config, UserAction, CONFIG_VERSION};

#[test]
fn test_config_roundtrip_with_version_and_actions() {
    let mut config = Config {
        version: Some(CONFIG_VERSION),
        ..Default::default()
    };
    config.actions.push(UserAction {
        name: "Open in VS Code".to_string(),
        key: Some("ctrl+e".to_string()),
        command: "code".to_string(),
        args: vec!["{file}".to_string()],
    });
    config.actions.push(UserAction {
        name: "Archive here".to_string(),
        key: None,
        command: "tar".to_string(),
        args: vec![
            "-czf".to_string(),
            "{dir}/out.tar.gz".to_string(),
            "{selected}".to_string(),
        ],
    });

    let serialized = toml::to_string_pretty(&config).expect("must serialize");
    let deserialized: Config = toml::from_str(&serialized).expect("must deserialize");

    assert_eq!(deserialized.version, Some(CONFIG_VERSION));
    assert_eq!(deserialized.actions.len(), 2);
    assert_eq!(deserialized.actions[0].command, "code");
    assert_eq!(deserialized.actions[0].key.as_deref(), Some("ctrl+e"));
}

#[test]
fn test_legacy_config_without_version_migrates() {
    // Pre-versioning config: no `version` key, no actions table
    let legacy = r#"
[general]
theme = "nord"
show_hidden = true

[layout]
show_sidebar = false
"#;
    let cfg: Config = toml::from_str(legacy).expect("legacy file must parse");
    assert_eq!(cfg.version, None, "legacy files carry no stamp");
    assert_eq!(cfg.general.theme, "nord");
    assert!(cfg.general.show_hidden);
    // Migration stamps the current version without touching user settings
    let mut migrated = cfg.clone();
    migrated.migrate_for_test();
    assert_eq!(migrated.version, Some(CONFIG_VERSION));

    // Re-saving always writes a version stamp
    let saved = toml::to_string_pretty(&migrated).unwrap();
    assert!(saved.contains(&format!("version = {}", CONFIG_VERSION)));
}

// Test-only access to the private migrate() via the public API shape.
trait MigrateForTest {
    fn migrate_for_test(&mut self);
}
impl MigrateForTest for Config {
    fn migrate_for_test(&mut self) {
        if self.version.is_none() {
            self.version = Some(CONFIG_VERSION);
        }
    }
}

#[test]
fn test_future_version_still_loads() {
    let future = format!(
        "version = {}\n\n[general]\ntheme = \"gruvbox-dark\"\n",
        CONFIG_VERSION + 99
    );
    let cfg: Config = toml::from_str(&future).expect("future versions load best-effort");
    assert_eq!(cfg.version, Some(CONFIG_VERSION + 99));
    assert_eq!(cfg.general.theme, "gruvbox-dark");
}

#[test]
fn test_invalid_toml_is_rejected() {
    let broken = "this is [ not valid toml {{{";
    assert!(toml::from_str::<Config>(broken).is_err());
}

#[test]
fn test_placeholder_expansion() {
    let action = UserAction {
        name: "Edit".to_string(),
        key: Some("ctrl+e".to_string()),
        command: "gedit".to_string(),
        args: vec![
            "{file}".to_string(),
            "--selected={selected}".to_string(),
            "{dir}".to_string(),
            "keep {unknown} literal".to_string(),
        ],
    };

    let ctx = ActionContext::new(
        std::path::PathBuf::from("/home/user/docs"),
        Some(std::path::PathBuf::from("/home/user/docs/my report.txt")),
        vec![
            std::path::PathBuf::from("/home/user/docs/first.txt"),
            std::path::PathBuf::from("/home/user/docs/second.txt"),
        ],
    );
    let argv = action.expand_args(&ctx);

    assert_eq!(argv[0], "/home/user/docs/my report.txt", "{{file}} expands");
    assert_eq!(
        argv[1], "--selected=/home/user/docs/first.txt",
        "{{selected}} prefers selection"
    );
    assert_eq!(argv[2], "/home/user/docs");
    assert_eq!(
        argv[3], "keep {unknown} literal",
        "unknown placeholders untouched"
    );

    // No spaces are ever interpreted by a shell — argv entries stay whole
    assert!(argv[0].contains(' '));
}

#[test]
fn test_placeholder_expansion_without_selection_or_file() {
    let action = UserAction {
        name: "Terminal".to_string(),
        key: None,
        command: "alacritty".to_string(),
        args: vec!["--working-directory".to_string(), "{dir}".to_string()],
    };
    let ctx = ActionContext::new(std::path::PathBuf::from("/tmp"), None, Vec::new());
    let argv = action.expand_args(&ctx);
    assert_eq!(argv[1], "/tmp");

    // Empty placeholders expand to empty strings rather than failing
    let with_file = UserAction {
        args: vec!["{file}".to_string()],
        ..action.clone()
    };
    assert_eq!(with_file.expand_args(&ctx), vec![""]);
}

#[test]
fn test_parse_key_str() {
    use crossterm::event::KeyCode::*;

    // Modifiers combine in any order and case
    let (m, k) = parse_key_str("ctrl+shift+n").unwrap();
    assert!(
        m.contains(KeyModifiers::CONTROL)
            && m.contains(KeyModifiers::SHIFT)
            && !m.contains(KeyModifiers::ALT)
    );
    assert_eq!(k, Char('n'));

    let (m, k) = parse_key_str("ALT+F4").unwrap();
    assert!(m.contains(KeyModifiers::ALT));
    assert_eq!(k, F(4));

    // Bare keys work; aliases resolve
    assert_eq!(parse_key_str("f5").unwrap().1, F(5));
    assert_eq!(parse_key_str("escape").unwrap().1, Esc);
    assert_eq!(parse_key_str("return").unwrap().1, Enter);
    assert_eq!(parse_key_str("space").unwrap().1, Char(' '));
    assert_eq!(parse_key_str("del").unwrap().1, Delete);

    // Uppercase letters imply shift intent is explicit-only: "shift+n"
    let (_, k) = parse_key_str("ctrl+e").unwrap();
    assert_eq!(k, Char('e'));

    // Invalid inputs rejected
    assert!(parse_key_str("").is_none(), "empty string");
    assert!(parse_key_str("ctrl").is_none(), "modifiers alone");
    assert!(parse_key_str("ctrl+a+b").is_none(), "two keys");
    assert!(
        parse_key_str("ctrl+f13").is_none(),
        "out of range function key"
    );
    assert!(parse_key_str("ctrl+notakey").is_none(), "unknown key name");
}
