use crate::app::{App, ButtonKind, Focus};
use crate::config::SortColumn;
use crate::theme::Theme;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

pub fn render_status_bar(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::NONE)
        .style(theme.style_status());

    let inner_area = area;
    f.render_widget(block, area);

    // Left side: Active operation progress, drag gesture, message, or stats
    let left_spans = if let Some(drag) = &app.drag_drop {
        vec![
            Span::styled(
                if drag.copy { " 󰆏 " } else { " 󰆎 " },
                Style::default().fg(theme.filter_match),
            ),
            Span::styled(
                format!(
                    "{} {} item(s) — release over a folder or breadcrumb",
                    if drag.copy { "Copying" } else { "Moving" },
                    drag.paths.len()
                ),
                Style::default()
                    .fg(theme.filter_match)
                    .add_modifier(Modifier::BOLD),
            ),
        ]
    } else if let Some(op) = app.active_ops.first() {
        vec![
            Span::styled(" 󰔟 ", Style::default().fg(theme.filter_match)),
            Span::styled(
                format!(
                    "{} {}/{}{}",
                    op.label,
                    op.done,
                    op.total,
                    if op.current.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", op.current)
                    }
                ),
                Style::default()
                    .fg(theme.filter_match)
                    .add_modifier(Modifier::BOLD),
            ),
        ]
    } else if let Some(ref msg) = app.status_message {
        let color = if msg.is_error {
            theme.status_error
        } else {
            theme.status_info
        };
        vec![
            Span::styled(" 󰀦 ", Style::default().fg(color)),
            Span::styled(
                &msg.text,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        let total_count = if app.tab().search_mode {
            app.tab().search_matches.len()
        } else {
            app.tab().entries.len()
        };
        let filtered_count = app.tab().filtered_indices.len();
        let selected_count = app.tab().multi_selected.len();

        let mut spans = vec![
            Span::styled(" ", Style::default().fg(theme.accent)),
            Span::styled(
                format!("{} items", total_count),
                Style::default().fg(theme.status_fg),
            ),
        ];

        if !app.tab().search_query.is_empty() && !app.tab().search_mode {
            spans.push(Span::styled(
                format!(" (filtered: {})", filtered_count),
                Style::default().fg(theme.filter_match),
            ));
        }

        if selected_count > 0 {
            spans.push(Span::styled(
                format!(" | {} selected", selected_count),
                Style::default()
                    .fg(theme.tree_bookmark)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // Active sort order
        let sort_label = match app.sort_column {
            SortColumn::Name => "Name",
            SortColumn::Size => "Size",
            SortColumn::Modified => "Modified",
            SortColumn::Extension => "Ext",
            SortColumn::Permissions => "Perms",
        };
        spans.push(Span::styled(" | ", Style::default().fg(theme.border)));
        spans.push(Span::styled(
            format!("{} {}", sort_label, app.sort_direction.symbol()),
            Style::default().fg(theme.status_info),
        ));

        // Free space on current filesystem
        if let Some(free) = app.disk_free {
            spans.push(Span::styled(" | ", Style::default().fg(theme.border)));
            spans.push(Span::styled(
                format!("{} free", crate::fs::format_size(free)),
                Style::default().fg(theme.status_fg),
            ));
        }

        // Search progress indicator
        if app.tab().search_running {
            spans.push(Span::styled(
                " | searching…",
                Style::default()
                    .fg(theme.filter_match)
                    .add_modifier(Modifier::ITALIC),
            ));
        }

        spans
    };

    // Right side: Keybinding hints
    let right_spans = vec![
        Span::styled(" [?]", Style::default().fg(theme.status_key)),
        Span::styled(" Help ", Style::default().fg(theme.status_fg)),
        Span::styled("[/]", Style::default().fg(theme.status_key)),
        Span::styled(" Filter ", Style::default().fg(theme.status_fg)),
        Span::styled("[Ctrl+F]", Style::default().fg(theme.status_key)),
        Span::styled(" Search ", Style::default().fg(theme.status_fg)),
        Span::styled("[Ctrl+H]", Style::default().fg(theme.status_key)),
        Span::styled(
            if app.show_hidden {
                " Hide . "
            } else {
                " Show . "
            },
            Style::default().fg(theme.status_fg),
        ),
        Span::styled("[F7]", Style::default().fg(theme.status_key)),
        Span::styled(
            if app.show_preview {
                " Hide Preview "
            } else {
                " Preview "
            },
            Style::default().fg(theme.status_fg),
        ),
    ];
    let mut right_spans = right_spans;
    if !app.active_ops.is_empty() {
        right_spans.push(Span::styled(
            "[Ctrl+J]",
            Style::default().fg(theme.status_key),
        ));
        right_spans.push(Span::styled(
            format!(" Jobs({}) ", app.active_ops.len()),
            Style::default()
                .fg(theme.filter_match)
                .add_modifier(Modifier::BOLD),
        ));
    }
    right_spans.extend(vec![
        Span::styled("[q]", Style::default().fg(theme.status_key)),
        Span::styled(" Quit ", Style::default().fg(theme.status_fg)),
    ]);

    let left_paragraph = Paragraph::new(Line::from(left_spans)).alignment(Alignment::Left);
    let right_paragraph = Paragraph::new(Line::from(right_spans)).alignment(Alignment::Right);

    f.render_widget(left_paragraph, inner_area);
    f.render_widget(right_paragraph, inner_area);
}

pub fn render_filter_bar(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    if app.focus != Focus::FilterInput && app.tab().search_query.is_empty() {
        return;
    }

    let title = if app.tab().search_mode {
        " 󰍟 Recursive Search (Enter keeps results, Esc exits) "
    } else {
        " 󰍉 Filter In Place (Esc to clear) "
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(theme.style_border(app.focus == Focus::FilterInput))
        .style(Style::default().bg(theme.bg));

    let input_text = format!(" / {}", app.tab().search_query);
    let paragraph = Paragraph::new(input_text)
        .style(Style::default().fg(theme.filter_match).bg(theme.bg))
        .block(block);

    f.render_widget(paragraph, area);

    if app.focus == Focus::FilterInput {
        let cursor_x = area.x + 4 + app.tab().search_cursor as u16;
        let cursor_y = area.y + 1;
        if cursor_x < area.x + area.width - 1 {
            f.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

pub fn render_help_modal(f: &mut Frame, app: &App, theme: &Theme) {
    if !app.show_help {
        return;
    }

    let area = centered_rect(65, 75, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" 󰞋 Fenestra — Keyboard & Mouse Shortcuts ")
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme.table_header_bg));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut help_text = vec![
        Line::from(vec![Span::styled(
            " NAVIGATION",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled(
                "   j / ↓             ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Move cursor down", Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled(
                "   k / ↑             ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Move cursor up", Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled(
                "   h / Backspace     ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Navigate to parent directory",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   l / Enter         ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Open folder / selected file", Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled(
                "   Tab               ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Switch focus (Sidebar ⇄ Table ⇄ Preview)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+F            ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Recursive search across subdirectories",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+L            ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Focus & edit address / path bar",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Alt+← / Alt+→     ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Navigate back / forward in history",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   g / Home  G / End ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Jump to first / last item", Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+D / Ctrl+U   ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Page down / page up", Style::default().fg(theme.fg)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            " SORTING & SELECTION",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled(
                "   Space             ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Toggle multi-select for current item",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+A            ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Select all items in directory",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Shift+↑/↓ / Click ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Extend range selection from anchor",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   *                 ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Invert selection", Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled(
                "   Esc               ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Clear multi-selection", Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled(
                "   s                 ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Cycle sort column (Name → Size → Modified → Perms)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   S / r             ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Reverse active sort direction (Asc ⇄ Desc)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            " OPERATIONS & PREVIEWS",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled(
                "   /                 ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Instant in-place filter search",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+H / .        ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Toggle hidden files (dotfiles)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   F7                ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Toggle file preview drawer", Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled(
                "   v                 ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Quick look (open preview for selected file)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   F9 / b            ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Toggle places & tree sidebar",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Delete            ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Move item(s) to system Trash",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   d                 ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Move to Trash after confirmation dialog",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Shift+Delete      ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Permanently delete (confirmation required)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+C / Ctrl+X   ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Copy / cut selection to clipboard",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+V            ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Paste clipboard into current directory (async)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   F2                ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Rename selected item", Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+N            ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Create new folder in current directory",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+Shift+N      ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Create new empty file in current directory",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   m / Menu / S-F10  ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Open context menu for the selected item",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            " TABS & DUAL-PANE",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+T            ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "New tab (duplicates current directory)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+W            ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Close tab (closing the last tab quits)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+Tab          ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Cycle tabs forward (Ctrl+Shift+Tab back)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Alt+1..9          ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Jump directly to tab N", Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled(
                "   F3                ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Toggle dual-pane Commander view",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Tab (dual-pane)   ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Swap between left and right panes",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   F5 / F6           ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Copy / move selection to opposite pane",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl+J            ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Background job queue (view & cancel running ops)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   ? / F1            ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Toggle this help dialog", Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled(
                "   q / Ctrl+C        ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Quit Fenestra", Style::default().fg(theme.fg)),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            " MOUSE INTERACTIONS",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled(
                "   Sidebar h/l ←/→   ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Collapse / expand tree folder (lazy-loaded)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Breadcrumb s      ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Open sibling directory dropdown for selected chip",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Right Click       ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Open sibling dropdown on a breadcrumb chip",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Alt+[ / Alt+]     ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Shrink / widen Name column", Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled(
                "   Left Click        ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Focus panel / select item / jump via breadcrumb / sort by column",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Double Click      ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Open folder / launch file (works in sidebar too)",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Right Click Row   ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Select row and open its context menu",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Right Click Tree  ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Context menu for a sidebar folder/bookmark",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Breadcrumb m      ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Context menu for the focused path chip",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Middle Click      ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled("Toggle selection on a row", Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled(
                "   Drag & Drop       ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Drag rows onto sidebar folders or breadcrumb chips to move",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Ctrl + Drag       ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Drag with Ctrl held to copy instead of move",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Scroll Wheel      ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Scroll table or sidebar under the cursor",
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Scroll ←/→       ",
                Style::default().fg(theme.status_key),
            ),
            Span::styled(
                "Navigate back / forward in history",
                Style::default().fg(theme.fg),
            ),
        ]),
    ];

    // User-defined [[actions]] from config.toml, listed with their bindings
    // so the cheat sheet always reflects the live configuration
    if !app.user_actions.is_empty() {
        help_text.push(Line::from(""));
        help_text.push(Line::from(vec![Span::styled(
            " USER ACTIONS",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )]));
        for action in &app.user_actions {
            let key_label = action.key.as_deref().unwrap_or("(menu only)");
            help_text.push(Line::from(vec![
                Span::styled(
                    format!("   {:<19}", key_label),
                    Style::default().fg(theme.status_key),
                ),
                Span::styled(action.name.clone(), Style::default().fg(theme.fg)),
            ]));
        }
    }

    let paragraph = Paragraph::new(help_text).wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}

/// Renders the active modal confirmation dialog: a centered, content-sized
/// panel with a dimmed backdrop, message body, and Cancel/Confirm buttons.
pub fn render_dialog_modal(f: &mut Frame, app: &mut App, theme: &Theme) {
    if app.dialog.is_none() {
        return;
    }

    let full = f.area();
    let dialog = app.dialog.as_mut().unwrap();

    // Content-sized geometry (clamped to the terminal)
    let msg_width = dialog
        .message
        .iter()
        .map(|l| l.chars().count() as u16)
        .max()
        .unwrap_or(0);
    let title_width = dialog.title.chars().count() as u16 + 2;
    let buttons_width: u16 = dialog
        .buttons
        .iter()
        .map(|b| b.label.chars().count() as u16 + 4)
        .sum::<u16>()
        + 3 * dialog.buttons.len().saturating_sub(1) as u16
        + 2; // gaps + right padding
    let prompt_width = dialog
        .prompt
        .as_ref()
        .map(|p| p.buffer.chars().count() as u16 + 6)
        .unwrap_or(0);
    let inner_width = msg_width
        .max(buttons_width)
        .max(prompt_width)
        .max(title_width.min(full.width));
    let width = (inner_width + 4).clamp(28, full.width.saturating_sub(2));
    let message_height =
        dialog.message.len() as u16 + if dialog.message.is_empty() { 0 } else { 1 };
    let height = (message_height + 4 + if dialog.prompt.is_some() { 2 } else { 0 })
        .min(full.height.saturating_sub(2));

    let area = Rect {
        x: full.x + (full.width - width) / 2,
        y: full.y + (full.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    let border_color = if dialog.destructive {
        theme.status_error
    } else {
        theme.accent
    };
    let block = Block::default()
        .title(Span::styled(
            &dialog.title,
            Style::default()
                .fg(border_color)
                .bg(theme.table_header_bg)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme.bg));

    // Dimmed backdrop so the modal visually mutes the app behind it
    let backdrop = Block::default().style(Style::default().bg(theme.bg));
    f.render_widget(backdrop, full);
    f.render_widget(Clear, area);

    let inner = block.inner(area);
    f.render_widget(block, area);
    dialog.screen_area = area;

    // Message body
    let lines: Vec<Line> = dialog
        .message
        .iter()
        .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(theme.fg))))
        .collect();
    let body_area = Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(2),
        height: inner.height.saturating_sub(1),
    };
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body_area);

    // Prompt input line (bottom of the body, above the buttons row)
    let mut cursor_pos: Option<(u16, u16)> = None;
    if let Some(prompt) = &dialog.prompt {
        let input_y = area.bottom().saturating_sub(4);
        let input_area = Rect {
            x: inner.x + 1,
            y: input_y,
            width: inner.width.saturating_sub(2),
            height: 1,
        };
        let text = format!(" {}", prompt.buffer);
        f.render_widget(
            Paragraph::new(Span::styled(
                text.clone(),
                Style::default().fg(theme.filter_match).bg(theme.bg),
            )),
            input_area,
        );
        let cursor_x = input_area.x + 1 + prompt.cursor as u16;
        if cursor_x < input_area.right() {
            cursor_pos = Some((cursor_x, input_y));
        }
    }

    // Buttons row (bottom-right): one bracketed button per choice
    let gap = 3u16;
    let btn_y = area.bottom().saturating_sub(2);
    let widths: Vec<u16> = dialog
        .buttons
        .iter()
        .map(|b| b.label.chars().count() as u16 + 4) // "[ X ]"
        .collect();
    let row_width: u16 =
        widths.iter().sum::<u16>() + (gap * (widths.len().saturating_sub(1)) as u16);
    let mut btn_x = area.x + area.width.saturating_sub(row_width + 2);

    let mut rects = Vec::with_capacity(dialog.buttons.len());
    for w in &widths {
        rects.push(Rect {
            x: btn_x,
            y: btn_y,
            width: *w,
            height: 1,
        });
        btn_x += w + gap;
    }

    let mut spans = Vec::new();
    for (i, btn) in dialog.buttons.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("   ", Style::default()));
        }
        let selected = i == dialog.selected_button;
        let is_cancel = matches!(btn.kind, ButtonKind::Cancel);
        spans.push(Span::styled("[ ", Style::default().fg(theme.border)));
        spans.push(Span::styled(
            btn.label.clone(),
            if selected {
                Style::default()
                    .fg(if dialog.destructive && !is_cancel {
                        theme.status_error
                    } else {
                        theme.accent
                    })
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.status_fg)
            },
        ));
        spans.push(Span::styled(" ]", Style::default().fg(theme.border)));
    }

    let first_x = rects.first().map(|r| r.x).unwrap_or(area.x);
    let btn_area = Rect {
        x: first_x,
        y: btn_y,
        width: area.right().saturating_sub(first_x),
        height: 1,
    };
    f.render_widget(Paragraph::new(Line::from(spans)), btn_area);

    dialog.button_rects = rects;

    if let Some((x, y)) = cursor_pos {
        f.set_cursor_position((x, y));
    }
}

/// Renders the floating context menu: a compact, bordered popup anchored at
/// the requested position (clamped to stay on-screen), with the highlighted
/// entry inverted and separators drawn as thin rules.
pub fn render_context_menu(f: &mut Frame, app: &mut App, theme: &Theme) {
    let Some(menu) = app.context_menu.as_mut() else {
        return;
    };

    let full = f.area();
    let label_width = menu
        .items
        .iter()
        .map(|item| item.label.chars().count() as u16)
        .max()
        .unwrap_or(10);
    let width = (label_width + 4).min(full.width.saturating_sub(2));
    let visible = menu.max_visible.min(menu.items.len()) as u16;
    let height = (visible + 2).min(full.height.saturating_sub(2));

    // Clamp so the popup stays inside the terminal
    let x = menu.anchor_x.min(full.width.saturating_sub(width + 1));
    let y = if menu.anchor_y + height >= full.height {
        menu.anchor_y.saturating_sub(height)
    } else {
        menu.anchor_y
    };
    let area = Rect {
        x,
        y,
        width,
        height,
    };

    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.table_header_bg));
    f.render_widget(block, area);
    menu.screen_rect = area;

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    for (row, idx) in (menu.scroll_offset..menu.items.len()).enumerate() {
        if row as u16 >= inner.height {
            break;
        }
        let item = &menu.items[idx];
        let row_area = Rect {
            x: inner.x,
            y: inner.y + row as u16,
            width: inner.width,
            height: 1,
        };
        if item.is_separator() {
            let rule = Span::styled(
                "─".repeat(inner.width as usize),
                Style::default().fg(theme.border),
            );
            f.render_widget(Paragraph::new(Line::from(rule)), row_area);
        } else if idx == menu.selected {
            f.render_widget(
                Paragraph::new(Span::styled(
                    item.label.clone(),
                    Style::default()
                        .bg(theme.selection_bg)
                        .fg(theme.selection_fg)
                        .add_modifier(Modifier::BOLD),
                )),
                row_area,
            );
        } else {
            f.render_widget(
                Paragraph::new(Span::styled(
                    item.label.clone(),
                    Style::default().fg(theme.fg),
                )),
                row_area,
            );
        }
    }
}

/// Renders the background job queue overlay (`Ctrl+J`): a centered panel
/// listing active operations with text progress bars, per-job [ Cancel ]
/// buttons (rects recorded for mouse hit-testing), and keyboard hints.
pub fn render_job_queue(f: &mut Frame, app: &mut App, theme: &Theme) {
    if !app.show_job_queue {
        return;
    }

    let full = f.area();
    let width = (full.width * 3 / 5).clamp(40, full.width.saturating_sub(2));
    let height = (app.active_ops.len() as u16 + 5).clamp(5, full.height.saturating_sub(2)); // rows + borders + header + footer
    let area = Rect {
        x: full.x + (full.width - width) / 2,
        y: full.y + (full.height - height) / 2,
        width,
        height,
    };

    f.render_widget(Clear, area);
    let block = Block::default()
        .title(Span::styled(
            " 󰆉 Background Jobs ",
            Style::default()
                .fg(theme.accent)
                .bg(theme.table_header_bg)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.bg));
    f.render_widget(block, area);
    app.job_queue_rect = area;
    app.job_queue_cancel_rects.clear();

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    let footer_y = area.bottom().saturating_sub(1);
    let list_height = footer_y.saturating_sub(inner.y + 1); // minus column header line

    // Column header row
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " Operation",
            Style::default()
                .fg(theme.border)
                .add_modifier(Modifier::BOLD),
        ))),
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        },
    );

    if app.active_ops.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " No background operations running.",
                Style::default().fg(theme.border),
            ))),
            Rect {
                x: inner.x,
                y: inner.y + 1,
                width: inner.width,
                height: 1,
            },
        );
    }

    for (row, (idx, op)) in app.active_ops.iter().enumerate().enumerate() {
        if row as u16 >= list_height {
            break;
        }
        let row_y = inner.y + 1 + row as u16;
        let selected = idx == app.job_queue_selected;

        // Progress bar: 10 cells, filled proportionally to done/total
        let cells = 10usize;
        let filled = (op.done.min(op.total) * cells)
            .checked_div(op.total)
            .unwrap_or(0)
            .min(cells);
        let bar = format!("{}{}", "▰".repeat(filled), "▱".repeat(cells - filled));
        let state = if op.cancelling {
            "cancelling…".to_string()
        } else {
            format!("{}/{}", op.done, op.total)
        };

        let cancel_label = "[ Cancel ]";
        let cancel_width = cancel_label.chars().count() as u16 + 2;
        let info_width = inner.width.saturating_sub(cancel_width + 1);

        let mut spans = vec![
            Span::styled(" ", Style::default()),
            Span::styled(
                &op.label,
                Style::default()
                    .fg(if selected {
                        theme.selection_fg
                    } else {
                        theme.fg
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {} ", bar),
                Style::default().fg(theme.filter_match),
            ),
            Span::styled(state.clone(), Style::default().fg(theme.status_info)),
        ];
        if !op.current.is_empty() && !op.cancelling {
            let avail = (info_width as usize).saturating_sub(
                2 + op.label.chars().count() + bar.chars().count() + state.len() + 4,
            );
            if avail > 3 {
                let name: String = op.current.chars().take(avail - 1).collect();
                spans.push(Span::styled(
                    format!(" — {}", name),
                    Style::default().fg(theme.border),
                ));
            }
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                x: inner.x,
                y: row_y,
                width: info_width,
                height: 1,
            },
        );

        // [ Cancel ] button (or dimmed placeholder while cancelling)
        let btn_area = Rect {
            x: inner.x + inner.width.saturating_sub(cancel_width),
            y: row_y,
            width: cancel_width,
            height: 1,
        };
        let btn_spans = vec![
            Span::styled("[ ", Style::default().fg(theme.border)),
            Span::styled(
                "Cancel",
                if op.cancelling {
                    Style::default().fg(theme.border)
                } else if selected {
                    Style::default()
                        .fg(theme.status_error)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.status_fg)
                },
            ),
            Span::styled(" ]", Style::default().fg(theme.border)),
        ];
        f.render_widget(Paragraph::new(Line::from(btn_spans)), btn_area);
        if !op.cancelling {
            app.job_queue_cancel_rects.push(btn_area);
        }
    }

    // Footer hints
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " ↑/↓ select · x cancel · Esc close",
            Style::default().fg(theme.border),
        ))),
        Rect {
            x: inner.x,
            y: footer_y,
            width: inner.width,
            height: 1,
        },
    );
}

/// Renders the modifier discovery popup: a small floating panel listing all
/// bindings that use the modifiers of an unbound chord (e.g. pressing an
/// unrecognised Ctrl+chord shows everything else Ctrl does). Auto-expires.
pub fn render_modifier_hint(f: &mut Frame, app: &App, theme: &Theme) {
    let Some((mods, _)) = app.modifier_hint else {
        return;
    };
    let focus_ctx = crate::keys::focus_to_context(app.focus);
    let bindings = crate::keys::bindings_using_modifiers(mods, focus_ctx);
    if bindings.is_empty() {
        return;
    }

    let full = f.area();
    let width = 52.min(full.width.saturating_sub(2));
    // Cap visible rows so huge modifier families stay readable
    const MAX_ROWS: u16 = 14;
    let rows = (bindings.len() as u16).min(MAX_ROWS);
    let height = rows + 2; // title row + list
    let area = Rect {
        x: full.x + (full.width - width) / 2,
        y: full.y + 1,
        width,
        height,
    };

    f.render_widget(Clear, area);
    let mut title = String::from(" ");
    for name in app.modifier_hint_names() {
        title.push_str(name);
        title.push('+');
    }
    title.push_str("keybindings ");

    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme.accent)
                .bg(theme.table_header_bg)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.bg));
    f.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    for (row, binding) in bindings.iter().take(MAX_ROWS as usize).enumerate() {
        let mut spans = vec![Span::styled(
            format!(" {:<14}", binding.key),
            Style::default()
                .fg(theme.status_key)
                .add_modifier(Modifier::BOLD),
        )];
        spans.push(Span::styled(
            truncate_hint_desc(binding.desc, inner.width as usize - 15),
            Style::default().fg(theme.fg),
        ));
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                x: inner.x,
                y: inner.y + row as u16,
                width: inner.width,
                height: 1,
            },
        );
    }

    if bindings.len() > MAX_ROWS as usize {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" … {} more (? for all)", bindings.len() - MAX_ROWS as usize),
                Style::default().fg(theme.border),
            ))),
            Rect {
                x: inner.x,
                y: inner.y + MAX_ROWS - 1,
                width: inner.width,
                height: 1,
            },
        );
    }
}

fn truncate_hint_desc(desc: &str, max: usize) -> String {
    if max <= 3 {
        return String::new();
    }
    if desc.chars().count() <= max {
        return desc.to_string();
    }
    let mut s: String = desc.chars().take(max - 2).collect();
    s.push('…');
    s
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
