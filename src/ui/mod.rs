use crate::app::{App, Focus};
use crate::theme::Theme;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub mod breadcrumb;
pub mod dialogs;
pub mod markdown;
pub mod preview;
pub mod table;
pub mod tree;

pub fn render(f: &mut Frame, app: &mut App, theme: &Theme) {
    // Mouse hit-test rects are re-recorded every frame; panels that are
    // hidden this frame must not respond to clicks
    app.sidebar_rect = Rect::default();
    app.preview_rect = Rect::default();

    let show_tab_bar = app.tabs.len() > 1;
    let filter_visible = app.focus == Focus::FilterInput || !app.tab().search_query.is_empty();

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Top Bar: Breadcrumbs & Path Input
            Constraint::Length(if show_tab_bar { 1 } else { 0 }), // Optional Tab Bar
            Constraint::Min(5),    // Main Content Area
            Constraint::Length(if filter_visible { 3 } else { 0 }), // Optional Filter Input Bar
            Constraint::Length(1), // Bottom Status Bar
        ])
        .split(f.area());

    // 1. Render Top Breadcrumb Bar
    breadcrumb::render_breadcrumb(f, app, main_layout[0], theme);

    // 2. Render Tab Bar (only when more than one tab is open)
    if show_tab_bar {
        render_tab_bar(f, app, main_layout[1], theme);
    }

    let content = main_layout[2];

    // 3. Main content: dual-pane (Commander) mode takes over the full area;
    // otherwise the responsive Sidebar | Table | Preview split applies.
    if app.dual_pane {
        render_dual_pane(f, app, content, theme);
    } else {
        render_single_pane(f, app, content, theme);
    }

    // 4. Filter Bar (if open)
    if filter_visible {
        dialogs::render_filter_bar(f, app, main_layout[3], theme);
    }

    // 5. Bottom Status Bar
    dialogs::render_status_bar(f, app, main_layout[4], theme);

    // 6. Floating layers: sibling popover, context menu, job queue, help modal, dialogs topmost
    breadcrumb::render_breadcrumb_popover(f, app, theme);
    dialogs::render_context_menu(f, app, theme);
    dialogs::render_job_queue(f, app, theme);
    dialogs::render_help_modal(f, app, theme);
    dialogs::render_dialog_modal(f, app, theme);
    dialogs::render_modifier_hint(f, app, theme);
}

fn render_single_pane(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    use crate::config::PreviewDock;

    let show_sidebar = app.show_sidebar;
    let show_preview = app.show_preview;
    let dock = app.preview_dock;

    // Bottom dock: split vertically first, then horizontally for sidebar+table
    if show_preview && dock == PreviewDock::Bottom {
        let v_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(100 - app.preview_height_percent),
                Constraint::Percentage(app.preview_height_percent),
            ])
            .split(area);

        // Top row: sidebar + table
        let top_constraints = if show_sidebar {
            vec![
                Constraint::Percentage(app.sidebar_width_percent),
                Constraint::Min(20),
            ]
        } else {
            vec![Constraint::Min(20)]
        };

        let top_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(top_constraints)
            .split(v_split[0]);

        let mut table_rect = top_layout[0];
        if show_sidebar {
            tree::render_sidebar(f, app, top_layout[0], theme);
            table_rect = top_layout[1];
        }

        table::render_active_table(f, app, table_rect, theme);
        app.table_rect = table_rect;

        // Bottom row: preview
        preview::render_preview(f, app, v_split[1], theme);
        return;
    }

    // Side dock (default): horizontal split
    let center_constraints = match (show_sidebar, show_preview) {
        (true, true) => vec![
            Constraint::Percentage(app.sidebar_width_percent),
            Constraint::Min(20),
            Constraint::Percentage(app.preview_width_percent),
        ],
        (true, false) => vec![
            Constraint::Percentage(app.sidebar_width_percent),
            Constraint::Min(20),
        ],
        (false, true) => vec![
            Constraint::Min(20),
            Constraint::Percentage(app.preview_width_percent),
        ],
        (false, false) => vec![Constraint::Min(20)],
    };

    let center_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(center_constraints)
        .split(area);

    let mut table_rect = center_layout[0];

    if show_sidebar && show_preview {
        tree::render_sidebar(f, app, center_layout[0], theme);
        table_rect = center_layout[1];
        preview::render_preview(f, app, center_layout[2], theme);
    } else if show_sidebar {
        tree::render_sidebar(f, app, center_layout[0], theme);
        table_rect = center_layout[1];
    } else if show_preview {
        table_rect = center_layout[0];
        preview::render_preview(f, app, center_layout[1], theme);
    }

    table::render_active_table(f, app, table_rect, theme);
    app.table_rect = table_rect;
}

fn render_dual_pane(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let cfg = table::TableConfig::from_app(app);

    for pane in 0..2 {
        if pane >= app.tabs.len() {
            break;
        }
        let focused = app.active_tab == pane && app.focus == Focus::MainTable;
        let rects = table::render_table(f, cfg, &mut app.tabs[pane], columns[pane], theme, focused);
        if focused {
            app.table_header_rects = rects;
        }
    }
    app.pane_rects = [columns[0], columns[1]];
    // Row/header hit-testing below uses the active pane's geometry
    app.table_rect = columns[app.active_tab.min(1)];
}

/// Horizontal strip of tab chips; the active tab is highlighted. Click
/// hit-test rects are recorded into `app.tab_chips`.
fn render_tab_bar(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    use crate::app::TabChip;

    let mut chips = Vec::new();
    let mut x = area.x;
    for (i, tab) in app.tabs.iter().enumerate() {
        let name = tab
            .current_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let label = format!(" {} ", name);
        let width = (label.chars().count() as u16 + 2).min(area.width.saturating_sub(x + area.x));
        if width == 0 {
            break;
        }
        let active = i == app.active_tab;
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                label,
                Style::default()
                    .fg(if active {
                        theme.selection_fg
                    } else {
                        theme.status_fg
                    })
                    .bg(if active { theme.selection_bg } else { theme.bg })
                    .add_modifier(if active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ))),
            Rect {
                x,
                y: area.y,
                width,
                height: 1,
            },
        );
        chips.push(TabChip {
            index: i,
            rect: Rect {
                x,
                y: area.y,
                width,
                height: 1,
            },
        });
        x += width;
        if x >= area.width {
            break;
        }
    }
    app.tab_chips = chips;
}
