use crate::app::{App, Focus};
use crate::theme::Theme;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render_preview(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let is_focused = app.focus == Focus::Preview;
    app.preview_rect = area;

    let block = Block::default()
        .title(" 󰋽 Preview ")
        .borders(Borders::ALL)
        .border_style(theme.style_border(is_focused))
        .style(Style::default().bg(theme.bg));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if inner_area.height == 0 || inner_area.width == 0 {
        return;
    }

    let selected_entry = app.selected_entry().cloned();

    let entry = match selected_entry {
        Some(e) => e,
        None => {
            let p = Paragraph::new("\n  No item selected").style(
                Style::default()
                    .fg(theme.status_fg)
                    .add_modifier(Modifier::ITALIC),
            );
            f.render_widget(p, inner_area);
            return;
        }
    };

    let mut lines = Vec::new();

    // Metadata header
    lines.push(Line::from(vec![
        Span::styled(
            format!(" {} ", crate::icons::file_icon(&entry, app.icon_style)),
            Style::default().fg(theme.accent),
        ),
        Span::styled(
            entry.name.to_string(),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled(" Type: ", Style::default().fg(theme.status_fg)),
        Span::styled(format!("{:?}", entry.kind), Style::default().fg(theme.fg)),
    ]));

    lines.push(Line::from(vec![
        Span::styled(" Size: ", Style::default().fg(theme.status_fg)),
        Span::styled(entry.formatted_size(), Style::default().fg(theme.fg)),
    ]));

    lines.push(Line::from(vec![
        Span::styled(" Modified: ", Style::default().fg(theme.status_fg)),
        Span::styled(entry.formatted_modified(), Style::default().fg(theme.fg)),
    ]));

    lines.push(Line::from(vec![
        Span::styled(" Perms: ", Style::default().fg(theme.status_fg)),
        Span::styled(
            entry.formatted_permissions(),
            Style::default().fg(theme.status_fg),
        ),
    ]));

    // Metadata inspector extras (computed off-thread with the preview)
    if let Some(mime) = &app.preview_mime {
        lines.push(Line::from(vec![
            Span::styled(" MIME: ", Style::default().fg(theme.status_fg)),
            Span::styled(mime.clone(), Style::default().fg(theme.fg)),
        ]));
    }
    if let Some(sha) = &app.preview_sha256 {
        // Full digests wrap badly in a narrow panel; show a recognizable prefix
        let shown: String = sha.chars().take(16).collect();
        lines.push(Line::from(vec![
            Span::styled(" SHA256: ", Style::default().fg(theme.status_fg)),
            Span::styled(format!("{}…", shown), Style::default().fg(theme.fg)),
        ]));
    }

    if let Some(ref target) = entry.symlink_target {
        lines.push(Line::from(vec![
            Span::styled(" Target: ", Style::default().fg(theme.status_fg)),
            Span::styled(
                target.to_string_lossy().to_string(),
                Style::default().fg(theme.file_symlink),
            ),
        ]));
    }

    lines.push(Line::from(vec![Span::styled(
        " ───────────────────────",
        Style::default().fg(theme.border),
    )]));

    // Content area (loaded asynchronously via AsyncScanner::load_preview).
    // Image files reserve the space below the metadata header for the
    // graphics-protocol render; everything else streams text lines.
    let mut image_slot: Option<Rect> = None;
    if entry.is_dir {
        lines.push(Line::from(vec![Span::styled(
            "  [Directory - Press Enter to browse]",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::ITALIC),
        )]));
    } else if app.preview_loaded_path.as_ref() == Some(&entry.path)
        && app.preview_image_protocol.is_some()
    {
        // Dimensions hint first, so the reserved slot starts BELOW it
        let (img_w, img_h) = app.preview_image_dims.unwrap_or((0, 0));
        lines.push(Line::from(vec![Span::styled(
            format!("  [{img_w}\u{d7}{img_h} image]"),
            Style::default()
                .fg(theme.status_fg)
                .add_modifier(Modifier::ITALIC),
        )]));
        let meta_lines = lines.len() as u16;
        let avail = Rect {
            x: inner_area.x,
            y: inner_area.y + meta_lines,
            width: inner_area.width,
            height: inner_area.height.saturating_sub(meta_lines),
        };
        image_slot = Some(app.centered_image_rect(avail));
    } else if app.preview_loaded_path.as_ref() == Some(&entry.path) {
        match &app.preview_text {
            Some(Some(text)) => {
                let max_lines = inner_area.height.saturating_sub(7) as usize;
                let total = text.lines().count();

                // Markdown files get a custom formatted renderer that
                // understands headers, bold, italic, code blocks, lists, etc.
                let is_markdown = entry
                    .extension
                    .as_deref()
                    .map(|e| matches!(e.to_lowercase().as_str(), "md" | "markdown" | "mdown"))
                    .unwrap_or(false);

                if is_markdown {
                    let rendered = crate::ui::markdown::render_markdown(text, theme);
                    for line in rendered.into_iter().take(max_lines) {
                        lines.push(line);
                    }
                } else if let Some(styled) = &app.preview_styled {
                    // Syntax-highlighted rendering when a syntect definition
                    // matched; line numbers keep the theme's own styling
                    for (i, line) in styled.iter().take(max_lines).enumerate() {
                        let mut spans = vec![Span::styled(
                            format!("{:3} ", i + 1),
                            Style::default().fg(theme.border),
                        )];
                        spans.extend(line.spans.iter().cloned());
                        lines.push(Line::from(spans));
                    }
                } else {
                    for (i, line) in text.lines().take(max_lines).enumerate() {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:3} ", i + 1),
                                Style::default().fg(theme.border),
                            ),
                            Span::styled(line.to_string(), Style::default().fg(theme.fg)),
                        ]));
                    }
                }
                if total > max_lines {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  … {} more lines", total - max_lines),
                        Style::default()
                            .fg(theme.status_fg)
                            .add_modifier(Modifier::ITALIC),
                    )]));
                }
            }
            Some(None) => {
                // Binary content: xxd-style hex dump (no line numbers)
                if let Some(dump) = &app.preview_hex {
                    let max_lines = inner_area.height.saturating_sub(8) as usize;
                    for (i, line) in dump.lines().enumerate() {
                        if i >= max_lines {
                            lines.push(Line::from(Span::styled(
                                "  … truncated",
                                Style::default()
                                    .fg(theme.status_fg)
                                    .add_modifier(Modifier::ITALIC),
                            )));
                            break;
                        }
                        let style = if i < 2 {
                            Style::default()
                                .fg(theme.accent)
                                .add_modifier(Modifier::ITALIC)
                        } else {
                            Style::default().fg(theme.fg)
                        };
                        lines.push(Line::from(Span::styled(line.to_string(), style)));
                    }
                } else {
                    lines.push(Line::from(vec![Span::styled(
                        "  [Binary or unreadable file]",
                        Style::default()
                            .fg(theme.status_fg)
                            .add_modifier(Modifier::ITALIC),
                    )]));
                }
            }
            None => {}
        }
    } else if app.preview_pending_path.as_ref() == Some(&entry.path) {
        lines.push(Line::from(vec![Span::styled(
            "  Loading preview…",
            Style::default()
                .fg(theme.status_fg)
                .add_modifier(Modifier::ITALIC),
        )]));
    }

    let paragraph = Paragraph::new(lines).style(Style::default().bg(theme.bg));
    f.render_widget(paragraph, inner_area);

    // Draw the image last so protocol rendering can't overwrite metadata.
    // Resize::Scale upscales to fill the slot; since centered_image_rect
    // already matches the image aspect, this stays undistorted.
    if let Some(slot) = image_slot {
        if slot.width > 0 && slot.height > 0 {
            if let Some(protocol) = app.preview_image_protocol.as_mut() {
                let image_widget = ratatui_image::StatefulImage::default().resize(
                    ratatui_image::Resize::Scale(Some(ratatui_image::FilterType::Triangle)),
                );
                f.render_stateful_widget(image_widget, slot, protocol);
            }
        }
    }
}
