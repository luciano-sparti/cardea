use crate::app::{App, Focus};
use crate::theme::Theme;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::path::{Component, PathBuf};

#[derive(Debug, Clone)]
pub struct BreadcrumbSegment {
    pub name: String,
    pub path: PathBuf,
    pub area: Rect,
}

pub fn render_breadcrumb(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let is_focused = app.focus == Focus::Breadcrumb || app.focus == Focus::PathInput;

    if app.focus == Focus::PathInput {
        // Editable text input mode
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.style_border(true))
            .style(Style::default().bg(theme.bg));

        let input_text = format!(" 󰉖  {}", app.path_input_buffer);
        let paragraph = Paragraph::new(input_text)
            .style(Style::default().fg(theme.fg).bg(theme.breadcrumb_bg))
            .block(block);

        f.render_widget(paragraph, area);

        // Render cursor in input mode
        let cursor_x = area.x + 5 + app.path_input_cursor as u16;
        let cursor_y = area.y + 1;
        if cursor_x < area.x + area.width - 1 {
            f.set_cursor_position((cursor_x, cursor_y));
        }
        return;
    }

    // Interactive Breadcrumb Chips mode
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.style_border(is_focused))
        .style(Style::default().bg(theme.breadcrumb_bg));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    // Calculate breadcrumb segments
    let mut spans = Vec::new();
    let mut segments = Vec::new();
    let mut current_accum = PathBuf::new();

    let mut current_x = inner_area.x;
    let y = inner_area.y;

    // Home / Root symbol
    spans.push(Span::styled(" ", Style::default()));
    current_x += 1;

    let path = &app.tab().current_dir;
    let components: Vec<_> = path.components().collect();
    let home = std::env::var("HOME").ok().map(PathBuf::from);

    for (i, comp) in components.iter().enumerate() {
        let is_last = i == components.len() - 1;
        let is_cursor = app.focus == Focus::Breadcrumb && app.breadcrumb_selected == segments.len();

        let (seg_name, seg_icon) = match comp {
            Component::RootDir => (
                String::new(),
                crate::icons::breadcrumb_icon(true, false, app.icon_style),
            ),
            Component::Normal(os_str) => {
                let name = os_str.to_string_lossy().to_string();
                let is_home = home.as_deref() == Some(current_accum.as_path());
                (
                    name,
                    crate::icons::breadcrumb_icon(false, is_home, app.icon_style),
                )
            }
            Component::Prefix(_) => (
                "Drive".to_string(),
                crate::icons::breadcrumb_icon(false, false, app.icon_style),
            ),
            _ => continue,
        };

        current_accum.push(comp);
        let segment_path = current_accum.clone();

        let chip_text = if seg_name.is_empty() {
            format!(" {} ", seg_icon)
        } else {
            format!(" {} {} ", seg_icon, seg_name)
        };
        let chip_len = chip_text.chars().count() as u16;

        let chip_style = if is_last {
            Style::default()
                .bg(theme.breadcrumb_active_bg)
                .fg(theme.breadcrumb_active_fg)
                .add_modifier(Modifier::BOLD)
        } else if is_cursor {
            theme.style_selected()
        } else {
            Style::default()
                .bg(theme.breadcrumb_bg)
                .fg(theme.breadcrumb_fg)
        };

        if current_x + chip_len < inner_area.x + inner_area.width - 10 {
            let seg_area = Rect::new(current_x, y, chip_len, 1);
            segments.push(BreadcrumbSegment {
                name: seg_name,
                path: segment_path,
                area: seg_area,
            });

            spans.push(Span::styled(chip_text, chip_style));
            current_x += chip_len;

            if !is_last {
                let arrow = " 󰅂 ";
                spans.push(Span::styled(
                    arrow,
                    Style::default().fg(theme.breadcrumb_arrow),
                ));
                current_x += 3;
            }
        }
    }

    // Right-aligned hint: "Ctrl+L: Edit path"
    let hint = " [Ctrl+L] Edit Path ";
    let hint_len = hint.len() as u16;
    if inner_area.width > current_x - inner_area.x + hint_len + 2 {
        let spaces = inner_area
            .width
            .saturating_sub((current_x - inner_area.x) + hint_len);
        spans.push(Span::raw(" ".repeat(spaces as usize)));
        spans.push(Span::styled(
            hint,
            Style::default()
                .fg(theme.status_fg)
                .add_modifier(Modifier::DIM),
        ));
    }

    app.breadcrumb_segments = segments;

    let paragraph = Paragraph::new(Line::from(spans));
    f.render_widget(paragraph, inner_area);
}

/// Renders the sibling-directory popover. Must be called *after* all main
/// panels are drawn so it floats above them.
pub fn render_breadcrumb_popover(f: &mut Frame, app: &mut App, theme: &Theme) {
    let Some(pop) = app.breadcrumb_popover.as_mut() else {
        return;
    };
    let inner_area = f.area();
    let anchor = app
        .breadcrumb_segments
        .get(app.breadcrumb_selected)
        .map(|s| s.area)
        .unwrap_or(Rect::new(inner_area.x, inner_area.y, 10, 1));
    let screen_w = inner_area.width;
    let screen_h = inner_area.height;

    let visible = pop.items.len().min(pop.max_visible).max(1) as u16;
    let width = pop
        .items
        .iter()
        .map(|p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .count()
        })
        .max()
        .unwrap_or(10) as u16
        + 6;
    let width = width
        .min(screen_w.saturating_sub(anchor.x + 2))
        .max(16)
        .min(screen_w.saturating_sub(2));

    let x = anchor.x.min(screen_w.saturating_sub(width + 1));
    let y = anchor.y + 1;
    // Clamp height if there's no room below the breadcrumb bar
    let max_below = screen_h.saturating_sub(y + 2);
    let height = visible.min(max_below.max(3));

    let rect = Rect::new(x, y, width, height);
    pop.screen_rect = rect;

    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_focus))
        .style(Style::default().bg(theme.bg));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let end = (pop.scroll_offset + height as usize).min(pop.items.len());
    let lines: Vec<Line> = pop.items[pop.scroll_offset..end]
        .iter()
        .enumerate()
        .map(|(vis_idx, path)| {
            let actual_idx = pop.scroll_offset + vis_idx;
            let is_sel = actual_idx == pop.selected;
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let style = if is_sel {
                theme.style_selected()
            } else {
                Style::default().fg(theme.tree_folder)
            };
            Line::from(Span::styled(format!(" {} ", name), style))
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}
