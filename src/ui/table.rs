use crate::app::{App, Focus, Tab};
use crate::config::{SortColumn, SortDirection};
use crate::fs::FileKind;
use crate::theme::Theme;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

#[derive(Debug, Clone)]
pub struct ColumnHeaderRect {
    pub column: SortColumn,
    pub x: u16,
    pub width: u16,
}

/// Shared table configuration snapshotted from `App` so tables can render
/// for any tab without borrow conflicts.
#[derive(Debug, Clone, Copy)]
pub struct TableConfig {
    pub sort_column: SortColumn,
    pub sort_direction: SortDirection,
    pub name_column_width_override: Option<u16>,
    pub icon_style: crate::icons::IconStyle,
    pub ls_colors_enabled: bool,
}

impl TableConfig {
    pub fn from_app(app: &App) -> Self {
        Self {
            sort_column: app.sort_column,
            sort_direction: app.sort_direction,
            name_column_width_override: app.name_column_width_override,
            icon_style: app.icon_style,
            ls_colors_enabled: app.ls_colors_enabled,
        }
    }
}

/// Renders one directory table for the given tab. Shared look-and-feel comes
/// from `TableConfig`; listing state lives in `Tab` so both single-tab and
/// dual-pane layouts share this code path.
/// Returns recorded column-header rects for mouse hit-testing.
pub fn render_table(
    f: &mut Frame,
    cfg: TableConfig,
    tab: &mut Tab,
    area: Rect,
    theme: &Theme,
    focused: bool,
) -> Vec<ColumnHeaderRect> {
    let in_search = tab.search_mode;

    let title = if in_search {
        format!(
            " 󰍟 Search \"{}\" ({} matches{}) ",
            tab.search_query,
            tab.search_matches.len(),
            if tab.search_running {
                ", searching…"
            } else {
                ""
            }
        )
    } else {
        format!(
            " {} ({} items{}) ",
            tab.current_dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
            tab.filtered_indices.len(),
            if tab.multi_selected.is_empty() {
                String::new()
            } else {
                format!(", {} selected", tab.multi_selected.len())
            }
        )
    };

    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(if focused {
                theme.border_focus
            } else {
                theme.border
            }),
        ))
        .borders(Borders::ALL)
        .border_style(theme.style_border(focused))
        .style(Style::default().bg(theme.bg));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if inner_area.height < 2 || inner_area.width < 10 {
        return Vec::new();
    }

    // Header sort indicators
    let sort_col = cfg.sort_column;
    let sort_dir = cfg.sort_direction;

    let sort_icon = |col: SortColumn| -> &'static str {
        if sort_col == col {
            match sort_dir {
                SortDirection::Ascending => " ▲",
                SortDirection::Descending => " ▼",
            }
        } else {
            ""
        }
    };

    let header_cells = vec![
        Cell::from("  "),
        Cell::from(" "),
        Cell::from(format!("Name{}", sort_icon(SortColumn::Name))),
        Cell::from(format!("Size{}", sort_icon(SortColumn::Size))),
        Cell::from(format!("Modified{}", sort_icon(SortColumn::Modified))),
        Cell::from(format!("Permissions{}", sort_icon(SortColumn::Permissions))),
    ];

    let header = Row::new(header_cells).style(theme.style_header()).height(1);

    // Calculate column constraints (manual Name width override wins when set)
    let total_width = inner_area.width;
    let fixed_width = 2 + 2 + 10 + 17 + 11;
    let auto_name_width = total_width.saturating_sub(fixed_width).max(18);
    let name_width = cfg.name_column_width_override.unwrap_or(auto_name_width);
    if focused {
        let _ = name_width;
    }

    let constraints = [
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(name_width),
        Constraint::Length(10),
        Constraint::Length(17),
        Constraint::Length(11),
    ];

    // Store column header click rects for mouse sorting
    let mut header_rects = Vec::new();
    let mut curr_x = inner_area.x;

    header_rects.push(ColumnHeaderRect {
        column: SortColumn::Name,
        x: curr_x + 4,
        width: name_width,
    });
    curr_x += 4 + name_width;

    header_rects.push(ColumnHeaderRect {
        column: SortColumn::Size,
        x: curr_x,
        width: 10,
    });
    curr_x += 10;

    header_rects.push(ColumnHeaderRect {
        column: SortColumn::Modified,
        x: curr_x,
        width: 17,
    });
    curr_x += 17;

    header_rects.push(ColumnHeaderRect {
        column: SortColumn::Permissions,
        x: curr_x,
        width: 11,
    });

    // Build visible rows
    let visible_rows_count = (inner_area.height.saturating_sub(1)) as usize;
    let total_items = tab.visible_len();

    // Adjust scroll offset
    if tab.table_selected_index >= tab.table_scroll_offset + visible_rows_count {
        tab.table_scroll_offset = tab.table_selected_index - visible_rows_count + 1;
    } else if tab.table_selected_index < tab.table_scroll_offset {
        tab.table_scroll_offset = tab.table_selected_index;
    }

    if total_items == 0 {
        let empty_msg = if in_search {
            format!("No matches for \"{}\"", tab.search_query)
        } else if tab.search_query.is_empty() {
            "Empty directory".to_string()
        } else {
            "No files match the filter".to_string()
        };
        let p = Paragraph::new(format!("\n  {}", empty_msg)).style(
            Style::default()
                .fg(theme.status_fg)
                .add_modifier(Modifier::ITALIC),
        );
        f.render_widget(p, inner_area);
        return header_rects;
    }

    let row_indices: Vec<usize> = if in_search {
        (tab.table_scroll_offset..total_items.min(tab.table_scroll_offset + visible_rows_count))
            .collect()
    } else {
        tab.filtered_indices
            .iter()
            .skip(tab.table_scroll_offset)
            .take(visible_rows_count)
            .copied()
            .collect()
    };

    let rows: Vec<Row> = row_indices
        .iter()
        .enumerate()
        .filter_map(|(rel_idx, &item_idx)| {
            let entry = if in_search {
                tab.search_matches.get(item_idx)?
            } else {
                // item_idx is already the resolved index into entries
                tab.entries.get(item_idx)?
            };
            let is_selected =
                tab.table_scroll_offset + rel_idx == tab.table_selected_index && focused;
            let is_multi_checked = tab.multi_selected.contains(&entry.path);

            let check_icon = crate::icons::checked_box_icon(cfg.icon_style);
            let icon_str = crate::icons::file_icon(entry, cfg.icon_style);

            let icon_color = match entry.kind {
                FileKind::Directory => theme.file_dir,
                FileKind::Symlink => theme.file_symlink,
                FileKind::Executable => theme.file_exec,
                FileKind::Archive => theme.file_archive,
                FileKind::Image => theme.file_image,
                FileKind::Audio => theme.file_image,
                FileKind::Video => theme.file_image,
                FileKind::Document => theme.file_doc,
                FileKind::Code => theme.accent,
                FileKind::Regular => {
                    if entry.is_hidden {
                        theme.file_hidden
                    } else {
                        theme.file_regular
                    }
                }
            };

            let theme_name_style = if entry.is_dir {
                Style::default()
                    .fg(theme.file_dir)
                    .add_modifier(Modifier::BOLD)
            } else if entry.is_executable {
                Style::default()
                    .fg(theme.file_exec)
                    .add_modifier(Modifier::BOLD)
            } else if entry.is_hidden {
                Style::default().fg(theme.file_hidden)
            } else {
                Style::default().fg(theme.fg)
            };
            // `$LS_COLORS` overrides the theme palette when enabled and a
            // rule matched; the theme's bold-on-dir/executable emphasis is
            // preserved on top.
            let name_style = if cfg.ls_colors_enabled {
                match crate::icons::ls_colors_style(&entry.path, entry.is_dir, entry.is_executable)
                {
                    Some(ls_style) => {
                        // Keep the desktop-style emphasis on dirs/executables
                        // even when an $LS_COLORS rule supplies the color.
                        if entry.is_dir || entry.is_executable {
                            ls_style.add_modifier(Modifier::BOLD)
                        } else {
                            ls_style
                        }
                    }
                    None => theme_name_style,
                }
            } else {
                theme_name_style
            };

            let row_cells = vec![
                Cell::from(Span::styled(check_icon, Style::default().fg(theme.accent))),
                Cell::from(Span::styled(icon_str, Style::default().fg(icon_color))),
                Cell::from(Span::styled(entry.name.as_str(), name_style)),
                Cell::from(Span::styled(
                    entry.formatted_size(),
                    Style::default().fg(theme.status_fg),
                )),
                Cell::from(Span::styled(
                    entry.formatted_modified(),
                    Style::default().fg(theme.status_fg),
                )),
                Cell::from(Span::styled(
                    entry.formatted_permissions(),
                    Style::default()
                        .fg(theme.status_fg)
                        .add_modifier(Modifier::DIM),
                )),
            ];

            let visual_idx = tab.table_scroll_offset + rel_idx;
            let row_style = if is_selected {
                theme.style_selected()
            } else if is_multi_checked {
                Style::default()
                    .bg(theme.selection_bg)
                    .fg(theme.selection_fg)
            } else if visual_idx % 2 == 1 {
                Style::default().bg(theme.table_row_alt_bg)
            } else {
                Style::default().bg(theme.bg)
            };

            Some(Row::new(row_cells).style(row_style).height(1))
        })
        .collect();

    let table = Table::new(rows, constraints)
        .header(header)
        .style(Style::default().bg(theme.bg));

    f.render_widget(table, inner_area);
    header_rects
}

/// Convenience wrapper used by the single-pane layout.
pub fn render_active_table(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let focused = app.focus == Focus::MainTable;
    let cfg = TableConfig::from_app(app);
    let rects = render_table(f, cfg, app.tab_mut(), area, theme, focused);
    if focused {
        app.table_header_rects = rects;
        app.name_column_effective_width = effective_name_width(&cfg, area.width);
    }
}

/// Recomputes the effective Name column width (mirrors render_table logic).
pub fn effective_name_width(cfg: &TableConfig, total_width: u16) -> u16 {
    let fixed_width = 2 + 2 + 10 + 17 + 11;
    let auto_name_width = total_width.saturating_sub(fixed_width).max(18);
    cfg.name_column_width_override.unwrap_or(auto_name_width)
}
