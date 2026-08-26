use crate::app::{App, Focus};
use crate::theme::Theme;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

pub fn render_sidebar(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let is_focused = app.focus == Focus::Sidebar;
    app.sidebar_rect = area;

    let block = Block::default()
        .title(" 󰉖 Places & Tree ")
        .borders(Borders::ALL)
        .border_style(theme.style_border(is_focused))
        .style(Style::default().bg(theme.bg));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if inner_area.height == 0 || inner_area.width == 0 {
        return;
    }

    let mut lines = Vec::new();
    let mut tree_item_paths = Vec::new();

    // 1. Quick Access / Bookmarks Section
    lines.push(Line::from(vec![Span::styled(
        " QUICK ACCESS",
        Style::default()
            .fg(theme.tree_bookmark)
            .add_modifier(Modifier::BOLD),
    )]));
    tree_item_paths.push(None); // Section header (not selectable)

    for bookmark in &app.bookmarks {
        let is_current = app.tab().current_dir == bookmark.path;
        let is_cursor = is_focused && app.sidebar_selected_index == lines.len();

        let icon_span = Span::styled(
            format!("  {} ", bookmark.icon),
            if is_current {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.tree_bookmark)
            },
        );

        let name_span = Span::styled(
            bookmark.name.clone(),
            if is_cursor {
                theme.style_selected()
            } else if is_current {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            },
        );

        lines.push(Line::from(vec![icon_span, name_span]));
        tree_item_paths.push(Some(bookmark.path.clone()));
    }

    // Separator line
    lines.push(Line::from(vec![Span::styled(
        " ──────────────",
        Style::default().fg(theme.border),
    )]));
    tree_item_paths.push(None);

    // 2. Directory Tree Section
    lines.push(Line::from(vec![Span::styled(
        " DIRECTORIES",
        Style::default()
            .fg(theme.tree_folder)
            .add_modifier(Modifier::BOLD),
    )]));
    tree_item_paths.push(None);

    for node in &app.tree_nodes {
        let is_current = app.tab().current_dir == node.path;
        let is_cursor = is_focused && app.sidebar_selected_index == lines.len();

        let indent = "  ".repeat(node.depth);
        let arrow = if node.is_expanded { "▾ " } else { "▸ " };
        let icon = format!(
            "{} ",
            crate::icons::folder_icon(node.is_expanded, app.icon_style)
        );

        let prefix_span = Span::styled(
            format!(" {}{}{}", indent, arrow, icon),
            if node.is_expanded {
                Style::default().fg(theme.tree_folder_expanded)
            } else {
                Style::default().fg(theme.tree_folder)
            },
        );

        let name_span = Span::styled(
            node.name.clone(),
            if is_cursor {
                theme.style_selected()
            } else if is_current {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            },
        );

        lines.push(Line::from(vec![prefix_span, name_span]));
        tree_item_paths.push(Some(node.path.clone()));
    }

    app.sidebar_rendered_paths = tree_item_paths;

    // Scrolling logic for sidebar
    let visible_height = inner_area.height as usize;
    let total_items = lines.len();

    let scroll_offset = if app.sidebar_selected_index >= app.sidebar_scroll_offset + visible_height
    {
        app.sidebar_selected_index - visible_height + 1
    } else if app.sidebar_selected_index < app.sidebar_scroll_offset {
        app.sidebar_selected_index
    } else {
        app.sidebar_scroll_offset
    };

    app.sidebar_scroll_offset = scroll_offset.min(total_items.saturating_sub(visible_height));

    let visible_lines: Vec<ListItem> = lines
        .into_iter()
        .skip(app.sidebar_scroll_offset)
        .take(visible_height)
        .map(ListItem::new)
        .collect();

    let list = List::new(visible_lines).style(Style::default().bg(theme.bg));
    f.render_widget(list, inner_area);
}
