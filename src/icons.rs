//! File and folder iconography with three rendering styles:
//! - [`IconStyle::Nerd`]: Nerd Fonts v3 glyphs (default; Omarchy ships these)
//! - [`IconStyle::Unicode`]: narrow symbols for non-Nerd setups
//! - [`IconStyle::Ascii`]: `ls -F` style plain-text markers for any terminal
//!
//! Selection order in [`file_icon`]: well-known filenames (README, Makefile,
//! Cargo.toml, ...) -> extension map -> [`FileKind`] fallback. All glyphs are
//! written as unicode escapes so the source stays ASCII-clean.

use crate::fs::{FileEntry, FileKind};
use ratatui::style::{Color, Modifier, Style};
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconStyle {
    #[default]
    Nerd,
    Unicode,
    Ascii,
}

impl IconStyle {
    /// Parses a config string ("nerd" | "unicode" | "ascii"); case-insensitive.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "nerd" | "nerdfont" | "nerd-font" | "nerd_fonts" => Some(Self::Nerd),
            "unicode" | "utf8" | "symbols" => Some(Self::Unicode),
            "ascii" | "plain" | "text" | "ls_f" => Some(Self::Ascii),
            _ => None,
        }
    }

    /// Resolves a configured value, falling back to Nerd with a warning on
    /// unknown names.
    pub fn resolve_config(value: &str) -> Self {
        match Self::parse(value) {
            Some(style) => style,
            None => {
                tracing::warn!("Unknown icon_style '{value}'; falling back to 'nerd'");
                Self::Nerd
            }
        }
    }
}

// Nerd Fonts v3 glyphs used outside the per-extension tables

/// Directory glyph; `expanded` picks the open variant where available.
pub fn folder_icon(expanded: bool, style: IconStyle) -> &'static str {
    match style {
        IconStyle::Nerd => {
            if expanded {
                "\u{f115}"
            } else {
                "\u{e5ff}"
            }
        }
        IconStyle::Unicode => {
            if expanded {
                "\u{1F4C2}"
            } else {
                "\u{1F4C1}"
            }
        }
        IconStyle::Ascii => "/",
    }
}

/// Quick-access bookmark glyph resolved from the bookmark's display name.
pub fn bookmark_icon(name: &str, style: IconStyle) -> &'static str {
    let key = name.to_lowercase();
    match style {
        IconStyle::Nerd => match key.as_str() {
            "home" => "\u{f015}",
            "documents" | "docs" => "\u{f0f6}",
            "downloads" => "\u{f019}",
            "projects" => "\u{f121}",
            "root" => "\u{f0a0}",
            _ => "\u{e5ff}",
        },
        IconStyle::Unicode => match key.as_str() {
            "home" => "\u{2302}",
            "root" => "\u{2442}",
            _ => "\u{1F4C1}",
        },
        IconStyle::Ascii => match key.as_str() {
            "root" => "/",
            _ => "~",
        },
    }
}

/// Breadcrumb segment glyph for a path component.
pub fn breadcrumb_icon(is_root: bool, is_home: bool, style: IconStyle) -> &'static str {
    if is_root {
        return match style {
            IconStyle::Nerd => "\u{f0a0}",
            IconStyle::Unicode => "\u{2442}",
            IconStyle::Ascii => "/",
        };
    }
    if is_home {
        return match style {
            IconStyle::Nerd => "\u{f015}",
            IconStyle::Unicode => "\u{2302}",
            IconStyle::Ascii => "~",
        };
    }
    folder_icon(false, style)
}

/// Checkbox glyph for multi-selection rows.
pub fn checked_box_icon(style: IconStyle) -> &'static str {
    match style {
        IconStyle::Nerd => "\u{f046}",
        IconStyle::Unicode => "\u{2714}",
        IconStyle::Ascii => "x",
    }
}

/// Unchecked box glyph for multi-selection rows.
pub fn unchecked_box_icon(style: IconStyle) -> &'static str {
    match style {
        IconStyle::Nerd => "\u{f096}",
        IconStyle::Unicode => "\u{25a2}",
        IconStyle::Ascii => " ",
    }
}

/// Unicode fallback glyphs by category.
fn uni_glyph(category: UniCat, is_hidden: bool) -> &'static str {
    match category {
        UniCat::Symlink => "\u{1F517}",
        UniCat::Exec => "\u{2699}",
        UniCat::Archive => "\u{1F4E6}",
        UniCat::Image => "\u{1F5BC}",
        UniCat::Audio => "\u{266A}",
        UniCat::Video => "\u{1F3AC}",
        UniCat::Doc => "\u{1F4C4}",
        UniCat::Table => "\u{1F5C2}",
        UniCat::Book => "\u{1F4D6}",
        UniCat::Code => "\u{2328}",
        UniCat::Config => "\u{2699}",
        UniCat::Db => "\u{1F4BE}",
        UniCat::Font => "\u{1F524}",
        UniCat::Lock => "\u{1F512}",
        UniCat::Regular => {
            if is_hidden {
                "\u{00B7}"
            } else {
                "\u{25AB}"
            }
        }
    }
}

#[allow(dead_code)]
enum UniCat {
    Symlink,
    Exec,
    Archive,
    Image,
    Audio,
    Video,
    Doc,
    Table,
    Book,
    Code,
    Config,
    Db,
    Font,
    Lock,
    Regular,
}

/// Well-known exact filenames (case-insensitive, includes dotfiles).
fn special_file_icon(name_lower: &str, style: IconStyle) -> Option<&'static str> {
    // Nerd style has its own generated table
    if matches!(style, IconStyle::Nerd) {
        return nerd_special_icon(name_lower);
    }
    let uni = matches!(style, IconStyle::Unicode);
    Some(match name_lower {
        "readme.md" | "readme" | "license" | "licence" | "copying" | "authors" | "contributors"
        | "changelog" | "notice" => {
            if uni {
                "\u{1F4D6}"
            } else {
                "-"
            }
        }
        "makefile" | "dockerfile" | "justfile" | "cmakelists.txt" => {
            if uni {
                "\u{2699}"
            } else {
                "*"
            }
        }
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitconfig" => {
            if uni {
                "\u{1F517}"
            } else {
                "@"
            }
        }
        "cargo.lock" => {
            if uni {
                "\u{1F512}"
            } else {
                "="
            }
        }
        "cargo.toml" | "package.json" | "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock" => {
            if uni {
                "\u{2699}"
            } else {
                "$"
            }
        }
        ".env" | ".env.local" | ".envrc" => {
            if uni {
                "\u{2699}"
            } else {
                "&"
            }
        }
        _ => return None,
    })
}

/// Extension-based icon table (lowercase extension, no leading dot).
fn ext_icon(ext: &str, style: IconStyle) -> Option<&'static str> {
    if matches!(style, IconStyle::Nerd) {
        return nerd_ext_icon(ext);
    }
    let uni = matches!(style, IconStyle::Unicode);

    Some(match ext {
        "rs" | "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "go" | "zig" | "py" | "js"
        | "mjs" | "cjs" | "ts" | "jsx" | "tsx" | "lua" | "vim" | "nvim" | "pl" | "pm" | "rb"
        | "php" => {
            if uni {
                "\u{2328}"
            } else {
                "{"
            }
        }
        "sh" | "bash" | "zsh" | "fish" | "ksh" | "bashrc" | "zshrc" | "ps1" => {
            if uni {
                "\u{2699}"
            } else {
                "*"
            }
        }
        "sql" => {
            if uni {
                "\u{1F4BE}"
            } else {
                "$"
            }
        }
        "html" | "htm" | "css" | "scss" | "sass" | "less" | "vue" | "astro" => {
            if uni {
                "\u{2328}"
            } else {
                "%"
            }
        }
        "json" | "jsonc" | "json5" | "toml" | "ini" | "cfg" | "conf" | "editorconfig" | "yaml"
        | "yml" | "xml" => {
            if uni {
                "\u{2699}"
            } else {
                "$"
            }
        }
        "md" | "markdown" | "mdx" | "txt" | "log" | "man" => {
            if uni {
                "\u{1F4C4}"
            } else {
                "-"
            }
        }
        "csv" | "tsv" => {
            if uni {
                "\u{1F5C2}"
            } else {
                "="
            }
        }
        "pdf" | "doc" | "docx" | "odt" | "rtf" => {
            if uni {
                "\u{1F4C4}"
            } else {
                "-"
            }
        }
        "xls" | "xlsx" | "ods" | "ppt" | "pptx" | "odp" => {
            if uni {
                "\u{1F5C2}"
            } else {
                "="
            }
        }
        "epub" | "mobi" => {
            if uni {
                "\u{1F4D6}"
            } else {
                "-"
            }
        }
        "zip" | "tar" | "gz" | "xz" | "bz2" | "zst" | "7z" | "rar" | "tgz" | "txz" | "lz4"
        | "zstd" => {
            if uni {
                "\u{1F4E6}"
            } else {
                "#"
            }
        }
        "deb" | "rpm" | "appimage" => {
            if uni {
                "\u{1F4E6}"
            } else {
                "*"
            }
        }
        "iso" | "img" => {
            if uni {
                "\u{1F4BF}"
            } else {
                "#"
            }
        }
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "avif" | "tiff" | "tif"
        | "heic" | "svg" => {
            if uni {
                "\u{1F5BC}"
            } else {
                "%"
            }
        }
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus" | "wma" => {
            if uni {
                "\u{266A}"
            } else {
                "&"
            }
        }
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "flv" | "wmv" | "m4v" | "mpg" | "mpeg" => {
            if uni {
                "\u{1F3AC}"
            } else {
                "'"
            }
        }
        "sqlite" | "sqlite3" | "db" | "db3" => {
            if uni {
                "\u{1F4BE}"
            } else {
                "$"
            }
        }
        "exe" | "msi" | "dll" | "so" | "dylib" | "o" | "a" | "bin" => {
            if uni {
                "\u{2699}"
            } else {
                "*"
            }
        }
        "ttf" | "otf" | "woff" | "woff2" | "eot" => {
            if uni {
                "\u{1F524}"
            } else {
                "-"
            }
        }
        "lock" => {
            if uni {
                "\u{1F512}"
            } else {
                "="
            }
        }
        _ => return None,
    })
}

/// Kind-level fallback when no filename or extension mapping matched.
fn kind_fallback(kind: FileKind, is_hidden: bool, style: IconStyle) -> &'static str {
    match kind {
        FileKind::Directory => folder_icon(false, style),
        FileKind::Symlink => match style {
            IconStyle::Nerd => "\u{f0c1}",
            IconStyle::Unicode => "\u{1F517}",
            IconStyle::Ascii => "@",
        },
        FileKind::Executable => match style {
            IconStyle::Nerd => "\u{eae8}",
            IconStyle::Unicode => "\u{2699}",
            IconStyle::Ascii => "*",
        },
        FileKind::Archive => match style {
            IconStyle::Nerd => "\u{f410}",
            IconStyle::Unicode => "\u{1F4E6}",
            IconStyle::Ascii => "#",
        },
        FileKind::Image => match style {
            IconStyle::Nerd => "\u{f1c5}",
            IconStyle::Unicode => "\u{1F5BC}",
            IconStyle::Ascii => "%",
        },
        FileKind::Audio => match style {
            IconStyle::Nerd => "\u{f001}",
            IconStyle::Unicode => "\u{266A}",
            IconStyle::Ascii => "&",
        },
        FileKind::Video => match style {
            IconStyle::Nerd => "\u{f03d}",
            IconStyle::Unicode => "\u{1F3AC}",
            IconStyle::Ascii => "'",
        },
        FileKind::Document => match style {
            IconStyle::Nerd => "\u{f0f6}",
            IconStyle::Unicode => "\u{1F4C4}",
            IconStyle::Ascii => "-",
        },
        FileKind::Code => match style {
            IconStyle::Nerd => "\u{f28c}",
            IconStyle::Unicode => "\u{2328}",
            IconStyle::Ascii => "{",
        },
        FileKind::Regular => match style {
            IconStyle::Nerd => "\u{f15b}",
            IconStyle::Unicode => uni_glyph(UniCat::Regular, is_hidden),
            IconStyle::Ascii => {
                if is_hidden {
                    "."
                } else {
                    " "
                }
            }
        },
    }
}

/// Picks the icon for a filesystem entry under the given style.
pub fn file_icon(entry: &FileEntry, style: IconStyle) -> &'static str {
    if entry.is_dir {
        return folder_icon(false, style);
    }

    if let Some(icon) = special_file_icon(&entry.name.to_lowercase(), style) {
        return icon;
    }

    if entry.is_symlink && !matches!(style, IconStyle::Nerd) {
        return kind_fallback(FileKind::Symlink, entry.is_hidden, style);
    }
    if entry.is_symlink {
        return "\u{f0c1}";
    }

    if let Some(ext) = entry.extension.as_ref() {
        if let Some(icon) = ext_icon(&ext.to_lowercase(), style) {
            return icon;
        }
    }

    let kind = if entry.is_executable && matches!(entry.kind, FileKind::Regular) {
        FileKind::Executable
    } else {
        entry.kind
    };
    kind_fallback(kind, entry.is_hidden, style)
}

/// Convenience wrapper for paths that are not yet `FileEntry`s (tree nodes,
/// breadcrumbs): resolves directory vs. symlink vs. regular file glyphs.
pub fn path_icon(path: &Path, is_dir_hint: bool, style: IconStyle) -> &'static str {
    if is_dir_hint {
        return folder_icon(false, style);
    }
    let meta = std::fs::symlink_metadata(path);
    match meta {
        Ok(m) if m.file_type().is_symlink() => match style {
            IconStyle::Nerd => "\u{f0c1}",
            IconStyle::Unicode => "\u{1F517}",
            IconStyle::Ascii => "@",
        },
        _ => match style {
            IconStyle::Nerd => "\u{f15b}",
            IconStyle::Unicode => "\u{25AB}",
            IconStyle::Ascii => " ",
        },
    }
}

// Nerd Fonts v3 codepoints (mirroring the eza icon set).
/// Extension → Nerd Font v3 glyph.
fn nerd_ext_icon(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("\u{e68b}"),
        "c|h" => Some("\u{e61e}"),
        "cpp|cc|cxx|hpp|hh" => Some("\u{e61d}"),
        "go" => Some("\u{e65e}"),
        "zig" => Some("\u{e6a9}"),
        "py" => Some("\u{e606}"),
        "js|mjs|cjs" => Some("\u{e74e}"),
        "ts" => Some("\u{e628}"),
        "jsx" => Some("\u{e7ba}"),
        "tsx" => Some("\u{e7ba}"),
        "lua" => Some("\u{e620}"),
        "vim|nvim" => Some("\u{e7c5}"),
        "pl|pm" => Some("\u{e67e}"),
        "rb" => Some("\u{e739}"),
        "php" => Some("\u{e73d}"),
        "sh|bash|zsh|fish|ksh|bashrc|zshrc" => Some("\u{f489}"),
        "ps1" => Some("\u{ebc7}"),
        "sql" => Some("\u{f1c0}"),
        "html|htm" => Some("\u{f13b}"),
        "css|scss|sass|less" => Some("\u{e603}"),
        "vue" => Some("\u{f0844}"),
        "astro" => Some("\u{f13b}"),
        "json|jsonc|json5" => Some("\u{e60b}"),
        "toml|ini|cfg|conf|editorconfig" => Some("\u{e6b2}"),
        "yaml|yml" => Some("\u{e8eb}"),
        "xml" => Some("\u{f05c0}"),
        "md|markdown|mdx" => Some("\u{f48a}"),
        "txt|log|man" => Some("\u{f18d}"),
        "csv|tsv" => Some("\u{eefc}"),
        "pdf" => Some("\u{f1c1}"),
        "doc|docx|odt|rtf" => Some("\u{f1c2}"),
        "xls|xlsx|ods" => Some("\u{f1c3}"),
        "ppt|pptx|odp" => Some("\u{f1c4}"),
        "epub|mobi" => Some("\u{e28b}"),
        "zip" => Some("\u{f410}"),
        "tar|gz|xz|bz2|zst|7z|rar|tgz|txz|lz4|zstd" => Some("\u{f410}"),
        "deb|rpm|appimage" => Some("\u{e77d}"),
        "iso|img" => Some("\u{e271}"),
        "png|jpg|jpeg|gif|webp|bmp|ico|avif|tiff|tif|heic" => Some("\u{f1c5}"),
        "svg" => Some("\u{f0559}"),
        "mp3|flac|wav|ogg|m4a|aac|opus|wma" => Some("\u{f001}"),
        "mp4|mkv|webm|avi|mov|flv|wmv|m4v|mpg|mpeg" => Some("\u{f03d}"),
        "sqlite|sqlite3|db|db3" => Some("\u{e7c4}"),
        "exe|msi|dll|so|dylib|o|a|bin" => Some("\u{ebc4}"),
        "ttf|otf|woff|woff2|eot" => Some("\u{f031}"),
        "lock" => Some("\u{f023}"),
        _ => None,
    }
}

/// Well-known filename → Nerd Font v3 glyph.
fn nerd_special_icon(name_lower: &str) -> Option<&'static str> {
    match name_lower {
        "readme.md" => Some("\u{f00ba}"),
        "readme" => Some("\u{f00ba}"),
        "license" => Some("\u{f02d}"),
        "licence" => Some("\u{f02d}"),
        "copying" => Some("\u{f02d}"),
        "changelog" => Some("\u{f1ea}"),
        "makefile" => Some("\u{e673}"),
        "dockerfile" => Some("\u{e650}"),
        "cargo.toml" => Some("\u{e68b}"),
        "package.json" => Some("\u{e71e}"),
        ".gitignore" => Some("\u{f02a2}"),
        ".gitattributes" => Some("\u{f02a2}"),
        ".gitmodules" => Some("\u{f02a2}"),
        ".gitconfig" => Some("\u{f02a2}"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// $LS_COLORS filename coloring
// ---------------------------------------------------------------------------

/// Parsed `$LS_COLORS` environment, loaded once per process. `None` when the
/// variable is unset/empty or `$NO_COLOR` is active.
static LS_COLORS: OnceLock<Option<lscolors::LsColors>> = OnceLock::new();

fn ls_colors() -> Option<&'static lscolors::LsColors> {
    LS_COLORS
        .get_or_init(|| {
            if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
                return None;
            }
            let env_var = std::env::var("LS_COLORS").ok()?;
            if env_var.trim().is_empty() {
                return None;
            }
            Some(lscolors::LsColors::from_string(&env_var))
        })
        .as_ref()
}

fn to_ratatui_color(color: Option<lscolors::Color>) -> Option<Color> {
    use lscolors::Color as Ls;
    Some(match color? {
        Ls::Black => Color::Indexed(0),
        Ls::Red => Color::Indexed(1),
        Ls::Green => Color::Indexed(2),
        Ls::Yellow => Color::Indexed(3),
        Ls::Blue => Color::Indexed(4),
        Ls::Magenta => Color::Indexed(5),
        Ls::Cyan => Color::Indexed(6),
        Ls::White => Color::Indexed(7),
        Ls::BrightBlack => Color::Indexed(8),
        Ls::BrightRed => Color::Indexed(9),
        Ls::BrightGreen => Color::Indexed(10),
        Ls::BrightYellow => Color::Indexed(11),
        Ls::BrightBlue => Color::Indexed(12),
        Ls::BrightMagenta => Color::Indexed(13),
        Ls::BrightCyan => Color::Indexed(14),
        Ls::BrightWhite => Color::Indexed(15),
        Ls::Fixed(n) => Color::Indexed(n),
        Ls::RGB(r, g, b) => Color::Rgb(r, g, b),
    })
}

fn apply_font_style(style: Style, font_style: lscolors::FontStyle) -> Style {
    let mut style = style;
    if font_style.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if font_style.dimmed {
        style = style.add_modifier(Modifier::DIM);
    }
    if font_style.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if font_style.underline {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if font_style.slow_blink || font_style.rapid_blink {
        style = style.add_modifier(Modifier::SLOW_BLINK);
    }
    if font_style.reverse {
        style = style.add_modifier(Modifier::REVERSED);
    }
    if font_style.hidden {
        style = style.add_modifier(Modifier::HIDDEN);
    }
    if font_style.strikethrough {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    style
}

/// Maps an `$LS_COLORS` style for `path` into a Ratatui style fragment
/// (foreground/background/modifiers only). Returns `None` when no rule
/// matched so callers keep their theme defaults. Directories and executables
/// resolve via their indicators using the flags already carried by the
/// caller -- no extra stat calls on hot paths.
pub fn ls_colors_style(path: &Path, is_dir: bool, is_executable: bool) -> Option<Style> {
    ls_colors_style_for(ls_colors()?, path, is_dir, is_executable)
}

/// Testable core of [`ls_colors_style`]: resolves against an explicit
/// `LsColors` table.
pub fn ls_colors_style_for(
    lsc: &lscolors::LsColors,
    path: &Path,
    is_dir: bool,
    is_executable: bool,
) -> Option<Style> {
    // Prefer an explicit extension/suffix match (e.g. *.zip); indicator-based
    // rules only kick in for directories and executables.
    let styled = if is_dir {
        lsc.style_for_path(path)
            .or_else(|| lsc.style_for_indicator(lscolors::Indicator::Directory))
    } else if is_executable {
        lsc.style_for_path(path)
            .or_else(|| lsc.style_for_indicator(lscolors::Indicator::ExecutableFile))
    } else {
        lsc.style_for_path(path)
    };
    let styled = styled?;

    let mut style = Style::default();
    if let Some(fg) = to_ratatui_color(styled.foreground) {
        style = style.fg(fg);
    }
    if let Some(bg) = to_ratatui_color(styled.background) {
        style = style.bg(bg);
    }
    Some(apply_font_style(style, styled.font_style))
}
