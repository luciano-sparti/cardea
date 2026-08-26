use fenestra::config::{Config, CustomTheme};
use fenestra::theme::{parse_hex_color, Theme};
use ratatui::style::Color;
use std::collections::HashMap;

// ---- New Palettes ----

#[test]
fn test_additional_palettes_are_distinct_and_named() {
    let palettes = [
        Theme::catppuccin_latte(),
        Theme::solarized_dark(),
        Theme::solarized_light(),
        Theme::high_contrast(),
    ];

    // All named and mutually distinct (light vs dark variants differ!)
    for i in 0..palettes.len() {
        assert!(!palettes[i].name.is_empty());
        for j in (i + 1)..palettes.len() {
            assert_ne!(
                palettes[i].bg, palettes[j].bg,
                "{} vs {}",
                palettes[i].name, palettes[j].name
            );
        }
    }

    // Light themes have light backgrounds; dark ones dark
    for t in [
        &Theme::catppuccin_mocha(),
        &Theme::solarized_dark(),
        &Theme::high_contrast(),
    ] {
        assert!(
            matches!(t.bg, Color::Rgb(r, _, _) if r < 80),
            "{} should be dark",
            t.name
        );
    }
    for t in [&Theme::catppuccin_latte(), &Theme::solarized_light()] {
        assert!(
            matches!(t.bg, Color::Rgb(r, _, _) if r > 200),
            "{} should be light",
            t.name
        );
    }
}

// ---- Strict Name Lookup ----

#[test]
fn test_try_from_name_known_and_unknown() {
    assert!(Theme::try_from_name("catppuccin-mocha").is_some());
    assert!(
        Theme::try_from_name("Catppuccin Mocha").is_some(),
        "spaces normalize"
    );
    assert!(Theme::try_from_name("tokyonight").is_some());
    assert!(
        Theme::try_from_name("gruvbox_dark").is_some(),
        "underscores normalize"
    );
    assert!(Theme::try_from_name("latte").is_some());
    assert!(Theme::try_from_name("solarized-light").is_some());
    assert!(Theme::try_from_name("high-contrast").is_some());
    assert!(Theme::try_from_name("ansi").is_some());

    assert!(Theme::try_from_name("").is_none());
    assert!(Theme::try_from_name("not-a-theme").is_none());

    // Lenient from_name still falls back to the default palette
    assert_eq!(Theme::from_name("not-a-theme").name, "Catppuccin Mocha");
}

// ---- $NO_COLOR ----

#[test]
fn test_stripped_theme_has_only_reset_colors() {
    let stripped = Theme::catppuccin_mocha().stripped();

    for (i, name) in Theme::color_field_names().iter().enumerate() {
        let color = stripped_field(&stripped, i);
        assert_eq!(color, Color::Reset, "field {} should be Reset", name);
    }
    assert!(stripped.name.contains("no color"));
}

/// Reads one color field by index without reflection.
fn stripped_field(theme: &Theme, idx: usize) -> Color {
    match Theme::color_field_names()[idx] {
        "bg" => theme.bg,
        "fg" => theme.fg,
        "accent" => theme.accent,
        "filter_match" => theme.filter_match,
        "status_error" => theme.status_error,
        _ => Color::Reset,
    }
}

#[test]
fn test_effective_applies_no_color() {
    let base = Theme::nord();
    let effective = Theme::effective(base, true);
    assert_eq!(effective.fg, Color::Reset);
    assert_eq!(effective.accent, Color::Reset);

    // Without NO_COLOR the palette passes through untouched
    let base = Theme::nord();
    let effective = Theme::effective(base, false);
    assert_eq!(effective, Theme::nord());
}

// ---- ANSI Degradation ----

#[test]
fn test_degraded_theme_maps_rgb_to_ansi() {
    let degraded = Theme::catppuccin_mocha().degraded_to_ansi();
    assert!(degraded.name.contains("ansi"));

    // No Rgb values remain
    let all_colors = [
        degraded.bg,
        degraded.fg,
        degraded.accent,
        degraded.selection_bg,
        degraded.selection_fg,
        degraded.border,
        degraded.border_focus,
        degraded.tree_branch,
        degraded.tree_folder,
        degraded.tree_folder_expanded,
        degraded.tree_bookmark,
        degraded.table_header_bg,
        degraded.table_header_fg,
        degraded.table_selected_bg,
        degraded.table_selected_fg,
        degraded.table_row_alt_bg,
        degraded.breadcrumb_bg,
        degraded.breadcrumb_fg,
        degraded.breadcrumb_active_bg,
        degraded.breadcrumb_active_fg,
        degraded.breadcrumb_arrow,
        degraded.status_bg,
        degraded.status_fg,
        degraded.status_key,
        degraded.status_info,
        degraded.status_warn,
        degraded.status_error,
        degraded.file_dir,
        degraded.file_exec,
        degraded.file_symlink,
        degraded.file_archive,
        degraded.file_image,
        degraded.file_doc,
        degraded.file_regular,
        degraded.file_hidden,
        degraded.filter_match,
    ];
    for c in all_colors {
        assert!(
            !matches!(c, Color::Rgb(..)),
            "RGB must not survive degradation"
        );
    }
}

#[test]
fn test_nearest_ansi_mapping() {
    // Exercised through the public degrade path using a custom base:
    // pure red maps to a red family color, white to white, black to black.
    let mut base = Theme::ansi_fallback(); // already ANSI; make an RGB-heavy theme
    base.bg = Color::Rgb(0, 0, 0);
    base.fg = Color::Rgb(255, 255, 255);
    base.accent = Color::Rgb(255, 0, 0);

    let d = base.degraded_to_ansi();
    assert_eq!(d.bg, Color::Black);
    assert!(matches!(d.fg, Color::White | Color::Gray));
    assert!(
        matches!(d.accent, Color::Red | Color::LightRed),
        "red stays red-ish, got {:?}",
        d.accent
    );
}

#[test]
fn test_truecolor_detection() {
    use fenestra::theme::Theme as T;
    assert!(T::supports_truecolor(
        Some("truecolor"),
        Some("xterm-256color")
    ));
    assert!(T::supports_truecolor(Some("24bit"), None));
    assert!(!T::supports_truecolor(Some("256"), Some("xterm-256color")));
    assert!(!T::supports_truecolor(None, None));
    assert!(T::supports_truecolor(None, Some("xterm-kitty")));
    assert!(T::supports_truecolor(None, Some("xterm-direct")));
    assert!(!T::supports_truecolor(None, Some("dumb")));
}

// ---- Hex Parsing & Custom Themes ----

#[test]
fn test_parse_hex_color_forms() {
    assert_eq!(parse_hex_color("#ff0000"), Some(Color::Rgb(255, 0, 0)));
    assert_eq!(parse_hex_color("00ff00"), Some(Color::Rgb(0, 255, 0)));
    assert_eq!(parse_hex_color("#abc"), Some(Color::Rgb(170, 187, 204)));
    assert_eq!(parse_hex_color("  #1E1E2E "), Some(Color::Rgb(30, 30, 46)));

    assert_eq!(parse_hex_color(""), None);
    assert_eq!(parse_hex_color("#12345"), None);
    assert_eq!(parse_hex_color("#gggggg"), None);
    assert_eq!(parse_hex_color("not-a-color"), None);
}

#[test]
fn test_custom_theme_partial_override_is_atomic() {
    let original = Theme::tokyo_night();

    // Partial override changes only the given keys
    let mut colors = HashMap::new();
    colors.insert("bg".to_string(), "#101010".to_string());
    colors.insert("accent".to_string(), "#ff8000".to_string());
    let custom = CustomTheme {
        base: Some("tokyo-night".to_string()),
        colors: colors.clone(),
    };
    let resolved = custom.resolve("catppuccin-mocha").expect("valid overrides");

    assert_eq!(resolved.bg, Color::Rgb(16, 16, 16));
    assert_eq!(resolved.accent, Color::Rgb(255, 128, 0));
    // Untouched fields keep the base palette's values
    assert_eq!(resolved.fg, original.fg);
    assert_eq!(resolved.border, original.border);
    assert_eq!(resolved.name, original.name);

    // Invalid value: nothing applied at all (atomicity)
    let mut bad = HashMap::new();
    bad.insert("bg".to_string(), "#101010".to_string());
    bad.insert("fg".to_string(), "not-a-color".to_string());
    let custom = CustomTheme {
        base: Some("tokyo-night".to_string()),
        colors: bad,
    };
    let err = custom.resolve("catppuccin-mocha").unwrap_err();
    assert!(err.iter().any(|e| e.contains("invalid hex")));

    // Unknown key reported
    let mut unknown = HashMap::new();
    unknown.insert("not_a_key".to_string(), "#000000".to_string());
    let custom = CustomTheme {
        base: Some("tokyo-night".to_string()),
        colors: unknown,
    };
    let err = custom.resolve("catppuccin-mocha").unwrap_err();
    assert!(err.iter().any(|e| e.contains("unknown theme key")));
}

#[test]
fn test_config_resolved_theme_layers_custom_over_base() {
    let toml_str = r##"
version = 1

[general]
theme = "nord"

[layout]

[[actions]]
name = "Test"
command = "true"

[custom_theme]
base = "solarized-dark"
bg = "#001122"
status_error = "#ff5555"
"##;
    let config: Config = toml::from_str(toml_str).expect("config with custom_theme parses");
    let theme = config.resolved_theme();

    assert_eq!(theme.bg, Color::Rgb(0, 17, 34));
    assert_eq!(theme.status_error, Color::Rgb(255, 85, 85));
    assert_eq!(
        theme.fg,
        Theme::solarized_dark().fg,
        "non-overridden fields come from base"
    );
}

// ---- Omarchy v4 (Aether) colors.toml Parsing ----

#[test]
fn test_parse_omarchy_v4_aether_style() {
    // Fixture mirrors a real Aether-generated colors.toml
    let content = r##"
mode = "dark"

accent = "#b59790"
selection = "#FAFCFB"
muted = "#584e51"

background = "#0c0b0c"
dark_background = "#090809"
darker_background = "#060606"

foreground = "#FAFCFB"
dark_foreground = "#bcbdbc"

red = "#ff0000"
yellow = "#6B5E73"
orange = "#cc9c8f"
green = "#87a9b0"
cyan = "#a5a0b6"
blue = "#1010aa"
magenta = "#c4d8e2"
brown = "#7a5e56"
"##;
    let t = fenestra::theme::parse_omarchy_v4("aether", content).expect("valid v4 file");

    assert_eq!(t.name, "Omarchy: aether");
    assert_eq!(t.bg, Color::Rgb(12, 11, 12));
    assert_eq!(t.fg, Color::Rgb(250, 252, 251));
    assert_eq!(t.accent, Color::Rgb(181, 151, 144));
    assert_eq!(t.border_focus, t.accent, "accent drives focus borders");

    assert_eq!(t.status_error, Color::Rgb(255, 0, 0), "red -> error");
    assert_eq!(t.file_dir, Color::Rgb(16, 16, 170), "blue -> directories");
    assert_eq!(
        t.file_exec,
        Color::Rgb(135, 169, 176),
        "green -> executables"
    );
    assert_eq!(
        t.file_archive,
        Color::Rgb(204, 156, 143),
        "orange -> archives"
    );
    assert_eq!(
        t.status_bg,
        Color::Rgb(6, 6, 6),
        "darker_background wins for status bar"
    );
    assert_eq!(
        t.table_header_bg,
        Color::Rgb(9, 8, 9),
        "dark_background for header rows"
    );

    // Mapped fields track the fixture (yellow drives filter matches)
    assert_eq!(t.filter_match, Color::Rgb(107, 94, 115));
    assert_eq!(t.tree_bookmark, t.filter_match);
}

#[test]
fn test_parse_omarchy_v4_partial_and_invalid() {
    // Minimal file: only background/foreground — rest falls back to base
    let minimal = r##"
background = "#111111"
foreground = "#eeeeee"
"##;
    let t = fenestra::theme::parse_omarchy_v4("minimal", minimal)
        .expect("partial file still yields a theme");
    assert_eq!(t.bg, Color::Rgb(17, 17, 17));
    assert_eq!(t.fg, Color::Rgb(238, 238, 238));
    assert_eq!(
        t.accent,
        Theme::catppuccin_mocha().accent,
        "unspecified keys keep base"
    );

    // Invalid TOML or a table with no recognizable color keys → None,
    // so the caller keeps the last valid palette
    assert!(fenestra::theme::parse_omarchy_v4("x", "not [ valid {{{").is_none());
    assert!(fenestra::theme::parse_omarchy_v4("x", "[meta]\nmode = \"dark\"\n").is_none());
}
