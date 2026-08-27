use crate::theme::adapt_syntax_color;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::path::Path;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

/// Syntax definitions are parsed once per process (~10ms) and shared across
/// all background preview loads.
static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

fn get_syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_nonewlines)
}

fn get_theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Highlights `text` (a full file's contents) using the syntax inferred from
/// `path`. Returns `None` when no syntax definition matches, in which case
/// the caller falls back to plain rendering.
pub fn highlight(path: &Path, text: &str) -> Option<Vec<Line<'static>>> {
    let ss = get_syntax_set();
    let syntax = ss.find_syntax_for_file(path).ok().flatten()?;
    let themes = get_theme_set();
    let theme = themes.themes.get("base16-ocean.dark")?;
    let mut hl = HighlightLines::new(syntax, theme);

    let mut out = Vec::new();
    for line in text.lines() {
        let Ok(regions) = hl.highlight_line(line, ss) else {
            return None;
        };
        let mut spans = Vec::with_capacity(regions.len());
        for (style, chunk) in regions {
            if chunk.is_empty() {
                continue;
            }
            let mut modifier = Modifier::empty();
            if style.font_style.contains(FontStyle::BOLD) {
                modifier |= Modifier::BOLD;
            }
            if style.font_style.contains(FontStyle::ITALIC) {
                modifier |= Modifier::ITALIC;
            }
            if style.font_style.contains(FontStyle::UNDERLINE) {
                modifier |= Modifier::UNDERLINED;
            }
            let fg = adapt_syntax_color(ratatui_color(
                style.foreground.r,
                style.foreground.g,
                style.foreground.b,
            ));
            spans.push(Span::styled(
                chunk.to_string(),
                Style::default().fg(fg).add_modifier(modifier),
            ));
        }
        out.push(Line::from(spans));
    }
    Some(out)
}

fn ratatui_color(r: u8, g: u8, b: u8) -> ratatui::style::Color {
    use ratatui::style::Color;
    Color::Rgb(r, g, b)
}
