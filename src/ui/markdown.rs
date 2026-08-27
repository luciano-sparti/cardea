use crate::theme::Theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Lightweight line-based markdown renderer. Parses inline patterns and
/// block-level structures and produces styled `Line`s for the preview panel.
/// No external dependencies — handles the most common GitHub-Flavored Markdown
/// subset sufficient for READMEs and notes.
pub fn render_markdown(text: &str, theme: &Theme) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let mut in_code_block = false;
    let mut code_block_lang = String::new();

    for raw_line in text.lines() {
        // Fenced code block toggle
        if let Some(rest) = raw_line.strip_prefix("```") {
            if in_code_block {
                in_code_block = false;
                code_block_lang.clear();
                out.push(Line::from(Span::styled(
                    "```",
                    Style::default().fg(theme.border),
                )));
            } else {
                in_code_block = true;
                code_block_lang = rest.trim().to_string();
                let label = if code_block_lang.is_empty() {
                    "```".to_string()
                } else {
                    format!("```{}", code_block_lang)
                };
                out.push(Line::from(Span::styled(
                    label,
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::ITALIC),
                )));
            }
            continue;
        }

        if in_code_block {
            out.push(Line::from(Span::styled(
                format!("  {}", raw_line),
                Style::default().fg(theme.status_fg),
            )));
            continue;
        }

        // Horizontal rule
        if is_horizontal_rule(raw_line) {
            out.push(Line::from(Span::styled(
                " ──────────────────────────────────────",
                Style::default().fg(theme.border),
            )));
            continue;
        }

        // Headers
        if let Some((level, text)) = parse_heading(raw_line) {
            let style = heading_style(level, theme);
            out.push(Line::from(Span::styled(
                format!("{}{}", "  ".repeat(((level - 1).min(3)) as usize), text),
                style,
            )));
            continue;
        }

        // Blockquote
        if let Some(quoted) = raw_line.strip_prefix('>') {
            let content = quoted.strip_prefix(' ').unwrap_or(quoted);
            let mut spans = vec![Span::styled(" ▎ ", Style::default().fg(theme.accent))];
            spans.extend(render_inline(content, theme));
            out.push(Line::from(spans));
            continue;
        }

        // Unordered list items
        if let Some(content) = parse_list_item(raw_line) {
            let mut spans = vec![Span::styled("   • ", Style::default().fg(theme.accent))];
            spans.extend(render_inline(content, theme));
            out.push(Line::from(spans));
            continue;
        }

        // Ordered list items
        if let Some((num, content)) = parse_ordered_list_item(raw_line) {
            let mut spans = vec![Span::styled(
                format!(" {:>2}. ", num),
                Style::default().fg(theme.accent),
            )];
            spans.extend(render_inline(content, theme));
            out.push(Line::from(spans));
            continue;
        }

        // Table rows (basic: | col | col |)
        if raw_line.trim_start().starts_with('|') && raw_line.trim_end().ends_with('|') {
            let inner = raw_line.trim().strip_prefix('|').unwrap_or(raw_line.trim());
            let inner = inner.strip_suffix('|').unwrap_or(inner);
            // Separator row (|---|---|)
            if inner
                .split('|')
                .all(|c| c.trim().chars().all(|ch| ch == '-' || ch == ':'))
            {
                out.push(Line::from(Span::styled(
                    "   ─── ─── ─── ─── ───",
                    Style::default().fg(theme.border),
                )));
            } else {
                let cells: Vec<&str> = inner.split('|').collect();
                let mut spans = vec![Span::styled(" ", Style::default())];
                for (i, cell) in cells.iter().enumerate() {
                    if i > 0 {
                        spans.push(Span::styled(" │ ", Style::default().fg(theme.border)));
                    }
                    spans.extend(render_inline(cell.trim(), theme));
                }
                out.push(Line::from(spans));
            }
            continue;
        }

        // Image reference (render as styled text, not actual image)
        if let Some((alt, _url)) = parse_image_ref(raw_line) {
            out.push(Line::from(vec![
                Span::styled("  󰥹 ", Style::default().fg(theme.accent)),
                Span::styled(
                    format!("[{}]", alt),
                    Style::default()
                        .fg(theme.status_fg)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
            continue;
        }

        // Empty line → blank line
        if raw_line.trim().is_empty() {
            out.push(Line::from(""));
            continue;
        }

        // Regular paragraph text
        let mut spans = vec![Span::styled("  ", Style::default())];
        spans.extend(render_inline(raw_line, theme));
        out.push(Line::from(spans));
    }

    out
}

fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    let first = trimmed.chars().next().unwrap();
    (first == '-' || first == '*' || first == '_')
        && trimmed.chars().all(|c| c == first || c == ' ')
}

/// Returns (level, text) for heading lines.
fn parse_heading(line: &str) -> Option<(u8, &str)> {
    let mut chars = line.char_indices();
    let mut level: u8 = 0;
    for (_, c) in chars.by_ref() {
        if c == '#' {
            level += 1;
        } else {
            break;
        }
    }
    if level == 0 || level > 6 {
        return None;
    }
    let rest = line[level as usize..].trim_start();
    if rest.is_empty() {
        return None;
    }
    Some((level, rest))
}

fn heading_style(level: u8, theme: &Theme) -> Style {
    let base = Style::default().add_modifier(Modifier::BOLD);
    match level {
        1 => base.fg(theme.accent),
        2 => base.fg(theme.accent),
        3 => base.fg(theme.fg),
        _ => base.fg(theme.status_fg),
    }
}

/// Strips leading `- ` or `* ` from a line if present.
fn parse_list_item(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        Some(&trimmed[2..])
    } else {
        None
    }
}

/// Parses `1. text` → Some((1, "text")).
fn parse_ordered_list_item(line: &str) -> Option<(u32, &str)> {
    let trimmed = line.trim_start();
    let dot_pos = trimmed.find('.')?;
    if dot_pos == 0 || dot_pos > 4 {
        return None;
    }
    let num_str = &trimmed[..dot_pos];
    let num: u32 = num_str.parse().ok()?;
    let rest = trimmed[dot_pos + 1..].trim_start();
    Some((num, rest))
}

fn parse_image_ref(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    let after_bang = trimmed.strip_prefix("![")?;
    let close = after_bang.find(']')?;
    let alt = &after_bang[..close];
    let rest = &after_bang[close + 1..];
    let url_start = rest.strip_prefix('(')?;
    let url_end = url_start.rfind(')')?;
    Some((alt, &url_start[..url_end]))
}

/// Renders inline markdown patterns: **bold**, *italic*, `code`, [links](url).
fn render_inline(text: &str, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Bold: **text** or __text__
        if let Some(end) = find_inline_delim(remaining, "**", 2) {
            if end > 2 {
                let inner = &remaining[2..end];
                spans.push(Span::styled(
                    inner.to_string(),
                    Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
                ));
                remaining = &remaining[end + 2..];
                continue;
            }
        }

        // Italic: *text* or _text_
        if !remaining.starts_with("**") {
            if let Some(end) = find_inline_delim(remaining, "*", 1) {
                if end > 0 {
                    let inner = &remaining[1..end];
                    spans.push(Span::styled(
                        inner.to_string(),
                        Style::default().fg(theme.fg).add_modifier(Modifier::ITALIC),
                    ));
                    remaining = &remaining[end + 1..];
                    continue;
                }
            }
        }

        // Inline code: `text`
        if remaining.starts_with('`') {
            if let Some(end) = remaining[1..].find('`') {
                let inner = &remaining[1..end + 1];
                spans.push(Span::styled(
                    format!("`{}`", inner),
                    Style::default().fg(theme.status_key),
                ));
                remaining = &remaining[end + 2..];
                continue;
            }
        }

        // Link: [text](url)
        if remaining.starts_with('[') {
            if let Some(close_bracket) = remaining[1..].find(']') {
                let link_text = &remaining[1..close_bracket + 1];
                let after = &remaining[close_bracket + 2..];
                if let Some(url_path) = after.strip_prefix('(') {
                    if let Some(close_paren) = url_path.find(')') {
                        let url = &url_path[..close_paren];
                        spans.push(Span::styled(
                            link_text.to_string(),
                            Style::default()
                                .fg(theme.accent)
                                .add_modifier(Modifier::UNDERLINED),
                        ));
                        spans.push(Span::styled(
                            format!(" ({})", url),
                            Style::default().fg(theme.status_fg),
                        ));
                        remaining = &url_path[close_paren + 1..];
                        continue;
                    }
                }
            }
        }

        // Plain text until the next special character
        let next_special = remaining
            .find(['*', '_', '`', '['])
            .unwrap_or(remaining.len());
        spans.push(Span::styled(
            remaining[..next_special].to_string(),
            Style::default().fg(theme.fg),
        ));
        remaining = &remaining[next_special..];
    }

    spans
}

/// Finds the closing delimiter, ensuring it's not escaped and not empty.
fn find_inline_delim(text: &str, delim: &str, delim_len: usize) -> Option<usize> {
    let mut search_from = delim_len;
    loop {
        let pos = text[search_from..].find(delim)?;
        let absolute = search_from + pos;
        // Don't match empty delimiters (e.g., ******)
        if absolute >= delim_len && absolute + delim_len <= text.len() {
            // Ensure it's a proper closing delimiter (not preceded by the same char
            // unless it's the opening)
            return Some(absolute);
        }
        search_from = absolute + delim_len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_heading() {
        assert_eq!(parse_heading("# Hello"), Some((1, "Hello")));
        assert_eq!(parse_heading("### Title"), Some((3, "Title")));
        assert_eq!(parse_heading("###### Deep"), Some((6, "Deep")));
        assert_eq!(parse_heading("No heading here"), None);
        assert_eq!(parse_heading("#"), None);
    }

    #[test]
    fn test_is_horizontal_rule() {
        assert!(is_horizontal_rule("---"));
        assert!(is_horizontal_rule("***"));
        assert!(is_horizontal_rule("___"));
        assert!(is_horizontal_rule("- - -"));
        assert!(!is_horizontal_rule("--"));
        assert!(!is_horizontal_rule("text ---"));
    }

    #[test]
    fn test_parse_list_item() {
        assert_eq!(parse_list_item("- item"), Some("item"));
        assert_eq!(parse_list_item("  * item"), Some("item"));
        assert_eq!(parse_list_item("not a list"), None);
    }

    #[test]
    fn test_parse_ordered_list_item() {
        assert_eq!(parse_ordered_list_item("1. first"), Some((1, "first")));
        assert_eq!(parse_ordered_list_item("42. answer"), Some((42, "answer")));
        assert_eq!(parse_ordered_list_item("not a list"), None);
    }
}
