use crate::config::{Config, SortColumn, SortDirection, UserAction};
use crate::event::AppEvent;
use crate::fs::ops::{create_directory, create_file, move_to_trash, rename_entry};
use crate::fs::scanner::AsyncScanner;
use crate::fs::watcher::DirectoryWatcher;
use crate::fs::worker::{CollisionMode, OpsWorker};
use crate::fs::{sort_entries, FileEntry};
use crate::ui::breadcrumb::BreadcrumbSegment;
use crate::ui::table::ColumnHeaderRect;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    MainTable,
    Breadcrumb,
    PathInput,
    FilterInput,
    Preview,
}

/// One directory view: its location, navigation history, listing state, and
/// selection/search state. Tabs are fully isolated; the active tab's fields
/// are accessed via `App::tab()` / `App::tab_mut()`.
#[derive(Debug, Clone)]
pub struct Tab {
    pub id: u64,
    pub current_dir: PathBuf,
    pub back_history: Vec<PathBuf>,
    pub forward_history: Vec<PathBuf>,

    pub entries: Vec<FileEntry>,
    pub filtered_indices: Vec<usize>,
    pub table_selected_index: usize,
    pub table_scroll_offset: usize,
    /// File name to place the cursor on once the next scan completes
    /// (set when navigating up so the folder we left is highlighted)
    pub pending_cursor_target: Option<String>,
    pub multi_selected: HashSet<PathBuf>,
    /// Cursor position where the current range selection started (Shift+arrows)
    pub selection_anchor: Option<usize>,

    // Recursive search (Ctrl+F)
    pub search_mode: bool,
    pub search_query: String,
    pub search_cursor: usize,
    active_search_id: u64,
    pub search_matches: Vec<FileEntry>,
    pub search_running: bool,
}

thread_local! {
    static NEXT_TAB_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
}

impl Tab {
    fn new(dir: PathBuf) -> Self {
        Self {
            id: NEXT_TAB_ID.with(|c| {
                let id = c.get();
                c.set(id + 1);
                id
            }),
            current_dir: dir,
            back_history: Vec::new(),
            forward_history: Vec::new(),
            entries: Vec::new(),
            filtered_indices: Vec::new(),
            table_selected_index: 0,
            table_scroll_offset: 0,
            pending_cursor_target: None,
            multi_selected: HashSet::new(),
            selection_anchor: None,
            search_mode: false,
            search_query: String::new(),
            search_cursor: 0,
            active_search_id: 0,
            search_matches: Vec::new(),
            search_running: false,
        }
    }

    /// Number of rows currently shown (directory listing or search results)
    pub fn visible_len(&self) -> usize {
        if self.search_mode {
            self.search_matches.len()
        } else {
            self.filtered_indices.len()
        }
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        if self.search_mode {
            return self.search_matches.get(self.table_selected_index);
        }
        let actual_idx = *self.filtered_indices.get(self.table_selected_index)?;
        self.entries.get(actual_idx)
    }

    pub(crate) fn visible_entry_at(&self, idx: usize) -> Option<&FileEntry> {
        if self.search_mode {
            return self.search_matches.get(idx);
        }
        self.entries.get(*self.filtered_indices.get(idx)?)
    }
}

#[derive(Debug, Clone)]
pub struct Bookmark {
    pub name: String,
    pub path: PathBuf,
    pub icon: &'static str,
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub depth: usize,
    pub is_expanded: bool,
    pub has_children: bool,
    pub children_loaded: bool,
}

/// Sibling-directory popover anchored to a breadcrumb chip.
#[derive(Debug, Clone)]
pub struct BreadcrumbPopover {
    pub items: Vec<PathBuf>,
    pub selected: usize,
    /// First visible item index, maintained by the renderer while scrolling
    pub scroll_offset: usize,
    pub max_visible: usize,
    /// Screen rect of the rendered popup, filled in by the renderer for hit-testing
    pub screen_rect: Rect,
}

struct ColumnDrag {
    start_x: u16,
    base_width: u16,
}

/// A left-button press on a table row that may become a drag. Promoted to a
/// full `DragDropState` once the cursor passes the activation threshold.
#[derive(Debug, Clone)]
struct DragCandidate {
    start: (u16, u16),
    paths: Vec<PathBuf>,
}

/// Active drag-and-drop gesture: sources are snapshotted at press time
/// (the whole multi-selection if the pressed row is part of it). Dropping is
/// resolved on mouse-up over sidebar folders or breadcrumb chips.
#[derive(Debug, Clone)]
pub struct DragDropState {
    pub start: (u16, u16),
    pub paths: Vec<PathBuf>,
    /// Ctrl held while dragging copies instead of moves
    pub copy: bool,
    /// Latest cursor position, for drop resolution and status feedback
    pub hover: (u16, u16),
}

/// How the transfer pipeline resolves destination-name collisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    Overwrite,
    Skip,
    AutoRename,
}

/// What a confirmed dialog should execute. Paths are snapshotted when the
/// dialog opens so later navigation cannot redirect the operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogAction {
    Trash(Vec<PathBuf>),
    DeletePermanently(Vec<PathBuf>),
    /// Rename the entry to the text entered in the dialog's prompt
    Rename(PathBuf),
    CreateFolder(PathBuf),
    CreateFile(PathBuf),
    /// Re-run a failed batch transfer with the given remaining sources
    RetryTransfer {
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
        cut: bool,
    },
    None,
}

/// One clickable button in a modal dialog.
#[derive(Debug, Clone)]
pub struct DialogButton {
    pub label: String,
    pub kind: ButtonKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ButtonKind {
    Cancel,
    /// Executes `Dialog.action` (with prompt validation for prompt dialogs)
    Confirm,
    /// Resolves the pending transfer conflict and submits the job
    Resolve(ConflictResolution),
}

impl DialogButton {
    fn cancel() -> Self {
        Self {
            label: "Cancel".to_string(),
            kind: ButtonKind::Cancel,
        }
    }

    fn confirm(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: ButtonKind::Confirm,
        }
    }

    fn resolve(label: impl Into<String>, resolution: ConflictResolution) -> Self {
        Self {
            label: label.into(),
            kind: ButtonKind::Resolve(resolution),
        }
    }
}

/// Editable single-line input for prompt dialogs (rename / new folder).
#[derive(Debug, Clone)]
pub struct PromptState {
    pub buffer: String,
    pub cursor: usize,
}

impl PromptState {
    fn new(initial: impl Into<String>) -> Self {
        let buffer = initial.into();
        let cursor = buffer.len();
        Self { buffer, cursor }
    }
}

/// Action dispatched when a context menu entry is activated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    Open,
    Copy,
    Cut,
    Paste,
    NewFolder,
    NewFile,
    Rename,
    MoveToTrash,
    DeletePermanently,
    Properties,
    ExtractHere,
    /// Runs the user action at the given index in `App::user_actions`
    UserAction(usize),
}

/// One row of the floating context menu; `action == None` renders a separator.
#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    pub label: String,
    pub action: Option<ContextAction>,
}

impl ContextMenuItem {
    fn separator() -> Self {
        Self {
            label: String::new(),
            action: None,
        }
    }

    fn enabled(label: impl Into<String>, action: ContextAction) -> Self {
        Self {
            label: label.into(),
            action: Some(action),
        }
    }

    pub fn is_separator(&self) -> bool {
        self.action.is_none()
    }
}

/// Floating right-click menu anchored to a screen position. The renderer
/// clamps the popup to the terminal and records `screen_rect` for hit-testing.
#[derive(Debug, Clone)]
pub struct ContextMenu {
    /// Requested anchor (may be adjusted by the renderer to stay on-screen)
    pub anchor_x: u16,
    pub anchor_y: u16,
    pub items: Vec<ContextMenuItem>,
    pub selected: usize,
    /// First visible item index, maintained while scrolling
    pub scroll_offset: usize,
    pub max_visible: usize,
    pub screen_rect: Rect,
    /// When set, actions apply to this explicit path instead of the table
    /// selection (sidebar / breadcrumb menus)
    pub target: Option<PathBuf>,
}

/// Modal confirmation/prompt dialog with an arbitrary set of buttons.
/// `selected_button` tracks keyboard focus; confirmation dialogs default to
/// Cancel for safety, prompt dialogs default to their Confirm button.
/// When `prompt` is set, typing edits the input line instead.
/// The renderer fills in the screen rects for mouse hit-testing.
#[derive(Debug, Clone)]
pub struct Dialog {
    pub title: String,
    pub message: Vec<String>,
    pub buttons: Vec<DialogButton>,
    pub selected_button: usize,
    pub destructive: bool,
    pub action: DialogAction,
    pub prompt: Option<PromptState>,
    /// Outer area of the rendered dialog, filled in by the renderer
    pub screen_area: Rect,
    /// Per-button screen rects, aligned with `buttons`
    pub button_rects: Vec<Rect>,
}

impl Dialog {
    /// Confirmation dialog: Cancel is listed first and focused by default so
    /// a stray Enter never confirms a potentially destructive operation.
    pub fn confirm(
        title: impl Into<String>,
        message: Vec<String>,
        confirm_label: impl Into<String>,
        destructive: bool,
        action: DialogAction,
    ) -> Self {
        Self {
            title: title.into(),
            message,
            buttons: vec![DialogButton::cancel(), DialogButton::confirm(confirm_label)],
            selected_button: 0,
            destructive,
            action,
            prompt: None,
            screen_area: Rect::default(),
            button_rects: Vec::new(),
        }
    }

    /// Conflict-resolution dialog: Overwrite / Skip / Auto-Rename / Cancel.
    pub fn conflict(title: impl Into<String>, message: Vec<String>) -> Self {
        Self {
            title: title.into(),
            message,
            buttons: vec![
                DialogButton::resolve("Overwrite", ConflictResolution::Overwrite),
                DialogButton::resolve("Skip", ConflictResolution::Skip),
                DialogButton::resolve("Auto-Rename", ConflictResolution::AutoRename),
                DialogButton::cancel(),
            ],
            selected_button: 3,
            destructive: true,
            action: DialogAction::None,
            prompt: None,
            screen_area: Rect::default(),
            button_rects: Vec::new(),
        }
    }

    /// Failure dialog offering to retry the failed subset of a batch.
    pub fn retry(
        message: Vec<String>,
        failed_sources: Vec<PathBuf>,
        dest_dir: PathBuf,
        cut: bool,
    ) -> Self {
        Self {
            title: " 󰀦 Operation Failed ".to_string(),
            message,
            buttons: vec![
                DialogButton::confirm("Retry Failed"),
                DialogButton::cancel(),
            ],
            selected_button: 0,
            destructive: false,
            action: DialogAction::RetryTransfer {
                sources: failed_sources,
                dest_dir,
                cut,
            },
            prompt: None,
            screen_area: Rect::default(),
            button_rects: Vec::new(),
        }
    }

    pub fn prompt(
        title: impl Into<String>,
        confirm_label: impl Into<String>,
        initial_text: impl Into<String>,
        action: DialogAction,
    ) -> Self {
        let mut dialog = Self {
            title: title.into(),
            message: Vec::new(),
            buttons: vec![DialogButton::confirm(confirm_label), DialogButton::cancel()],
            selected_button: 0,
            destructive: false,
            action,
            prompt: Some(PromptState::new(initial_text)),
            screen_area: Rect::default(),
            button_rects: Vec::new(),
        };
        dialog.selected_button = 0; // Confirm first & focused (typing then Enter flows)
        dialog
    }

    fn validate_input(&self) -> Result<(), String> {
        let Some(prompt) = &self.prompt else {
            return Ok(());
        };
        let name = prompt.buffer.trim();
        if name.is_empty() {
            return Err("Name cannot be empty".to_string());
        }
        if name == "." || name == ".." || name.contains('/') || name.contains('\0') {
            return Err(format!("Invalid name: {:?}", name));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct StatusMessageState {
    pub text: String,
    pub is_error: bool,
    pub created_at: Instant,
    pub duration: Duration,
}

const PREVIEW_MAX_BYTES: u64 = 512 * 1024;
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(300);
/// How long the modifier discovery popup stays visible without the Kitty
/// keyboard protocol (which would let us detect modifier release directly)
const MODIFIER_HINT_DURATION: Duration = Duration::from_millis(1500);

/// Cut/copy clipboard for paste operations.
#[derive(Debug, Clone)]
pub struct Clipboard {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
}

/// Live progress of one submitted background operation.
#[derive(Debug, Clone)]
pub struct ActiveOp {
    pub id: u64,
    pub label: String,
    pub done: usize,
    pub total: usize,
    pub current: String,
    /// Cancel was requested; the job stops at the next item boundary
    pub cancelling: bool,
}

/// A transfer awaiting a conflict-resolution decision before submission.
#[derive(Debug, Clone)]
pub struct PendingTransfer {
    pub sources: Vec<PathBuf>,
    pub dest_dir: PathBuf,
    pub cut: bool,
}

/// Screen rect of one rendered tab-bar chip, recorded for mouse hit-testing.
#[derive(Debug, Clone, Copy)]
pub struct TabChip {
    pub index: usize,
    pub rect: Rect,
}

/// Completion summary of one background operation, mirroring the fields of
/// `AppEvent::OpsFinished`.
pub struct OpsOutcome {
    pub job_id: u64,
    pub label: String,
    pub succeeded: usize,
    pub skipped: usize,
    pub errors: Vec<(PathBuf, String)>,
    pub dest: Option<PathBuf>,
    pub cancelled: bool,
}

impl OpsOutcome {
    pub fn from_event(
        job_id: u64,
        label: String,
        succeeded: usize,
        skipped: usize,
        errors: Vec<(PathBuf, String)>,
        dest: Option<PathBuf>,
        cancelled: bool,
    ) -> Self {
        Self {
            job_id,
            label,
            succeeded,
            skipped,
            errors,
            dest,
            cancelled,
        }
    }
}

pub struct App {
    /// All open tabs; the active tab's state is accessed via `tab()`/`tab_mut()`
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    /// Dual-pane (Commander) mode: tabs[0] left, tabs[1] right
    pub dual_pane: bool,
    pub pane_rects: [Rect; 2],
    pub tab_chips: Vec<TabChip>,

    pub bookmarks: Vec<Bookmark>,
    pub tree_nodes: Vec<TreeNode>,
    pub sidebar_selected_index: usize,
    pub sidebar_scroll_offset: usize,
    pub sidebar_rendered_paths: Vec<Option<PathBuf>>,

    pub focus: Focus,
    pub show_sidebar: bool,
    pub sidebar_width_percent: u16,
    pub show_preview: bool,
    pub preview_width_percent: u16,
    pub preview_dock: crate::config::PreviewDock,
    pub preview_height_percent: u16,
    pub show_hidden: bool,
    pub show_help: bool,
    pub dialog: Option<Dialog>,

    pub sort_column: SortColumn,
    pub sort_direction: SortDirection,
    pub dirs_first: bool,
    pub natural_sort: bool,

    pub path_input_buffer: String,
    pub path_input_cursor: usize,

    pub breadcrumb_segments: Vec<BreadcrumbSegment>,
    pub breadcrumb_selected: usize,
    pub breadcrumb_popover: Option<BreadcrumbPopover>,
    pub context_menu: Option<ContextMenu>,
    pub table_header_rects: Vec<ColumnHeaderRect>,
    pub name_column_effective_width: u16,
    pub name_column_width_override: Option<u16>,
    column_drag: Option<ColumnDrag>,
    drag_candidate: Option<DragCandidate>,
    pub drag_drop: Option<DragDropState>,
    pub table_rect: Rect,
    /// Widget areas recorded during render for mouse hit-testing
    pub sidebar_rect: Rect,
    pub preview_rect: Rect,

    pub disk_free: Option<u64>,

    pub status_message: Option<StatusMessageState>,
    pub clipboard: Option<Clipboard>,
    pub active_ops: Vec<ActiveOp>,
    pub pending_transfer: Option<PendingTransfer>,
    /// Background job queue overlay (`Ctrl+J`)
    pub show_job_queue: bool,
    pub job_queue_selected: usize,
    /// Widget areas recorded during render for mouse hit-testing
    pub job_queue_rect: Rect,
    pub job_queue_cancel_rects: Vec<Rect>,
    /// User-defined launcher actions from [[actions]] config tables
    pub user_actions: Vec<UserAction>,
    /// Pre-parsed keybinding remaps: (from_mods, from_code, to_mods, to_code)
    parsed_remaps: Vec<(
        crossterm::event::KeyModifiers,
        crossterm::event::KeyCode,
        crossterm::event::KeyModifiers,
        crossterm::event::KeyCode,
    )>,
    /// Icon rendering style (Nerd Fonts / Unicode / ASCII)
    pub icon_style: crate::icons::IconStyle,
    /// Whether filenames are colored from `$LS_COLORS`
    pub ls_colors_enabled: bool,
    pub should_quit: bool,

    // Scan bookkeeping
    pub active_scan_id: u64,
    applied_scan_id: u64,
    pending_refresh: Option<Instant>,

    // Preview state (async-loaded)
    pub preview_loaded_path: Option<PathBuf>,
    pub preview_pending_path: Option<PathBuf>,
    /// Outer `Some` = loaded for the path above; inner `None` = binary/unreadable
    pub preview_text: Option<Option<String>>,
    /// Syntax-highlighted lines matching the loaded text above; `None` when
    /// no syntax definition matched (plain fallback rendering)
    pub preview_styled: Option<Vec<ratatui::text::Line<'static>>>,
    /// Render state for graphics-protocol image previews; rebuilt whenever a
    /// new image preview loads. Font size comes from `image_picker`.
    pub preview_image_protocol: Option<StatefulProtocol>,
    /// Pixel dimensions of the loaded preview image, used to center and
    /// aspect-fit the render area
    pub preview_image_dims: Option<(u32, u32)>,
    /// Hex-dump preview of binary content, aligned with `preview_loaded_path`
    pub preview_hex: Option<String>,
    /// MIME type from the metadata inspector (off-thread computed)
    pub preview_mime: Option<String>,
    /// SHA-256 digest from the metadata inspector
    pub preview_sha256: Option<String>,
    /// Modifier discovery popup: modifiers of an unbound chord + shown-at
    pub modifier_hint: Option<(KeyModifiers, Instant)>,
    /// Terminal graphics capabilities (protocol + font size), probed at
    /// startup and replaceable for tests / headless fallbacks
    pub image_picker: Picker,

    pub event_tx: UnboundedSender<AppEvent>,
    pub scanner: AsyncScanner,
    pub watcher: DirectoryWatcher,
    pub ops_worker: OpsWorker,

    last_click_time: Instant,
    last_click_pos: (u16, u16),
}

impl App {
    /// The active tab (panics only if tabs were removed unsafely — never).
    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    pub fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    pub fn new(
        initial_path: Option<PathBuf>,
        config: &Config,
        event_tx: UnboundedSender<AppEvent>,
    ) -> Self {
        let current_dir = initial_path
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")))
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from("/"));

        let icon_style = crate::icons::IconStyle::resolve_config(&config.general.icon_style);

        let mut bookmarks = Vec::new();
        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(&home);
            bookmarks.push(Bookmark {
                name: "Home".to_string(),
                path: home_path.clone(),
                icon: crate::icons::bookmark_icon("Home", icon_style),
            });
            bookmarks.push(Bookmark {
                name: "Documents".to_string(),
                path: home_path.join("Documents"),
                icon: crate::icons::bookmark_icon("Documents", icon_style),
            });
            bookmarks.push(Bookmark {
                name: "Downloads".to_string(),
                path: home_path.join("Downloads"),
                icon: crate::icons::bookmark_icon("Downloads", icon_style),
            });
            let projects_path = home_path.join("Projects");
            if projects_path.exists() {
                bookmarks.push(Bookmark {
                    name: "Projects".to_string(),
                    path: projects_path,
                    icon: crate::icons::bookmark_icon("Projects", icon_style),
                });
            }
        }
        bookmarks.push(Bookmark {
            name: "Root".to_string(),
            path: PathBuf::from("/"),
            icon: crate::icons::bookmark_icon("Root", icon_style),
        });

        let mut tree_nodes = Vec::new();
        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(&home);
            tree_nodes.push(TreeNode {
                name: home_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                path: home_path,
                depth: 0,
                is_expanded: false,
                has_children: true,
                children_loaded: false,
            });
        }
        tree_nodes.push(TreeNode {
            name: "/".to_string(),
            path: PathBuf::from("/"),
            depth: 0,
            is_expanded: false,
            has_children: true,
            children_loaded: false,
        });

        let scanner = AsyncScanner::new(event_tx.clone());
        let watcher = DirectoryWatcher::new();
        let ops_worker = OpsWorker::new(event_tx.clone());

        let mut app = Self {
            tabs: vec![Tab::new(current_dir.clone())],
            active_tab: 0,
            dual_pane: false,
            pane_rects: [Rect::default(); 2],
            tab_chips: Vec::new(),

            bookmarks,
            tree_nodes,
            sidebar_selected_index: 1, // First bookmark item
            sidebar_scroll_offset: 0,
            sidebar_rendered_paths: Vec::new(),

            focus: Focus::MainTable,
            show_sidebar: config.layout.show_sidebar,
            sidebar_width_percent: config.layout.sidebar_width_percent,
            show_preview: config.layout.show_preview,
            preview_width_percent: config.layout.preview_width_percent,
            preview_dock: config.layout.preview_dock,
            preview_height_percent: config.layout.preview_height_percent,
            show_hidden: config.general.show_hidden,
            show_help: false,
            dialog: None,

            sort_column: config.general.default_sort_column,
            sort_direction: config.general.default_sort_direction,
            dirs_first: config.general.dirs_first,
            natural_sort: config.general.natural_sort,

            path_input_buffer: current_dir.to_string_lossy().to_string(),
            path_input_cursor: current_dir.to_string_lossy().len(),

            breadcrumb_segments: Vec::new(),
            breadcrumb_selected: 0,
            breadcrumb_popover: None,
            context_menu: None,
            table_header_rects: Vec::new(),
            name_column_effective_width: 30,
            name_column_width_override: None,
            column_drag: None,
            drag_candidate: None,
            drag_drop: None,
            table_rect: Rect::default(),
            sidebar_rect: Rect::default(),
            preview_rect: Rect::default(),

            disk_free: None,

            status_message: None,
            clipboard: None,
            active_ops: Vec::new(),
            pending_transfer: None,
            show_job_queue: false,
            job_queue_selected: 0,
            job_queue_rect: Rect::default(),
            job_queue_cancel_rects: Vec::new(),
            user_actions: config.actions.clone(),
            parsed_remaps: config
                .remap
                .iter()
                .filter_map(|r| r.parsed())
                .collect(),
            icon_style,
            ls_colors_enabled: config.general.ls_colors_enabled,
            should_quit: false,

            active_scan_id: 0,
            applied_scan_id: 0,
            pending_refresh: None,

            preview_loaded_path: None,
            preview_pending_path: None,
            preview_text: None,
            preview_styled: None,
            preview_image_protocol: None,
            preview_image_dims: None,
            preview_hex: None,
            preview_mime: None,
            preview_sha256: None,
            modifier_hint: None,
            image_picker: Picker::from_fontsize((8, 18)),

            event_tx,
            scanner,
            watcher,
            ops_worker,

            last_click_time: Instant::now(),
            last_click_pos: (0, 0),
        };

        app.navigate_to(current_dir);
        app
    }

    fn load_directory(&mut self, path: PathBuf, push_history: bool) {
        let canonical = path.canonicalize().unwrap_or(path);

        if push_history && self.tab().current_dir != canonical {
            let tab = self.tab_mut();
            tab.back_history.push(tab.current_dir.clone());
            tab.forward_history.clear();
        }
        self.tab_mut().current_dir = canonical;

        self.tab_mut().table_selected_index = 0;
        self.tab_mut().table_scroll_offset = 0;
        self.tab_mut().pending_cursor_target = None;
        self.tab_mut().entries.clear();
        self.tab_mut().filtered_indices.clear();
        self.clear_selection();

        // Reset transient UI state
        self.exit_search_mode();
        self.column_drag = None;
        self.drag_candidate = None;
        self.drag_drop = None;
        self.sync_active_dir_state();
    }

    /// Re-syncs global UI state to the active tab's directory: path bar,
    /// transient overlays, disk metrics, filesystem watcher, and a fresh
    /// scan. Used on navigation and on every tab switch.
    fn sync_active_dir_state(&mut self) {
        let dir = self.tab().current_dir.clone();
        self.path_input_buffer = dir.to_string_lossy().to_string();
        self.path_input_cursor = self.path_input_buffer.len();
        self.breadcrumb_popover = None;
        self.context_menu = None;
        self.breadcrumb_selected = 0;
        self.disk_free = crate::fs::disk_free_bytes(&dir);

        // Start filesystem watcher on new directory
        self.watcher.watch(&dir, self.event_tx.clone());

        // Start async directory scan
        self.active_scan_id = self.scanner.scan_directory(dir);
    }

    // ---- Tab Management ----

    /// Ctrl+T — opens a new tab duplicating the current directory.
    pub fn new_tab(&mut self) {
        let dir = self.tab().current_dir.clone();
        self.tabs.push(Tab::new(dir));
        self.switch_to_tab(self.tabs.len() - 1);
    }

    /// Ctrl+W — closes the active tab (the last one quits the app).
    pub fn close_tab(&mut self) {
        if self.tabs.len() == 1 {
            self.should_quit = true;
            return;
        }
        let idx = self.active_tab;
        self.exit_search_mode(); // cancel the closing tab's search
        self.tabs.remove(idx);
        // Activate the tab that took this slot (right neighbor), clamped
        self.active_tab = idx.min(self.tabs.len() - 1);
        self.sync_active_dir_state();
    }

    /// Ctrl+Tab / Ctrl+Shift+Tab — cycles through open tabs.
    pub fn cycle_tab(&mut self, delta: i32) {
        if self.tabs.len() < 2 {
            return;
        }
        let n = self.tabs.len() as i32;
        let next = (self.active_tab as i32 + delta).rem_euclid(n) as usize;
        self.switch_to_tab(next);
    }

    pub fn switch_to_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() || idx == self.active_tab {
            return;
        }
        self.exit_search_mode(); // don't leave searches running in background tabs
        self.active_tab = idx;
        // Cached entries/selection survive; only freshness is restored
        self.focus = Focus::MainTable;
        self.sync_active_dir_state();
    }

    // ---- Dual-Pane (Commander) Mode ----

    /// F3 — toggles Commander mode: two independent directory panes side by
    /// side (tabs[0] left, tabs[1] right), hiding sidebar/preview panels.
    pub fn toggle_dual_pane(&mut self) {
        self.dual_pane = !self.dual_pane;
        if self.dual_pane {
            // A second pane spawns at the current directory if none exists
            while self.tabs.len() < 2 {
                let dir = self.tab().current_dir.clone();
                self.tabs.push(Tab::new(dir));
            }
            if self.active_tab > 1 {
                self.active_tab = 0;
            }
            self.focus = Focus::MainTable;
            self.context_menu = None;
            self.drag_candidate = None;
            self.drag_drop = None;
        }
    }

    /// Directory of the opposite pane in dual-pane mode.
    pub fn other_pane_dir(&self) -> Option<PathBuf> {
        if !self.dual_pane || self.tabs.len() < 2 {
            return None;
        }
        Some(
            self.tabs[if self.active_tab == 0 { 1 } else { 0 }]
                .current_dir
                .clone(),
        )
    }

    /// F5 — copies the selection to the opposite pane's directory.
    pub fn copy_to_other_pane(&mut self) {
        self.transfer_to_other_pane(false);
    }

    /// F6 — moves the selection to the opposite pane's directory.
    pub fn move_to_other_pane(&mut self) {
        self.transfer_to_other_pane(true);
    }

    fn transfer_to_other_pane(&mut self, cut: bool) {
        let Some(dest) = self.other_pane_dir() else {
            self.set_status_info("Dual-pane mode is off (F3)".to_string());
            return;
        };
        let sources = self.operation_targets();
        if sources.is_empty() {
            return;
        }
        self.prepare_transfer(sources, dest, cut);
    }

    pub fn navigate_to(&mut self, path: PathBuf) {
        self.load_directory(path, true);
    }

    pub fn navigate_up(&mut self) {
        if let Some(parent) = self.tab().current_dir.parent() {
            let parent_path = parent.to_path_buf();
            // Highlight the folder we are leaving once the parent listing loads
            let leaving_name = self
                .tab()
                .current_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string());
            self.navigate_to(parent_path);
            self.tab_mut().pending_cursor_target = leaving_name;
        }
    }

    pub fn navigate_back(&mut self) {
        if let Some(prev) = self.tab_mut().back_history.pop() {
            {
                let tab = self.tab_mut();
                tab.forward_history.push(tab.current_dir.clone());
            }
            self.load_directory(prev, false);
        }
    }

    pub fn navigate_forward(&mut self) {
        if let Some(next) = self.tab_mut().forward_history.pop() {
            {
                let tab = self.tab_mut();
                tab.back_history.push(tab.current_dir.clone());
            }
            self.load_directory(next, false);
        }
    }

    /// Rescans the current directory in the background. Existing entries stay
    /// visible until the first chunk of the fresh scan arrives (no flicker).
    pub fn refresh(&mut self) {
        self.active_scan_id = self.scanner.scan_directory(self.tab().current_dir.clone());
    }

    /// Called from `tick` after watcher activity settles, so event bursts
    /// (e.g. git operations) coalesce into a single refresh.
    pub fn mark_fs_dirty(&mut self, path: &Path) {
        if path == self.tab().current_dir {
            self.pending_refresh = Some(Instant::now());
        }
    }

    /// Applies one streamed chunk from the scanner. Chunks from stale scans
    /// are dropped; the first chunk of a new scan atomically replaces entries.
    pub fn apply_scan_chunk(
        &mut self,
        scan_id: u64,
        path: &Path,
        entries: Vec<FileEntry>,
        is_final: bool,
    ) {
        if scan_id < self.active_scan_id || path != self.tab().current_dir {
            return;
        }
        if self.applied_scan_id != scan_id {
            self.tab_mut().entries.clear();
            self.tab_mut().filtered_indices.clear();
            self.tab_mut().table_selected_index = 0;
            self.tab_mut().table_scroll_offset = 0;
            self.applied_scan_id = scan_id;
        }
        self.tab_mut().entries.extend(entries);
        if is_final {
            self.resort_entries();
            self.settle_pending_cursor();
        } else {
            // Progressive display: show chunks as they arrive; sort on completion
            self.reapply_filter();
        }
    }

    /// Places the table cursor on the entry reserved by `navigate_up` (the
    /// folder just left) after the parent's listing finished sorting. No-op
    /// when the entry vanished between scans.
    fn settle_pending_cursor(&mut self) {
        let Some(name) = self.tab_mut().pending_cursor_target.take() else {
            return;
        };
        let idx = (0..self.tab().visible_len()).find(|&i| {
            self.tab()
                .visible_entry_at(i)
                .is_some_and(|e| e.name.as_str() == name)
        });
        if let Some(idx) = idx {
            let tab = self.tab_mut();
            tab.table_selected_index = idx;
            // Keep the target inside the visible window
            if idx < tab.table_scroll_offset {
                tab.table_scroll_offset = idx;
            }
        }
    }

    /// Number of rows currently shown in the table (directory or search results)
    pub fn visible_len(&self) -> usize {
        if self.tab().search_mode {
            self.tab().search_matches.len()
        } else {
            self.tab().filtered_indices.len()
        }
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        if self.tab().search_mode {
            return self
                .tab()
                .search_matches
                .get(self.tab().table_selected_index);
        }
        let actual_idx = *self
            .tab()
            .filtered_indices
            .get(self.tab().table_selected_index)?;
        self.tab().entries.get(actual_idx)
    }

    // ---- Recursive Search (Ctrl+F) ----

    pub fn begin_recursive_search(&mut self) {
        self.tab_mut().search_mode = true;
        self.tab_mut().search_matches.clear();
        self.tab_mut().search_running = false;
        self.tab_mut().search_query.clear();
        self.tab_mut().search_cursor = 0;
        self.tab_mut().table_selected_index = 0;
        self.tab_mut().table_scroll_offset = 0;
        self.focus = Focus::FilterInput;
    }

    pub fn exit_search_mode(&mut self) {
        if !self.tab().search_mode {
            return;
        }
        self.scanner.cancel_search();
        // Ids start at 1, so 0 can never match an in-flight search's chunks
        self.tab_mut().active_search_id = 0;
        self.tab_mut().search_mode = false;
        self.tab_mut().search_running = false;
        self.tab_mut().search_matches.clear();
        self.tab_mut().table_selected_index = 0;
        self.tab_mut().table_scroll_offset = 0;
    }

    fn restart_search(&mut self) {
        if self.tab().search_query.is_empty() {
            self.tab_mut().search_matches.clear();
            self.tab_mut().search_running = false;
            self.scanner.cancel_search();
            self.tab_mut().active_search_id = 0;
            return;
        }
        self.tab_mut().active_search_id = self.scanner.search_recursive(
            self.tab().current_dir.clone(),
            self.tab().search_query.clone(),
            self.show_hidden,
        );
        self.tab_mut().search_matches.clear();
        self.tab_mut().search_running = true;
        self.tab_mut().table_selected_index = 0;
        self.tab_mut().table_scroll_offset = 0;
    }

    pub fn apply_search_chunk(&mut self, search_id: u64, matches: Vec<FileEntry>, is_final: bool) {
        if search_id != self.tab().active_search_id || !self.tab().search_mode {
            return;
        }
        self.tab_mut().search_matches.extend(matches);
        if is_final {
            self.tab_mut().search_running = false;
        } else {
            let max_idx = self.tab().search_matches.len().saturating_sub(1);
            self.tab_mut().table_selected_index = self.tab().table_selected_index.min(max_idx);
        }
    }

    // ---- Tree Sidebar Expansion ----

    fn selected_tree_path(&self) -> Option<PathBuf> {
        self.sidebar_rendered_paths
            .get(self.sidebar_selected_index)?
            .clone()
    }

    /// Expands or collapses the selected tree node with lazy child loading.
    pub fn toggle_tree_node(&mut self, expand: bool) {
        let Some(sel_path) = self.selected_tree_path() else {
            return;
        };
        let Some(idx) = self.tree_nodes.iter().position(|n| n.path == sel_path) else {
            return;
        };

        if expand {
            let node = &mut self.tree_nodes[idx];
            if node.has_children && !node.is_expanded {
                node.is_expanded = true;
                if !node.children_loaded {
                    self.scanner.scan_tree_children(node.path.clone());
                }
            }
        } else {
            let depth = self.tree_nodes[idx].depth;
            self.tree_nodes[idx].is_expanded = false;
            // Drop all descendants from the flat render list
            let mut end = idx + 1;
            while end < self.tree_nodes.len() && self.tree_nodes[end].depth > depth {
                end += 1;
            }
            self.tree_nodes.drain(idx + 1..end);
        }
    }

    pub fn on_tree_children_loaded(&mut self, path: PathBuf, children: Vec<PathBuf>) {
        let Some(pos) = self.tree_nodes.iter().position(|n| n.path == path) else {
            return;
        };
        let depth = self.tree_nodes[pos].depth;
        self.tree_nodes[pos].children_loaded = true;
        self.tree_nodes[pos].has_children = !children.is_empty();

        if !self.tree_nodes[pos].is_expanded {
            return; // collapsed again before results arrived
        }

        let nodes: Vec<TreeNode> = children
            .into_iter()
            .map(|p| TreeNode {
                name: p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                path: p,
                depth: depth + 1,
                is_expanded: false,
                has_children: true,
                children_loaded: false,
            })
            .collect();
        let insert_at = pos + 1;
        self.tree_nodes.splice(insert_at..insert_at, nodes);
    }

    // ---- Breadcrumb Sibling Popover ----

    pub fn open_breadcrumb_siblings(&mut self) {
        let Some(seg) = self.breadcrumb_segments.get(self.breadcrumb_selected) else {
            return;
        };
        let Some(parent) = seg.path.parent() else {
            self.set_status_info("No sibling directories".to_string());
            return;
        };

        let mut items: Vec<PathBuf> = std::fs::read_dir(parent)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                    && (self.show_hidden || !e.file_name().to_string_lossy().starts_with('.'))
            })
            .map(|e| e.path())
            .collect();
        items.sort_by(|a, b| {
            natord::compare(
                &a.file_name().unwrap_or_default().to_string_lossy(),
                &b.file_name().unwrap_or_default().to_string_lossy(),
            )
        });

        if items.is_empty() {
            self.set_status_info("No sibling directories".to_string());
            return;
        }

        let selected = items.iter().position(|p| p == &seg.path).unwrap_or(0);
        self.breadcrumb_popover = Some(BreadcrumbPopover {
            scroll_offset: selected.saturating_sub(11), // keep the selected sibling in view
            max_visible: 12,
            items,
            selected,
            screen_rect: Rect::default(),
        });
    }

    // ---- Context Menu ----

    /// Builds the entry list for the current selection state. Paste is always
    /// available; everything else requires a target under the cursor.
    fn build_context_items(
        has_target: bool,
        user_actions: &[UserAction],
        target_path: Option<&Path>,
    ) -> Vec<ContextMenuItem> {
        let mut items = Vec::new();
        if has_target {
            items.push(ContextMenuItem::enabled(" 󰈅 Open", ContextAction::Open));
            items.push(ContextMenuItem::separator());
        }
        if has_target {
            items.push(ContextMenuItem::enabled(" 󰆏 Copy", ContextAction::Copy));
            items.push(ContextMenuItem::enabled(" 󰆎 Cut", ContextAction::Cut));
        }
        items.push(ContextMenuItem::enabled(" 󰁉 Paste", ContextAction::Paste));
        // Directory-scoped creation actions work with or without a target
        items.push(ContextMenuItem::separator());
        items.push(ContextMenuItem::enabled(
            " 󰉋 New Folder",
            ContextAction::NewFolder,
        ));
        items.push(ContextMenuItem::enabled(
            " 󰈔 New File",
            ContextAction::NewFile,
        ));
        if has_target {
            items.push(ContextMenuItem::separator());
            items.push(ContextMenuItem::enabled(
                " 󰈔 Rename…",
                ContextAction::Rename,
            ));
            items.push(ContextMenuItem::enabled(
                " 󰆴 Move to Trash",
                ContextAction::MoveToTrash,
            ));
            items.push(ContextMenuItem::enabled(
                " 󰀬 Delete Permanently…",
                ContextAction::DeletePermanently,
            ));
            // Archive extraction
            if target_path
                .map(crate::fs::archive::is_archive)
                .unwrap_or(false)
            {
                items.push(ContextMenuItem::enabled(
                    " 󰁹 Extract Here",
                    ContextAction::ExtractHere,
                ));
            }
            items.push(ContextMenuItem::separator());
            items.push(ContextMenuItem::enabled(
                " 󰋽 Properties",
                ContextAction::Properties,
            ));
        }
        if !user_actions.is_empty() {
            items.push(ContextMenuItem::separator());
            for (idx, action) in user_actions.iter().enumerate() {
                items.push(ContextMenuItem::enabled(
                    action.name.clone(),
                    ContextAction::UserAction(idx),
                ));
            }
        }
        items
    }

    /// Spawns a user action detached from the TUI. Never goes through a
    /// shell; argv[0] is executed directly.
    pub fn run_user_action(&mut self, idx: usize) {
        self.run_user_action_for(idx, None);
    }

    /// User-action launch with an optional explicit target path (context
    /// menu opened on a sidebar / breadcrumb entry) substituted for the
    /// `{file}`/`{selected}` placeholders.
    pub fn run_user_action_for(&mut self, idx: usize, override_target: Option<PathBuf>) {
        let Some(action) = self.user_actions.get(idx).cloned() else {
            return;
        };
        let ctx = crate::config::ActionContext::new(
            override_target
                .as_ref()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                .unwrap_or_else(|| self.tab().current_dir.clone()),
            override_target.or_else(|| self.selected_entry().map(|e| e.path.clone())),
            {
                let mut sel: Vec<PathBuf> = self.tab().multi_selected.iter().cloned().collect();
                sel.sort();
                sel
            },
        );
        let argv = action.expand_args(&ctx);

        match std::process::Command::new(&action.command)
            .args(&argv)
            .spawn()
        {
            Ok(_) => self.set_status_info(format!("Launched: {}", action.name)),
            Err(e) => self.set_status_error(format!("Failed to launch {}: {}", action.command, e)),
        }
    }

    /// `` ` `` / F4 — opens a new terminal window at the current directory.
    pub fn open_terminal(&mut self) {
        let Some(term) = Self::terminal_program() else {
            self.set_status_error(
                "No terminal emulator found ($TERMINAL, alacritty, kitty, foot…)".to_string(),
            );
            return;
        };
        let dir = self.tab().current_dir.clone();
        match std::process::Command::new(term.0)
            .args(term.1)
            .current_dir(&dir)
            .spawn()
        {
            Ok(_) => self.set_status_info(format!("Terminal opened at {}", dir.display())),
            Err(e) => self.set_status_error(format!("Failed to open terminal: {}", e)),
        }
    }

    fn terminal_program() -> Option<(String, Vec<String>)> {
        // $TERMINAL wins when set; then known emulators in launch preference
        if let Ok(t) = std::env::var("TERMINAL") {
            if !t.trim().is_empty() {
                let mut parts = t.split_whitespace();
                let prog = parts.next()?.to_string();
                return Some((prog, parts.map(String::from).collect()));
            }
        }
        for candidate in ["alacritty", "kitty", "foot", "wezterm", "ghostty"] {
            if which_exists(candidate) {
                return Some((candidate.to_string(), Vec::new()));
            }
        }
        None
    }
    /// `e` — opens the selected file in $EDITOR/$VISUAL inside a new
    /// terminal window (GUI editors are launched directly).
    pub fn open_in_editor(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_default();
        if editor.trim().is_empty() {
            self.set_status_error("No editor configured (set $EDITOR or $VISUAL)".to_string());
            return;
        }

        self.launch_in_terminal_or_direct(&editor, &entry.path, "editor", "Editing");
    }

    /// `p` — opens the selected file read-only in $PAGER (falls back to the
    /// editor when no pager is configured).
    pub fn open_in_pager(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        let pager = std::env::var("PAGER").unwrap_or_default();
        if pager.trim().is_empty() {
            self.set_status_error("No pager configured (set $PAGER, e.g. less)".to_string());
            return;
        }

        self.launch_in_terminal_or_direct(&pager, &entry.path, "pager", "Viewing");
    }

    /// Splits a launcher spec into program + flags and spawns it: console
    /// programs are hosted in a new terminal window, GUI programs directly.
    fn launch_in_terminal_or_direct(&mut self, spec: &str, path: &PathBuf, kind: &str, verb: &str) {
        let mut parts = spec.split_whitespace();
        let prog = parts.next().unwrap_or_default().to_string();
        let flags: Vec<String> = parts.map(String::from).collect();

        // Console programs need a terminal host; GUI programs don't
        const CONSOLE_PROGS: &[&str] = &[
            "nvim", "vim", "vi", "nano", "emacs", "helix", "hx", "micro", "less", "more", "most",
            "bat", "cat",
        ];
        if CONSOLE_PROGS.contains(&prog.as_str()) {
            let Some((term, term_args)) = Self::terminal_program() else {
                self.set_status_error(format!("No terminal emulator found to host the {}", kind));
                return;
            };
            let mut cmd = std::process::Command::new(term);
            cmd.args(term_args);
            cmd.args(["-e", &prog]);
            cmd.args(flags);
            cmd.arg(path);
            match cmd.spawn() {
                Ok(_) => self.set_status_info(format!(
                    "{} {:?} in {}",
                    verb,
                    path.file_name().unwrap_or_default(),
                    prog
                )),
                Err(e) => self.set_status_error(format!("Failed to open {}: {}", kind, e)),
            }
        } else {
            match std::process::Command::new(prog.clone())
                .args(flags)
                .arg(path)
                .spawn()
            {
                Ok(_) => self.set_status_info(format!(
                    "{} {:?} in {}",
                    verb,
                    path.file_name().unwrap_or_default(),
                    prog
                )),
                Err(e) => self.set_status_error(format!("Failed to open {}: {}", prog, e)),
            }
        }
    }

    /// Opens the context menu anchored at a screen position.
    pub fn open_context_menu_at(&mut self, x: u16, y: u16) {
        if self.context_menu.is_some() {
            return;
        }
        let has_target = self.selected_entry().is_some();
        let target_path = self.selected_entry().map(|e| e.path.clone());
        let user_actions = self.user_actions.clone();
        self.context_menu = Some(ContextMenu {
            anchor_x: x,
            anchor_y: y,
            selected: 0,
            scroll_offset: 0,
            max_visible: 14 + user_actions.len(),
            screen_rect: Rect::default(),
            items: Self::build_context_items(has_target, &user_actions, target_path.as_deref()),
            target: None,
        });
    }

    /// Opens the menu for an explicit filesystem path (sidebar tree node or
    /// breadcrumb chip). All targeted actions apply to `path`; Paste goes
    /// into the folder when the target is a directory.
    pub fn open_context_menu_for_path(&mut self, path: PathBuf, x: u16, y: u16) {
        if self.context_menu.is_some() {
            return;
        }
        let user_actions = self.user_actions.clone();
        let mut items = Self::build_context_items(true, &user_actions, Some(&path));
        if path.is_dir() {
            for item in &mut items {
                if item.action == Some(ContextAction::Paste) {
                    item.label = " 󰁉 Paste Into Folder".to_string();
                }
            }
        }
        self.context_menu = Some(ContextMenu {
            anchor_x: x,
            anchor_y: y,
            selected: 0,
            scroll_offset: 0,
            max_visible: 14 + user_actions.len(),
            screen_rect: Rect::default(),
            items,
            target: Some(path),
        });
    }

    /// Keyboard trigger (`m` / Menu / Shift+F10): anchors near the cursor row.
    pub fn open_context_menu(&mut self) {
        let rel = self
            .tab()
            .table_selected_index
            .saturating_sub(self.tab().table_scroll_offset);
        let x = self.table_rect.x + self.table_rect.width / 4;
        let y = self.table_rect.y + 2 + rel as u16;
        self.open_context_menu_at(x, y);
    }

    pub fn close_context_menu(&mut self) {
        self.context_menu = None;
    }

    fn context_menu_move_selection(&mut self, delta: i32) {
        let Some(menu) = &mut self.context_menu else {
            return;
        };
        let len = menu.items.len();
        if len == 0 {
            return;
        }
        let mut idx = menu.selected as i32;
        for _ in 0..len {
            idx = (idx + delta).clamp(0, len as i32 - 1);
            if !menu.items[idx as usize].is_separator() || idx == 0 || idx == len as i32 - 1 {
                break;
            }
        }
        menu.selected = idx as usize;

        // Keep the selected item inside the visible window
        if menu.selected < menu.scroll_offset {
            menu.scroll_offset = menu.selected;
        } else if menu.selected >= menu.scroll_offset + menu.max_visible {
            menu.scroll_offset = menu.selected + 1 - menu.max_visible;
        }
    }

    /// Runs the given context action and closes the menu. When the menu was
    /// opened for an explicit path (sidebar / breadcrumb), targeted actions
    /// apply to that path instead of the table selection.
    pub fn execute_context_action(&mut self, action: ContextAction) {
        let target = self
            .context_menu
            .as_ref()
            .and_then(|menu| menu.target.clone());
        self.close_context_menu();
        match action {
            ContextAction::Open => match target {
                Some(path) => self.open_path(path),
                None => self.open_selected(),
            },
            ContextAction::Copy => match target {
                Some(path) => {
                    self.clipboard = Some(Clipboard {
                        paths: vec![path],
                        cut: false,
                    });
                    self.set_status_info("Copied 1 item(s) to clipboard".to_string());
                }
                None => self.copy_selected(),
            },
            ContextAction::Cut => match target {
                Some(path) => {
                    self.clipboard = Some(Clipboard {
                        paths: vec![path],
                        cut: true,
                    });
                    self.set_status_info("Cut 1 item(s) to clipboard".to_string());
                }
                None => self.cut_selected(),
            },
            // A directory target receives the paste; otherwise paste into
            // the current directory as usual
            ContextAction::Paste => {
                if let Some(dir) = target.filter(|p| p.is_dir()) {
                    if let Some(clipboard) = self.clipboard.take() {
                        self.prepare_transfer(clipboard.paths, dir, clipboard.cut);
                    } else {
                        self.set_status_info("Clipboard is empty".to_string());
                    }
                } else {
                    self.paste_clipboard();
                }
            }
            ContextAction::NewFolder => {
                let parent = target
                    .filter(|p| p.is_dir())
                    .unwrap_or_else(|| self.tab().current_dir.clone());
                self.request_new_folder_in(parent);
            }
            ContextAction::NewFile => {
                let parent = target
                    .filter(|p| p.is_dir())
                    .unwrap_or_else(|| self.tab().current_dir.clone());
                self.request_new_file_in(parent);
            }
            ContextAction::Rename => match target {
                Some(path) => self.request_rename_path(path),
                None => self.request_rename(),
            },
            ContextAction::MoveToTrash => match target {
                Some(path) => self.request_trash_paths(vec![path]),
                None => self.request_trash(),
            },
            ContextAction::DeletePermanently => match target {
                Some(path) => self.request_permanent_delete_paths(vec![path]),
                None => self.request_permanent_delete(),
            },
            ContextAction::Properties => {
                // Bring the target under the table cursor when possible so
                // the metadata panel inspects it
                if let Some(path) = &target {
                    let idx = self.tab().entries.iter().position(|e| &e.path == path);
                    if let Some(idx) = idx {
                        self.tab_mut().table_selected_index = idx;
                        self.tab_mut().multi_selected.clear();
                    }
                }
                if !self.show_preview {
                    self.show_preview = true;
                }
                self.focus = Focus::Preview;
            }
            ContextAction::ExtractHere => {
                if let Some(path) = target {
                    let dest = self.tab().current_dir.clone();
                    let event_tx = self.event_tx.clone();
                    let archive_name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "archive".to_string());
                    let name_for_status = archive_name.clone();
                    tokio::spawn(async move {
                        let result =
                            tokio::task::spawn_blocking(move || {
                                crate::fs::archive::extract_archive(&path, &dest)
                            })
                            .await;
                        match result {
                            Ok(Ok(())) => {
                                let _ = event_tx.send(AppEvent::StatusMessage {
                                    text: format!("Extracted: {}", archive_name),
                                    is_error: false,
                                });
                            }
                            Ok(Err(e)) => {
                                let _ = event_tx.send(AppEvent::StatusMessage {
                                    text: format!("Extraction failed: {}", e),
                                    is_error: true,
                                });
                            }
                            Err(e) => {
                                let _ = event_tx.send(AppEvent::StatusMessage {
                                    text: format!("Extraction task failed: {}", e),
                                    is_error: true,
                                });
                            }
                        }
                    });
                    self.set_status_info(format!("Extracting {}…", name_for_status));
                }
            }
            ContextAction::UserAction(idx) => self.run_user_action_for(idx, target),
        }
    }

    /// Opens a path: navigate for directories, desktop-open for files.
    pub fn open_path(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.navigate_to(path);
        } else if let Err(e) = open::that(&path) {
            self.set_status_error(format!("Failed to open file: {}", e));
        } else {
            self.set_status_info(format!("Opened {:?}", path.file_name().unwrap_or_default()));
        }
    }

    /// Executes the currently highlighted context menu entry.
    fn activate_context_menu(&mut self) {
        let Some(action) = self
            .context_menu
            .as_ref()
            .and_then(|menu| menu.items.get(menu.selected))
            .and_then(|item| item.action)
        else {
            self.close_context_menu();
            return;
        };
        self.execute_context_action(action);
    }

    /// Context-menu key handling; swallows all other input while open.
    fn handle_context_menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Left | KeyCode::Right => {
                self.close_context_menu();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.context_menu_move_selection(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.context_menu_move_selection(-1);
            }
            KeyCode::Home | KeyCode::Char('g') => {
                if let Some(menu) = &mut self.context_menu {
                    menu.selected = 0;
                    menu.scroll_offset = 0;
                }
            }
            KeyCode::End => {
                if let Some(menu) = &mut self.context_menu {
                    menu.selected = menu.items.len().saturating_sub(1);
                    self.context_menu_move_selection(0); // snap into view
                }
            }
            KeyCode::Enter => {
                self.activate_context_menu();
            }
            _ => {}
        }
    }

    // ---- Column Resizing ----

    pub fn resize_name_column(&mut self, delta: i16) {
        let inner_width = self.table_rect.width.saturating_sub(2);
        // Other columns consume ~42 cells (2 checkbox + 2 icon + size + modified + perms)
        let min_w = 10u16;
        let max_w = inner_width.saturating_sub(42).max(min_w);
        let current = self.name_column_effective_width as i32 + delta as i32;
        let clamped = current.clamp(min_w as i32, max_w as i32) as u16;
        self.name_column_width_override = Some(clamped);
    }

    pub fn open_selected(&mut self) {
        if let Some(entry) = self.selected_entry().cloned() {
            if entry.is_dir {
                self.navigate_to(entry.path);
            } else {
                // Open file with default desktop opener
                if let Err(e) = open::that(&entry.path) {
                    self.set_status_error(format!("Failed to open file: {}", e));
                } else {
                    self.set_status_info(format!("Opened {:?}", entry.name));
                }
            }
        }
    }

    pub fn toggle_selection(&mut self) {
        if let Some(entry) = self.selected_entry() {
            let path = entry.path.clone();
            if self.tab().multi_selected.contains(&path) {
                self.tab_mut().multi_selected.remove(&path);
            } else {
                self.tab_mut().multi_selected.insert(path);
            }
            // Range selections extend from the last explicit toggle
            self.tab_mut().selection_anchor = Some(self.tab().table_selected_index);
        }
    }

    /// `*` — selects everything not selected, deselects everything that is.
    pub fn invert_selection(&mut self) {
        for idx in 0..self.visible_len() {
            if let Some(entry) = self.tab().visible_entry_at(idx) {
                let path = entry.path.clone();
                if !self.tab_mut().multi_selected.remove(&path) {
                    self.tab_mut().multi_selected.insert(path);
                }
            }
        }
    }

    /// Shift+movement: select the whole span from the anchor to the new
    /// cursor position and move the cursor there. The first press anchors at
    /// the current cursor.
    pub fn extend_selection(&mut self, new_idx: usize) {
        let max_idx = self.visible_len().saturating_sub(1);
        let new_idx = new_idx.min(max_idx);
        let cursor = self.tab().table_selected_index;
        let anchor = *self.tab_mut().selection_anchor.get_or_insert(cursor);
        let (start, end) = if anchor <= new_idx {
            (anchor, new_idx)
        } else {
            (new_idx, anchor)
        };

        for idx in start..=end {
            let path = self.tab().visible_entry_at(idx).map(|e| e.path.clone());
            if let Some(path) = path {
                self.tab_mut().multi_selected.insert(path);
            }
        }
        self.tab_mut().table_selected_index = new_idx;
    }

    pub fn clear_selection(&mut self) {
        self.tab_mut().multi_selected.clear();
        self.tab_mut().selection_anchor = None;
    }

    pub fn select_all(&mut self) {
        if self.tab().search_mode {
            let all_selected = self
                .tab()
                .search_matches
                .iter()
                .all(|e| self.tab().multi_selected.contains(&e.path));
            if all_selected && !self.tab().search_matches.is_empty() {
                let paths: Vec<PathBuf> = self
                    .tab()
                    .search_matches
                    .iter()
                    .map(|e| e.path.clone())
                    .collect();
                for p in paths {
                    self.tab_mut().multi_selected.remove(&p);
                }
            } else {
                let paths: Vec<PathBuf> = self
                    .tab()
                    .search_matches
                    .iter()
                    .map(|e| e.path.clone())
                    .collect();
                for p in paths {
                    self.tab_mut().multi_selected.insert(p);
                }
            }
            return;
        }

        if self.tab().multi_selected.len() == self.tab().filtered_indices.len() {
            self.tab_mut().multi_selected.clear();
        } else {
            let idxs = self.tab().filtered_indices.clone();
            for &idx in &idxs {
                if let Some(path) = self.tab().entries.get(idx).map(|e| e.path.clone()) {
                    self.tab_mut().multi_selected.insert(path);
                }
            }
        }
    }

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.reapply_filter();
    }

    pub fn cycle_sort_column(&mut self) {
        self.sort_column = match self.sort_column {
            SortColumn::Name => SortColumn::Size,
            SortColumn::Size => SortColumn::Modified,
            SortColumn::Modified => SortColumn::Permissions,
            SortColumn::Permissions => SortColumn::Extension,
            SortColumn::Extension => SortColumn::Name,
        };
        self.resort_entries();
    }

    pub fn reverse_sort(&mut self) {
        self.sort_direction = self.sort_direction.toggle();
        self.resort_entries();
    }

    pub fn set_sort(&mut self, col: SortColumn) {
        if self.sort_column == col {
            self.sort_direction = self.sort_direction.toggle();
        } else {
            self.sort_column = col;
            self.sort_direction = SortDirection::Ascending;
        }
        self.resort_entries();
    }

    pub fn resort_entries(&mut self) {
        let (col, dir, dirs_first, natural) = (
            self.sort_column,
            self.sort_direction,
            self.dirs_first,
            self.natural_sort,
        );
        let tab = self.tab_mut();
        sort_entries(&mut tab.entries, col, dir, dirs_first, natural);
        self.reapply_filter();
    }

    pub fn reapply_filter(&mut self) {
        let query = self.tab().search_query.to_lowercase();
        let show_hidden = self.show_hidden;
        self.tab_mut().filtered_indices = self
            .tab()
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if !show_hidden && e.is_hidden {
                    return false;
                }
                if query.is_empty() {
                    return true;
                }
                e.name.to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect();

        if self.tab().table_selected_index >= self.tab().filtered_indices.len() {
            {
                let len = self.tab().filtered_indices.len();
                self.tab_mut().table_selected_index = len.saturating_sub(1);
            }
        }
    }

    /// Paths a file operation applies to: explicit multi-selection, or the
    /// cursor's entry. Sorted for deterministic batch ordering.
    fn operation_targets(&self) -> Vec<PathBuf> {
        if !self.tab().multi_selected.is_empty() {
            let mut paths: Vec<PathBuf> = self.tab().multi_selected.iter().cloned().collect();
            paths.sort();
            return paths;
        }
        self.selected_entry()
            .map(|e| vec![e.path.clone()])
            .unwrap_or_default()
    }

    pub fn trash_selected(&mut self) {
        let targets = self.operation_targets();
        if targets.is_empty() {
            return;
        }

        let mut success: Vec<PathBuf> = Vec::new();
        let mut err_msg = None;

        for p in &targets {
            if !p.exists() {
                continue; // stale selection from a previous directory state
            }
            match move_to_trash(p) {
                Ok(_) => success.push(p.clone()),
                Err(e) => {
                    err_msg = Some(e);
                    break;
                }
            }
        }

        for p in &success {
            self.tab_mut().multi_selected.remove(p);
        }

        if let Some(err) = err_msg {
            // Trash infrastructure unavailable (headless / SSH / no XDG dirs):
            // fall back to an explicit destructive-action confirmation.
            let remaining: Vec<PathBuf> = targets
                .iter()
                .filter(|p| !success.contains(p))
                .cloned()
                .collect();
            if remaining.is_empty() {
                self.set_status_info(format!("Moved {} item(s) to Trash", success.len()));
                self.refresh();
                return;
            }
            let noun = if remaining.len() == 1 {
                "item"
            } else {
                "items"
            };
            self.dialog = Some(Dialog::confirm(
                " 󰀬 Trash Unavailable ",
                vec![
                    err,
                    String::new(),
                    format!(
                        "{} {} could not be moved to the system Trash.",
                        remaining.len(),
                        noun
                    ),
                    String::new(),
                    "Permanently delete instead? This cannot be undone.".to_string(),
                ],
                "Delete Permanently",
                true,
                DialogAction::DeletePermanently(remaining),
            ));
        } else if !success.is_empty() {
            self.set_status_info(format!("Moved {} item(s) to Trash", success.len()));
            self.refresh();
        }
    }

    /// `d` — asks before trashing (plain `Delete` skips the prompt).
    pub fn request_trash(&mut self) {
        let targets = self.operation_targets();
        if targets.is_empty() {
            return;
        }
        self.request_trash_paths(targets);
    }

    /// Trash confirmation for explicit paths (context menu target).
    pub fn request_trash_paths(&mut self, targets: Vec<PathBuf>) {
        let names = summarize_paths(&targets);
        self.dialog = Some(Dialog::confirm(
            " 󰆴 Move to Trash ",
            vec![
                format!("Move {} to the system Trash?", names),
                String::new(),
                "Items can be restored from the Trash later.".to_string(),
            ],
            "Move to Trash",
            false,
            DialogAction::Trash(targets),
        ));
    }

    /// Shift+Delete — permanent deletion always requires confirmation.
    pub fn request_permanent_delete(&mut self) {
        let targets = self.operation_targets();
        if targets.is_empty() {
            return;
        }
        self.request_permanent_delete_paths(targets);
    }

    /// Permanent-delete confirmation for explicit paths (context menu target).
    pub fn request_permanent_delete_paths(&mut self, targets: Vec<PathBuf>) {
        let names = summarize_paths(&targets);
        self.dialog = Some(Dialog::confirm(
            " 󰀬 Permanently Delete ",
            vec![
                format!("Permanently delete {}?", names),
                String::new(),
                "This bypasses the Trash and cannot be undone.".to_string(),
            ],
            "Delete Forever",
            true,
            DialogAction::DeletePermanently(targets),
        ));
    }

    /// Executes the active dialog's Confirm button (action + validation).
    pub fn confirm_dialog(&mut self) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };

        if dialog.prompt.is_some() {
            if let Err(e) = dialog.validate_input() {
                self.set_status_error(e);
                self.dialog = Some(dialog); // keep open for correction
                return;
            }
        }

        match dialog.action {
            DialogAction::Trash(paths) => self.perform_trash(&paths),
            DialogAction::DeletePermanently(paths) => self.perform_permanent_delete(&paths),
            DialogAction::Rename(source) => {
                let name = dialog
                    .prompt
                    .map(|p| p.buffer.trim().to_string())
                    .unwrap_or_default();
                match rename_entry(&source, &name) {
                    Ok(_) => {
                        self.tab_mut().multi_selected.remove(&source);
                        self.set_status_info(format!("Renamed to {:?}", name));
                        self.refresh();
                    }
                    Err(e) => self.set_status_error(e),
                }
            }
            DialogAction::CreateFolder(parent) => {
                let name = dialog
                    .prompt
                    .map(|p| p.buffer.trim().to_string())
                    .unwrap_or_default();
                match create_directory(&parent, &name) {
                    Ok(_) => {
                        self.set_status_info(format!("Created folder {:?}", name));
                        self.refresh();
                    }
                    Err(e) => self.set_status_error(e),
                }
            }
            DialogAction::CreateFile(parent) => {
                let name = dialog
                    .prompt
                    .map(|p| p.buffer.trim().to_string())
                    .unwrap_or_default();
                match create_file(&parent, &name) {
                    Ok(_) => {
                        self.set_status_info(format!("Created file {:?}", name));
                        self.refresh();
                    }
                    Err(e) => self.set_status_error(e),
                }
            }
            DialogAction::RetryTransfer {
                sources,
                dest_dir,
                cut,
            } => self.prepare_transfer(sources, dest_dir, cut),
            DialogAction::None => {}
        }
    }

    /// Resolves the pending transfer conflict by submitting the job with the
    /// chosen collision mode.
    fn resolve_conflict(&mut self, resolution: ConflictResolution) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        let _ = dialog;
        let Some(pending) = self.pending_transfer.take() else {
            return;
        };
        self.submit_transfer(pending.sources, pending.dest_dir, pending.cut, resolution);
    }

    pub fn cancel_dialog(&mut self) {
        self.dialog = None;
        // A dismissed conflict dialog discards the queued transfer
        self.pending_transfer = None;
    }

    fn perform_trash(&mut self, paths: &[PathBuf]) {
        let total = paths.len();
        let id = self.ops_worker.trash(paths.to_vec());
        self.active_ops.push(ActiveOp {
            id,
            label: "Moving to Trash".to_string(),
            done: 0,
            total,
            current: String::new(),
            cancelling: false,
        });
    }

    fn perform_permanent_delete(&mut self, paths: &[PathBuf]) {
        // Selection is dropped up-front: the confirmed operation is committed
        for p in paths {
            self.tab_mut().multi_selected.remove(p);
        }
        let total = paths.len();
        let id = self.ops_worker.delete(paths.to_vec());
        self.active_ops.push(ActiveOp {
            id,
            label: "Deleting".to_string(),
            done: 0,
            total,
            current: String::new(),
            cancelling: false,
        });
    }

    // ---- Clipboard (Copy / Cut / Paste) ----

    fn clipboard_sources(&self) -> Vec<PathBuf> {
        if !self.tab().multi_selected.is_empty() {
            let mut paths: Vec<PathBuf> = self.tab().multi_selected.iter().cloned().collect();
            paths.sort();
            return paths;
        }
        self.selected_entry()
            .map(|e| vec![e.path.clone()])
            .unwrap_or_default()
    }

    pub fn copy_selected(&mut self) {
        let sources = self.clipboard_sources();
        if sources.is_empty() {
            return;
        }
        self.clipboard = Some(Clipboard {
            paths: sources,
            cut: false,
        });
        self.set_status_info(format!(
            "Copied {} item(s) to clipboard",
            self.clipboard.as_ref().unwrap().paths.len()
        ));
    }

    pub fn cut_selected(&mut self) {
        let sources = self.clipboard_sources();
        if sources.is_empty() {
            return;
        }
        self.clipboard = Some(Clipboard {
            paths: sources,
            cut: true,
        });
        self.set_status_info(format!(
            "Cut {} item(s) to clipboard",
            self.clipboard.as_ref().unwrap().paths.len()
        ));
    }

    pub fn paste_clipboard(&mut self) {
        let Some(clipboard) = self.clipboard.take() else {
            self.set_status_info("Clipboard is empty".to_string());
            return;
        };

        self.prepare_transfer(
            clipboard.paths,
            self.tab().current_dir.clone(),
            clipboard.cut,
        );
    }

    /// Conflict-aware transfer entry point: filters no-ops, detects name
    /// collisions at the destination, then either submits directly or opens
    /// a resolution dialog first.
    pub fn prepare_transfer(&mut self, sources: Vec<PathBuf>, dest_dir: PathBuf, cut: bool) {
        // Moving into the same directory is a no-op; drop those entries
        let sources: Vec<PathBuf> = sources
            .into_iter()
            .filter(|p| !cut || p.parent() != Some(dest_dir.as_path()))
            .collect();
        if sources.is_empty() {
            self.set_status_info("Nothing to transfer — items are already there".to_string());
            return;
        }

        // Detect collisions (destination names that already exist)
        let collisions: Vec<String> = sources
            .iter()
            .filter_map(|p| {
                let name = p.file_name()?.to_string_lossy().to_string();
                dest_dir.join(&name).exists().then_some(name)
            })
            .collect();

        if collisions.is_empty() {
            self.submit_transfer(sources, dest_dir, cut, ConflictResolution::AutoRename);
            return;
        }

        let shown: Vec<String> = collisions.iter().take(3).cloned().collect();
        let more = collisions.len().saturating_sub(3);
        let mut message = vec![format!(
            "{} item(s) already exist in {}:",
            collisions.len(),
            dest_dir.file_name().unwrap_or_default().to_string_lossy()
        )];
        message.extend(shown.iter().map(|n| format!("  {}", n)));
        if more > 0 {
            message.push(format!("…and {} more", more));
        }
        message.push(String::new());
        message.push("How should the conflicts be resolved?".to_string());

        self.pending_transfer = Some(PendingTransfer {
            sources,
            dest_dir,
            cut,
        });
        self.dialog = Some(Dialog::conflict(" 󰀨 File Conflicts ", message));
    }

    /// Submits the transfer job with the chosen collision mode.
    fn submit_transfer(
        &mut self,
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
        cut: bool,
        collision: ConflictResolution,
    ) {
        let verb = if cut { "Moving" } else { "Copying" };
        let total = sources.len();
        let mode = match collision {
            ConflictResolution::Overwrite => CollisionMode::Overwrite,
            ConflictResolution::Skip => CollisionMode::Skip,
            ConflictResolution::AutoRename => CollisionMode::AutoRename,
        };
        let id = if cut {
            self.ops_worker.move_with(sources, dest_dir, mode)
        } else {
            self.ops_worker.copy_with(sources, dest_dir, mode)
        };
        self.active_ops.push(ActiveOp {
            id,
            label: verb.to_string(),
            done: 0,
            total,
            current: String::new(),
            cancelling: false,
        });
    }

    // ---- Drag & Drop ----

    /// Resolves an active drag on mouse-up: finds the drop target under the
    /// cursor (sidebar folder or breadcrumb chip), filters no-ops (same
    /// parent, dropping a directory into itself), and submits the move/copy.
    fn finish_drag_drop(&mut self) {
        let Some(state) = self.drag_drop.take() else {
            return;
        };
        let (x, y) = state.hover;

        let Some(dest_dir) = self.resolve_drop_target(x, y).filter(|p| p.is_dir()) else {
            return; // released over nothing droppable — cancel silently
        };

        let sources: Vec<PathBuf> = state
            .paths
            .into_iter()
            .filter(|p| p.parent() != Some(dest_dir.as_path()))
            .filter(|p| !dest_dir.starts_with(p)) // no dropping a dir into itself
            .collect();
        if sources.is_empty() {
            return;
        }

        self.prepare_transfer(sources, dest_dir, !state.copy);
    }

    /// Maps a screen position to a potential drop target directory.
    fn resolve_drop_target(&self, x: u16, y: u16) -> Option<PathBuf> {
        // Breadcrumb chips (top bar row)
        if y == 1 {
            return self.breadcrumb_segments.iter().find_map(|seg| {
                (x >= seg.area.x && x < seg.area.x + seg.area.width).then(|| seg.path.clone())
            });
        }
        // Sidebar tree / bookmark rows
        if Self::point_in_rect(x, y, self.sidebar_rect) {
            let inner_y = self.sidebar_rect.y + 1;
            if y >= inner_y {
                let idx = self.sidebar_scroll_offset + (y - inner_y) as usize;
                return self.sidebar_rendered_paths.get(idx).cloned().flatten();
            }
        }
        None
    }

    /// F2 — rename prompt pre-filled with the cursor's entry name.
    pub fn request_rename(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        self.request_rename_path(entry.path);
    }

    /// Rename prompt for an explicit path (context menu target).
    pub fn request_rename_path(&mut self, path: PathBuf) {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        self.dialog = Some(Dialog::prompt(
            format!(" 󰈔 Rename \"{}\" ", name),
            "Rename",
            name,
            DialogAction::Rename(path),
        ));
    }

    /// Ctrl+N — new folder prompt in the current directory.
    pub fn request_new_folder(&mut self) {
        let parent = self.tab().current_dir.clone();
        self.request_new_folder_in(parent);
    }

    /// New-folder prompt inside an explicit parent (context menu target).
    pub fn request_new_folder_in(&mut self, parent: PathBuf) {
        self.dialog = Some(Dialog::prompt(
            " 󰉋 New Folder ",
            "Create Folder",
            String::new(),
            DialogAction::CreateFolder(parent),
        ));
    }

    /// Ctrl+Shift+N — new empty file prompt in the current directory.
    pub fn request_new_file(&mut self) {
        let parent = self.tab().current_dir.clone();
        self.request_new_file_in(parent);
    }

    /// New-file prompt inside an explicit parent (context menu target).
    pub fn request_new_file_in(&mut self, parent: PathBuf) {
        self.dialog = Some(Dialog::prompt(
            " 󰈔 New File ",
            "Create File",
            String::new(),
            DialogAction::CreateFile(parent),
        ));
    }

    // ---- Ops Worker Event Handlers ----

    pub fn on_ops_progress(&mut self, job_id: u64, done: usize, total: usize, current: String) {
        if let Some(op) = self.active_ops.iter_mut().find(|op| op.id == job_id) {
            op.done = done;
            op.total = total;
            op.current = current;
        }
    }

    pub fn on_ops_finished(&mut self, outcome: OpsOutcome) {
        let OpsOutcome {
            job_id,
            label,
            succeeded,
            skipped,
            errors,
            dest,
            cancelled,
        } = outcome;
        self.ops_worker.cleanup(job_id);
        let was_cancelling = self
            .active_ops
            .iter()
            .find(|op| op.id == job_id)
            .is_some_and(|op| op.cancelling);
        self.active_ops.retain(|op| op.id != job_id);
        // Keep the queue cursor in bounds after removals
        if self.job_queue_selected >= self.active_ops.len().max(1) {
            self.job_queue_selected = self.active_ops.len().saturating_sub(1);
        }

        // A cancelled job reports partial completion; never open retry/conflict
        // dialogs for it — the user asked for it to stop
        if cancelled || was_cancelling {
            self.set_status_info(format!("Cancelled {} — {} item(s) done", label, succeeded));
            return;
        }

        if !errors.is_empty() {
            let (first_path, first_err) = &errors[0];
            let file = first_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mut message = vec![
                format!("{} {} item(s); {} failed:", label, succeeded, errors.len()),
                format!("  {}: {}", file, first_err),
            ];
            if errors.len() > 1 {
                message.push(format!("…and {} more failures", errors.len() - 1));
            }
            // Resilient batch handling: offer to retry just the failed subset
            if self.dialog.is_none() {
                let sources: Vec<PathBuf> = errors.iter().map(|(p, _)| p.clone()).collect();
                self.dialog = Some(Dialog::retry(
                    message,
                    sources,
                    dest.unwrap_or_else(|| self.tab().current_dir.clone()),
                    false,
                ));
            } else {
                self.set_status_error(format!(
                    "{} {} item(s), {} failed (dialog open — see status log)",
                    label,
                    succeeded,
                    errors.len()
                ));
            }
        } else if skipped > 0 {
            self.set_status_info(format!(
                "{} {} item(s), {} skipped",
                label, succeeded, skipped
            ));
        } else {
            self.set_status_info(format!("{} {} item(s)", label, succeeded));
        }

        self.refresh();
    }

    // ---- Background Job Queue Overlay ----

    /// `Ctrl+J` — toggles the background job queue overlay.
    pub fn toggle_job_queue(&mut self) {
        self.show_job_queue = !self.show_job_queue;
        if self.show_job_queue {
            self.job_queue_selected = self
                .job_queue_selected
                .min(self.active_ops.len().saturating_sub(1));
        }
    }

    pub fn close_job_queue(&mut self) {
        self.show_job_queue = false;
    }

    fn job_queue_move_selection(&mut self, delta: i32) {
        if self.active_ops.is_empty() {
            return;
        }
        let len = self.active_ops.len() as i32;
        let current = self.job_queue_selected as i32;
        self.job_queue_selected = (current + delta).rem_euclid(len) as usize;
    }

    /// Requests cancellation of the job under the queue cursor. The worker
    /// stops at the next item boundary; the UI marks it "cancelling…" until
    /// the completion event arrives.
    pub fn cancel_selected_job(&mut self) {
        let Some(op) = self.active_ops.get(self.job_queue_selected) else {
            return;
        };
        let (id, label) = (op.id, op.label.clone());
        if self.ops_worker.cancel(id) {
            if let Some(op) = self.active_ops.iter_mut().find(|op| op.id == id) {
                op.cancelling = true;
            }
            self.set_status_info(format!("Cancelling: {}", label));
        } else {
            self.set_status_error(format!("Job already finished: {}", label));
        }
    }

    /// Key handling while the job queue overlay is open: arrows/j/k move the
    /// cursor, `x`/Enter cancel the selected job, Esc/q/? close.
    fn handle_job_queue_key(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
                self.job_queue_move_selection(-1);
            }
            (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
                self.job_queue_move_selection(1);
            }
            (KeyModifiers::NONE, KeyCode::Char('x')) | (KeyModifiers::NONE, KeyCode::Enter) => {
                self.cancel_selected_job();
            }
            (KeyModifiers::NONE, KeyCode::Esc)
            | (KeyModifiers::NONE, KeyCode::Char('q'))
            | (KeyModifiers::NONE, KeyCode::Char('?')) => self.close_job_queue(),
            _ => {}
        }
    }

    /// Requests an async preview load when the selected file changes.
    /// Called every loop iteration; cheap path comparison when up-to-date.
    pub fn maybe_request_preview(&mut self) {
        if !self.show_preview {
            if self.preview_loaded_path.is_some() || self.preview_pending_path.is_some() {
                self.clear_preview();
            }
            return;
        }

        let want = self
            .selected_entry()
            .filter(|e| !e.is_dir)
            .map(|e| e.path.clone());

        if self.preview_loaded_path == want && self.preview_pending_path.is_none() {
            return; // up to date
        }
        if self.preview_pending_path == want {
            return; // load in flight for exactly this file
        }

        self.clear_preview();
        if let Some(p) = want {
            self.preview_pending_path = Some(p.clone());
            self.scanner.load_preview(p, PREVIEW_MAX_BYTES);
        }
    }

    pub fn on_preview_loaded(
        &mut self,
        path: PathBuf,
        text: Option<String>,
        styled: Option<Vec<ratatui::text::Line<'static>>>,
        image: Option<image::DynamicImage>,
        hex_dump: Option<String>,
        meta: Option<crate::event::PreviewMeta>,
    ) {
        // Ignore late results for files that are no longer pending
        if self.preview_pending_path.as_deref() == Some(path.as_path()) {
            self.preview_pending_path = None;
            self.preview_loaded_path = Some(path);
            self.preview_text = Some(text);
            self.preview_styled = styled;
            self.preview_hex = hex_dump;
            if let Some(meta) = meta {
                self.preview_mime = meta.mime;
                self.preview_sha256 = meta.sha256;
            } else {
                self.preview_mime = None;
                self.preview_sha256 = None;
            }
            self.preview_image_dims = image.as_ref().map(|img| (img.width(), img.height()));
            self.preview_image_protocol =
                image.map(|img| self.image_picker.new_resize_protocol(img));
        }
    }

    /// Aspect-fit rectangle for the preview image inside `avail`, centered
    /// horizontally with a one-cell margin, using the picker's cell pixel
    /// size for the conversion.
    pub fn centered_image_rect(&self, avail: Rect) -> Rect {
        // Margin keeps the graphics render clear of the panel borders and text
        const MARGIN: u16 = 1;
        let avail = Rect {
            x: avail.x + MARGIN,
            y: avail.y + MARGIN,
            width: avail.width.saturating_sub(MARGIN * 2),
            height: avail.height.saturating_sub(MARGIN * 2),
        };

        let Some((img_w, img_h)) = self.preview_image_dims else {
            return avail;
        };
        let (cell_w, cell_h) = {
            let (w, h) = self.image_picker.font_size();
            (w.max(1) as f64, h.max(1) as f64)
        };
        if img_w == 0 || img_h == 0 || avail.width == 0 || avail.height == 0 {
            return avail;
        }

        // Cells covered by the image at native aspect: N columns cover
        // N*cell_w pixels, so rows = cols * img_h*cell_h / (img_w*cell_w)
        let aspect = (img_h as f64 * cell_h) / (img_w as f64 * cell_w);
        let cols = (avail.width as f64)
            .min(avail.height as f64 / aspect)
            .floor()
            .max(1.0);
        let rows = (cols * aspect).ceil().min(avail.height as f64).max(1.0);

        let x = avail.x + ((avail.width as f64 - cols) / 2.0) as u16;
        Rect {
            x,
            y: avail.y,
            width: cols as u16,
            height: rows as u16,
        }
    }

    /// Replaces the graphics-protocol picker (startup stdio probe) and drops
    /// any cached render state built with the previous one.
    pub fn set_image_picker(&mut self, picker: Picker) {
        self.image_picker = picker;
        self.preview_image_protocol = None;
    }

    /// Pops the latest async encoding result from the image protocol after a
    /// frame draw; failures are logged, never fatal.
    pub fn drain_image_encoding(&mut self) {
        if let Some(protocol) = &mut self.preview_image_protocol {
            if let Some(Err(e)) = protocol.last_encoding_result() {
                tracing::warn!("image preview encoding failed: {}", e);
            }
        }
    }

    fn clear_preview(&mut self) {
        self.preview_loaded_path = None;
        self.preview_pending_path = None;
        self.preview_text = None;
        self.preview_styled = None;
        self.preview_image_protocol = None;
        self.preview_image_dims = None;
        self.preview_hex = None;
        self.preview_mime = None;
        self.preview_sha256 = None;
    }

    pub fn set_status_info(&mut self, text: String) {
        self.status_message = Some(StatusMessageState {
            text,
            is_error: false,
            created_at: Instant::now(),
            duration: Duration::from_secs(4),
        });
    }

    pub fn set_status_error(&mut self, text: String) {
        self.status_message = Some(StatusMessageState {
            text,
            is_error: true,
            created_at: Instant::now(),
            duration: Duration::from_secs(6),
        });
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        // Any key press dismisses the modifier discovery popup
        self.modifier_hint = None;

        // Apply keybinding remaps: transform the key event if it matches any
        // configured `from` → `to` remap
        let key = self.apply_remap(key);

        // Modal dialogs take top priority
        if self.dialog.is_some() {
            self.handle_dialog_key(key);
            return;
        }

        // Floating context menu captures all input while open
        if self.context_menu.is_some() {
            self.handle_context_menu_key(key);
            return;
        }

        // Modal overlays take priority
        if self.show_help {
            if key.code == KeyCode::Esc
                || key.code == KeyCode::Char('q')
                || key.code == KeyCode::Char('?')
            {
                self.show_help = false;
            }
            return;
        }

        // Job queue overlay captures all input while open
        if self.show_job_queue {
            self.handle_job_queue_key(key);
            return;
        }

        // User-defined action bindings ([[actions]] tables); exact modifier
        // match so configured chords are predictable
        for (idx, action) in self.user_actions.iter().enumerate() {
            if let Some((mods, code)) = action.parse_key() {
                if key.modifiers == mods && key.code == code {
                    self.run_user_action(idx);
                    return;
                }
            }
        }

        // Tab management works from every input context (but not overlays)
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('t')) => {
                self.new_tab();
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
                self.close_tab();
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::Tab) => {
                self.cycle_tab(1);
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::BackTab) => {
                self.cycle_tab(-1);
                return;
            }
            (KeyModifiers::ALT, KeyCode::Char(d @ '1'..='9')) => {
                let idx = d.to_digit(10).unwrap() as usize - 1;
                self.switch_to_tab(idx);
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('j')) => {
                self.toggle_job_queue();
                return;
            }
            _ => {}
        }

        match self.focus {
            Focus::PathInput => self.handle_path_input_key(key),
            Focus::FilterInput => self.handle_filter_input_key(key),
            Focus::Sidebar => self.handle_sidebar_key(key),
            Focus::MainTable => self.handle_table_key(key),
            Focus::Breadcrumb => self.handle_breadcrumb_key(key),
            Focus::Preview => self.handle_preview_key(key),
        }
    }

    /// Dialog-modal key handling: prompt dialogs capture text editing keys;
    /// otherwise Left/Right (or h/l, Tab) move focus between buttons; Enter
    /// activates the focused button; Esc always cancels.
    fn handle_dialog_key(&mut self, key: KeyEvent) {
        let has_prompt = self.dialog.as_ref().is_some_and(|d| d.prompt.is_some());

        if has_prompt {
            // Tab still cycles buttons; editing keys go to the input line
            if !matches!(key.code, KeyCode::Tab | KeyCode::Enter | KeyCode::Esc) {
                self.handle_prompt_edit_key(key);
                return;
            }
        }

        match key.code {
            KeyCode::Esc => {
                self.cancel_dialog();
            }
            KeyCode::Left | KeyCode::Char('h') if !has_prompt => {
                if let Some(dlg) = &mut self.dialog {
                    dlg.selected_button = dlg.selected_button.saturating_sub(1);
                }
            }
            KeyCode::Right | KeyCode::Char('l') if !has_prompt => {
                let max = self
                    .dialog
                    .as_ref()
                    .map(|d| d.buttons.len().saturating_sub(1))
                    .unwrap_or(0);
                if let Some(dlg) = &mut self.dialog {
                    dlg.selected_button = (dlg.selected_button + 1).min(max);
                }
            }
            KeyCode::Tab => {
                if let Some(dlg) = &mut self.dialog {
                    dlg.selected_button = (dlg.selected_button + 1) % dlg.buttons.len().max(1);
                }
            }
            KeyCode::Enter => {
                let kind = self
                    .dialog
                    .as_ref()
                    .and_then(|d| d.buttons.get(d.selected_button))
                    .map(|b| b.kind.clone());
                match kind {
                    Some(ButtonKind::Cancel) => self.cancel_dialog(),
                    Some(ButtonKind::Confirm) => self.confirm_dialog(),
                    Some(ButtonKind::Resolve(resolution)) => self.resolve_conflict(resolution),
                    None => self.cancel_dialog(),
                }
            }
            _ => {}
        }
    }

    fn handle_prompt_edit_key(&mut self, key: KeyEvent) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let Some(prompt) = &mut dialog.prompt else {
            return;
        };

        match key.code {
            KeyCode::Char(c) => {
                prompt.buffer.insert(prompt.cursor, c);
                prompt.cursor += 1;
            }
            KeyCode::Backspace => {
                if prompt.cursor > 0 {
                    prompt.cursor -= 1;
                    prompt.buffer.remove(prompt.cursor);
                }
            }
            KeyCode::Delete => {
                if prompt.cursor < prompt.buffer.len() {
                    prompt.buffer.remove(prompt.cursor);
                }
            }
            KeyCode::Left => {
                prompt.cursor = prompt.cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                prompt.cursor = (prompt.cursor + 1).min(prompt.buffer.len());
            }
            KeyCode::Home => {
                prompt.cursor = 0;
            }
            KeyCode::End => {
                prompt.cursor = prompt.buffer.len();
            }
            _ => {}
        }
        dialog.selected_button = 0;
    }

    /// Applies keybinding remaps: if the incoming key matches any configured
    /// `from` combination, returns the `to` combination instead. First match
    /// wins. Returns the original key when no remap applies.
    fn apply_remap(&self, key: KeyEvent) -> KeyEvent {
        for (from_mods, from_code, to_mods, to_code) in &self.parsed_remaps {
            if key.modifiers == *from_mods && key.code == *from_code {
                return KeyEvent::new(*to_code, *to_mods);
            }
        }
        key
    }

    /// Cycles focus through visible panels: Sidebar ⇄ Table ⇄ Preview.
    /// Reverse direction with Shift+Tab. Hidden panels are skipped.
    pub fn advance_focus(&mut self, reverse: bool) {
        // In dual-pane mode Tab alternates between the two panes instead
        if self.dual_pane && self.tabs.len() >= 2 {
            let _ = reverse;
            let next = if self.active_tab == 0 { 1 } else { 0 };
            self.switch_to_tab(next);
            return;
        }

        let mut ring = vec![Focus::MainTable];
        if self.show_sidebar {
            ring.insert(0, Focus::Sidebar);
        }
        if self.show_preview {
            ring.push(Focus::Preview);
        }

        let cur = ring.iter().position(|&f| f == self.focus).unwrap_or(0);
        let next = if reverse {
            (cur + ring.len() - 1) % ring.len()
        } else {
            (cur + 1) % ring.len()
        };
        self.focus = ring[next];
    }

    fn handle_table_key(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            // Quit
            (KeyModifiers::NONE, KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('q')) => {
                self.should_quit = true;
            }

            // Navigation
            (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
                if self.visible_len() > 0 {
                    self.tab_mut().table_selected_index =
                        (self.tab().table_selected_index + 1).min(self.visible_len() - 1);
                }
            }
            (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
                self.tab_mut().table_selected_index =
                    self.tab().table_selected_index.saturating_sub(1);
            }
            // Shift+movement extends the range selection from the anchor
            (KeyModifiers::SHIFT, KeyCode::Down) | (KeyModifiers::SHIFT, KeyCode::Char('J')) => {
                self.extend_selection(self.tab().table_selected_index + 1);
            }
            (KeyModifiers::SHIFT, KeyCode::Up) | (KeyModifiers::SHIFT, KeyCode::Char('K')) => {
                self.extend_selection(self.tab().table_selected_index.saturating_sub(1));
            }
            (KeyModifiers::NONE, KeyCode::Home) | (KeyModifiers::NONE, KeyCode::Char('g')) => {
                self.tab_mut().table_selected_index = 0;
            }
            (KeyModifiers::NONE, KeyCode::End) | (KeyModifiers::SHIFT, KeyCode::Char('G')) => {
                self.tab_mut().table_selected_index = self.visible_len().saturating_sub(1);
            }
            (KeyModifiers::NONE, KeyCode::PageDown)
            | (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                self.tab_mut().table_selected_index = (self.tab().table_selected_index + 15)
                    .min(self.visible_len().saturating_sub(1));
            }
            (KeyModifiers::NONE, KeyCode::PageUp) | (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                self.tab_mut().table_selected_index =
                    self.tab().table_selected_index.saturating_sub(15);
            }

            // Enter directory or open file
            (KeyModifiers::NONE, KeyCode::Enter)
            | (KeyModifiers::NONE, KeyCode::Char('l'))
            | (KeyModifiers::NONE, KeyCode::Right) => {
                self.open_selected();
            }

            // Go to parent directory
            (KeyModifiers::NONE, KeyCode::Backspace)
            | (KeyModifiers::NONE, KeyCode::Char('h'))
            | (KeyModifiers::NONE, KeyCode::Left)
            | (KeyModifiers::ALT, KeyCode::Up) => {
                self.navigate_up();
            }

            // History back & forward
            (KeyModifiers::ALT, KeyCode::Left) => {
                self.navigate_back();
            }
            (KeyModifiers::ALT, KeyCode::Right) => {
                self.navigate_forward();
            }

            // Focus switching
            (KeyModifiers::NONE, KeyCode::Tab) => {
                self.advance_focus(false);
            }
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::BackTab) => {
                self.advance_focus(true);
            }

            // Path editing
            (KeyModifiers::CONTROL, KeyCode::Char('l')) => {
                self.focus = Focus::PathInput;
                self.path_input_buffer = self.tab().current_dir.to_string_lossy().to_string();
                self.path_input_cursor = self.path_input_buffer.len();
            }

            // Selection
            (KeyModifiers::NONE, KeyCode::Char(' ')) => {
                self.toggle_selection();
            }
            (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
                self.select_all();
            }
            (KeyModifiers::NONE, KeyCode::Char('*'))
            | (KeyModifiers::SHIFT, KeyCode::Char('8')) => {
                self.invert_selection();
            }
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.clear_selection();
            }

            // Clipboard: desktop copy/cut/paste conventions
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.copy_selected();
            }
            (KeyModifiers::CONTROL, KeyCode::Char('x')) => {
                self.cut_selected();
            }
            (KeyModifiers::CONTROL, KeyCode::Char('v')) => {
                self.paste_clipboard();
            }

            // Sorting
            (KeyModifiers::NONE, KeyCode::Char('s')) => {
                self.cycle_sort_column();
            }
            (KeyModifiers::SHIFT, KeyCode::Char('S'))
            | (KeyModifiers::NONE, KeyCode::Char('r')) => {
                self.reverse_sort();
            }

            // Filter search
            (KeyModifiers::NONE, KeyCode::Char('/')) => {
                self.exit_search_mode();
                self.focus = Focus::FilterInput;
                self.tab_mut().search_cursor = self.tab().search_query.len();
            }

            // Recursive search
            (KeyModifiers::CONTROL, KeyCode::Char('f')) => {
                self.begin_recursive_search();
            }

            // Column resizing
            (KeyModifiers::ALT, KeyCode::Char('[')) => {
                self.resize_name_column(-2);
            }
            (KeyModifiers::ALT, KeyCode::Char(']')) => {
                self.resize_name_column(2);
            }

            // Hidden files toggle
            (KeyModifiers::CONTROL, KeyCode::Char('h'))
            | (KeyModifiers::NONE, KeyCode::Char('.')) => {
                self.toggle_hidden();
            }

            // Sidebar & Preview toggles
            (KeyModifiers::NONE, KeyCode::F(7)) => {
                self.show_preview = !self.show_preview;
            }
            // Cycle preview dock position: Side → Bottom → Side
            (KeyModifiers::SHIFT, KeyCode::F(7)) => {
                use crate::config::PreviewDock;
                self.preview_dock = match self.preview_dock {
                    PreviewDock::Side => PreviewDock::Bottom,
                    PreviewDock::Bottom => PreviewDock::Side,
                };
            }
            // Quick look: open preview and focus it
            (KeyModifiers::NONE, KeyCode::Char('v')) => {
                if !self.show_preview {
                    self.show_preview = true;
                }
                self.focus = Focus::Preview;
            }
            (KeyModifiers::NONE, KeyCode::F(9)) | (KeyModifiers::NONE, KeyCode::Char('b')) => {
                self.show_sidebar = !self.show_sidebar;
            }

            // Delete: trash immediately; `d` asks first; Shift+Delete is
            // permanent and always requires confirmation (M2 dialogs)
            (KeyModifiers::NONE, KeyCode::Delete) => {
                self.trash_selected();
            }
            (KeyModifiers::NONE, KeyCode::Char('d')) => {
                self.request_trash();
            }
            (KeyModifiers::SHIFT, KeyCode::Delete) => {
                self.request_permanent_delete();
            }

            // Rename (F2, desktop convention) & create folder/file
            (KeyModifiers::NONE, KeyCode::F(2)) => {
                self.request_rename();
            }
            // Terminals differ: Ctrl+Shift+N arrives as Shift+'n' or as 'N'
            (mods, KeyCode::Char('n'))
                if mods.contains(KeyModifiers::CONTROL) && mods.contains(KeyModifiers::SHIFT) =>
            {
                self.request_new_file();
            }
            (mods, KeyCode::Char('N')) if mods.contains(KeyModifiers::CONTROL) => {
                self.request_new_file();
            }
            (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
                self.request_new_folder();
            }

            // Context menu (keyboard triggers)
            (KeyModifiers::NONE, KeyCode::Char('m'))
            | (KeyModifiers::NONE, KeyCode::Menu)
            | (KeyModifiers::SHIFT, KeyCode::F(10)) => {
                self.open_context_menu();
            }

            // Dual-pane mode (Commander)
            (KeyModifiers::NONE, KeyCode::F(3)) => {
                self.toggle_dual_pane();
            }
            (KeyModifiers::NONE, KeyCode::F(5)) => {
                self.copy_to_other_pane();
            }
            (KeyModifiers::NONE, KeyCode::F(6)) => {
                self.move_to_other_pane();
            }

            // Desktop integration: terminal & editor
            (KeyModifiers::NONE, KeyCode::F(4)) | (KeyModifiers::NONE, KeyCode::Char('`')) => {
                self.open_terminal();
            }
            (KeyModifiers::NONE, KeyCode::Char('e')) => {
                self.open_in_editor();
            }
            (KeyModifiers::NONE, KeyCode::Char('p')) => {
                self.open_in_pager();
            }

            // Help
            (KeyModifiers::NONE, KeyCode::Char('?')) | (KeyModifiers::NONE, KeyCode::F(1)) => {
                self.show_help = true;
            }

            // Unbound chord: surface every binding that uses these modifiers
            _ => {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    || key.modifiers.contains(KeyModifiers::ALT)
                {
                    self.show_modifier_hint(key.modifiers);
                }
            }
        }
    }

    fn handle_sidebar_key(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('q')) => {
                self.should_quit = true;
            }
            (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
                if !self.sidebar_rendered_paths.is_empty() {
                    let mut next = (self.sidebar_selected_index + 1)
                        .min(self.sidebar_rendered_paths.len() - 1);
                    while next < self.sidebar_rendered_paths.len() - 1
                        && self.sidebar_rendered_paths[next].is_none()
                    {
                        next += 1;
                    }
                    if self
                        .sidebar_rendered_paths
                        .get(next)
                        .is_some_and(|p| p.is_some())
                    {
                        self.sidebar_selected_index = next;
                    }
                }
            }
            (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
                if self.sidebar_selected_index > 0 {
                    let mut prev = self.sidebar_selected_index - 1;
                    while prev > 0 && self.sidebar_rendered_paths[prev].is_none() {
                        prev -= 1;
                    }
                    if self
                        .sidebar_rendered_paths
                        .get(prev)
                        .is_some_and(|p| p.is_some())
                    {
                        self.sidebar_selected_index = prev;
                    }
                }
            }
            // Expand / lazy-load children of selected node
            (KeyModifiers::NONE, KeyCode::Char('l')) | (KeyModifiers::NONE, KeyCode::Right) => {
                self.toggle_tree_node(true);
            }
            // Collapse selected node
            (KeyModifiers::NONE, KeyCode::Char('h')) | (KeyModifiers::NONE, KeyCode::Left) => {
                self.toggle_tree_node(false);
            }
            // Navigate to selected bookmark or tree directory
            (KeyModifiers::NONE, KeyCode::Enter) => {
                if let Some(Some(path)) = self
                    .sidebar_rendered_paths
                    .get(self.sidebar_selected_index)
                    .cloned()
                {
                    self.navigate_to(path);
                    self.focus = Focus::MainTable;
                }
            }
            (KeyModifiers::NONE, KeyCode::Tab) => {
                self.advance_focus(false);
            }
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::BackTab) => {
                self.advance_focus(true);
            }
            (KeyModifiers::NONE, KeyCode::Char('?')) | (KeyModifiers::NONE, KeyCode::F(1)) => {
                self.show_help = true;
            }
            _ => {}
        }
    }

    fn handle_path_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::MainTable;
            }
            KeyCode::Enter => {
                let target_path = PathBuf::from(&self.path_input_buffer);
                if target_path.exists() {
                    self.navigate_to(target_path);
                    self.focus = Focus::MainTable;
                } else {
                    self.set_status_error(format!(
                        "Path does not exist: {}",
                        self.path_input_buffer
                    ));
                }
            }
            KeyCode::Char(c) => {
                self.path_input_buffer.insert(self.path_input_cursor, c);
                self.path_input_cursor += 1;
            }
            KeyCode::Backspace => {
                if self.path_input_cursor > 0 {
                    self.path_input_cursor -= 1;
                    self.path_input_buffer.remove(self.path_input_cursor);
                }
            }
            KeyCode::Delete => {
                if self.path_input_cursor < self.path_input_buffer.len() {
                    self.path_input_buffer.remove(self.path_input_cursor);
                }
            }
            KeyCode::Left => {
                self.path_input_cursor = self.path_input_cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                self.path_input_cursor =
                    (self.path_input_cursor + 1).min(self.path_input_buffer.len());
            }
            KeyCode::Tab => {
                // Path auto-completion
                self.autocomplete_path();
            }
            _ => {}
        }
    }

    fn handle_filter_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if self.tab().search_mode {
                    self.exit_search_mode();
                } else {
                    self.tab_mut().search_query.clear();
                    self.tab_mut().search_cursor = 0;
                    self.reapply_filter();
                }
                self.focus = Focus::MainTable;
            }
            KeyCode::Enter => {
                self.focus = Focus::MainTable;
            }
            KeyCode::Tab => {
                self.focus = Focus::MainTable;
            }
            KeyCode::Char(c) => {
                {
                    let tab = self.tab_mut();
                    tab.search_query.insert(tab.search_cursor, c);
                    tab.search_cursor += 1;
                }
                if self.tab().search_mode {
                    self.restart_search();
                } else {
                    self.reapply_filter();
                }
            }
            KeyCode::Backspace => {
                if self.tab().search_cursor > 0 {
                    {
                        let tab = self.tab_mut();
                        tab.search_cursor -= 1;
                        tab.search_query.remove(tab.search_cursor);
                    }
                    if self.tab().search_mode {
                        self.restart_search();
                    } else {
                        self.reapply_filter();
                    }
                }
            }
            KeyCode::Delete => {
                if self.tab().search_cursor < self.tab().search_query.len() {
                    {
                        let tab = self.tab_mut();
                        let c = tab.search_cursor;
                        tab.search_query.remove(c);
                    }
                    if self.tab().search_mode {
                        self.restart_search();
                    } else {
                        self.reapply_filter();
                    }
                }
            }
            KeyCode::Left => {
                self.tab_mut().search_cursor = self.tab().search_cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                self.tab_mut().search_cursor =
                    (self.tab().search_cursor + 1).min(self.tab().search_query.len());
            }
            _ => {}
        }
    }

    fn handle_breadcrumb_key(&mut self, key: KeyEvent) {
        // Popover open: navigate the sibling list
        if self.breadcrumb_popover.is_some() {
            match key.code {
                KeyCode::Esc | KeyCode::Left | KeyCode::Right | KeyCode::Char('s') => {
                    self.breadcrumb_popover = None;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if let Some(pop) = &mut self.breadcrumb_popover {
                        if pop.selected + 1 < pop.items.len() {
                            pop.selected += 1;
                            if pop.selected >= pop.scroll_offset + pop.max_visible {
                                pop.scroll_offset = pop.selected + 1 - pop.max_visible;
                            }
                        }
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if let Some(pop) = &mut self.breadcrumb_popover {
                        if pop.selected > 0 {
                            pop.selected -= 1;
                            if pop.selected < pop.scroll_offset {
                                pop.scroll_offset = pop.selected;
                            }
                        }
                    }
                }
                KeyCode::Enter => {
                    let target = self
                        .breadcrumb_popover
                        .as_ref()
                        .and_then(|pop| pop.items.get(pop.selected).cloned());
                    self.breadcrumb_popover = None;
                    if let Some(path) = target {
                        self.navigate_to(path);
                        self.focus = Focus::MainTable;
                    }
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Down => {
                self.focus = Focus::MainTable;
            }
            KeyCode::Left => {
                self.breadcrumb_selected = self.breadcrumb_selected.saturating_sub(1);
            }
            KeyCode::Right => {
                let max = self.breadcrumb_segments.len().saturating_sub(1);
                self.breadcrumb_selected = (self.breadcrumb_selected + 1).min(max);
            }
            KeyCode::Enter => {
                let target = self
                    .breadcrumb_segments
                    .get(self.breadcrumb_selected)
                    .map(|seg| seg.path.clone());
                if let Some(path) = target {
                    self.navigate_to(path);
                    self.focus = Focus::MainTable;
                }
            }
            KeyCode::Char('s') => {
                self.open_breadcrumb_siblings();
            }
            KeyCode::Char('m') | KeyCode::F(10) => {
                // Context menu for the focused chip (right-click stays bound
                // to the sibling dropdown)
                if let Some(seg) = self.breadcrumb_segments.get(self.breadcrumb_selected) {
                    let x = seg.area.x + seg.area.width / 2;
                    let path = seg.path.clone();
                    self.open_context_menu_for_path(path, x, 2);
                }
            }
            KeyCode::Tab => {
                self.advance_focus(false);
            }
            KeyCode::BackTab => {
                self.advance_focus(true);
            }
            _ => {}
        }
    }

    fn handle_preview_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::MainTable;
            }
            KeyCode::Tab => {
                self.advance_focus(false);
            }
            KeyCode::BackTab => {
                self.advance_focus(true);
            }
            _ => {}
        }
    }

    fn autocomplete_path(&mut self) {
        let input = Path::new(&self.path_input_buffer);
        let parent = input.parent().unwrap_or_else(|| Path::new("/"));
        let prefix = input
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();

        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.to_lowercase().starts_with(&prefix) {
                    let completed = parent.join(&name);
                    let mut completed_str = completed.to_string_lossy().to_string();
                    if entry.path().is_dir() {
                        completed_str.push('/');
                    }
                    self.path_input_buffer = completed_str;
                    self.path_input_cursor = self.path_input_buffer.len();
                    break;
                }
            }
        }
    }

    pub fn handle_mouse_event(&mut self, mouse: MouseEvent) {
        // Any click dismisses the modifier discovery popup
        self.modifier_hint = None;

        // Job queue overlay: row click selects, [ Cancel ] click cancels,
        // outside click closes, wheel moves the cursor. Sits below modal
        // dialogs and the context menu in z-order, so yield to those first.
        if self.show_job_queue && self.dialog.is_none() && self.context_menu.is_none() {
            let rect = self.job_queue_rect;
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if Self::point_in_rect(mouse.column, mouse.row, rect) {
                        // Cancel-button rects are indexed per visible job row
                        if let Some(idx) = self
                            .job_queue_cancel_rects
                            .iter()
                            .position(|r| Self::point_in_rect(mouse.column, mouse.row, *r))
                        {
                            self.job_queue_selected =
                                idx.min(self.active_ops.len().saturating_sub(1));
                            self.cancel_selected_job();
                        } else {
                            // Row click selects (top border + header offset handled by renderer rows)
                            let inner_y = rect.y + 2; // top border + column header line
                            if mouse.row >= inner_y && mouse.row < rect.bottom().saturating_sub(1) {
                                let row = (mouse.row - inner_y) as usize;
                                if row < self.active_ops.len() {
                                    self.job_queue_selected = row;
                                }
                            }
                        }
                    } else {
                        self.close_job_queue();
                    }
                }
                MouseEventKind::ScrollUp => self.job_queue_move_selection(-1),
                MouseEventKind::ScrollDown => self.job_queue_move_selection(1),
                _ => {}
            }
            return;
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let now = Instant::now();
                let is_double_click = now.duration_since(self.last_click_time)
                    < Duration::from_millis(350)
                    && (mouse.column, mouse.row) == self.last_click_pos;
                self.last_click_time = now;
                self.last_click_pos = (mouse.column, mouse.row);

                // 0a. Context menu: item click activates, outside click closes
                if let Some(menu) = &self.context_menu {
                    let rect = menu.screen_rect;
                    let inside = Self::point_in_rect(mouse.column, mouse.row, rect);
                    let target = if inside {
                        let row_idx = (mouse.row - rect.y - 1) as usize; // top border offset
                        menu.items
                            .get(menu.scroll_offset + row_idx)
                            .filter(|item| !item.is_separator())
                            .and_then(|item| item.action)
                    } else {
                        None
                    };
                    self.context_menu = None;
                    if let Some(action) = target {
                        self.execute_context_action(action);
                    }
                    return;
                }

                // 0b. Modal dialog hit-testing: buttons activate on click;
                // clicks outside a modal are swallowed (dialogs are exclusive)
                if self.dialog.is_some() {
                    let rects = self.dialog.as_ref().unwrap().button_rects.clone();
                    let hit = rects
                        .iter()
                        .position(|r| Self::point_in_rect(mouse.column, mouse.row, *r));
                    if let Some(idx) = hit {
                        if let Some(dlg) = &mut self.dialog {
                            dlg.selected_button = idx;
                        }
                        let kind = self
                            .dialog
                            .as_ref()
                            .and_then(|d| d.buttons.get(idx))
                            .map(|b| b.kind.clone());
                        match kind {
                            Some(ButtonKind::Cancel) => self.cancel_dialog(),
                            Some(ButtonKind::Confirm) => self.confirm_dialog(),
                            Some(ButtonKind::Resolve(resolution)) => {
                                self.resolve_conflict(resolution)
                            }
                            None => self.cancel_dialog(),
                        }
                    }
                    return;
                }

                // 0. Sibling popover hit-testing (when open)
                if let Some(pop) = &self.breadcrumb_popover {
                    let rect = pop.screen_rect;
                    let inside = mouse.row >= rect.y
                        && mouse.row < rect.y + rect.height
                        && mouse.column >= rect.x
                        && mouse.column < rect.x + rect.width;
                    let target = if inside {
                        let row_idx = (mouse.row - rect.y) as usize;
                        pop.items.get(pop.scroll_offset + row_idx).cloned()
                    } else {
                        None
                    };
                    self.breadcrumb_popover = None;
                    if let Some(path) = target {
                        self.navigate_to(path);
                        self.focus = Focus::MainTable;
                    }
                    return; // click outside popover closes it without side effects
                }

                // 0c. Tab bar chips: click switches tabs
                let chip_hit = self
                    .tab_chips
                    .iter()
                    .find(|chip| Self::point_in_rect(mouse.column, mouse.row, chip.rect))
                    .map(|chip| chip.index);
                if let Some(idx) = chip_hit {
                    self.switch_to_tab(idx);
                    return;
                }

                // 1. Breadcrumb bar: left-click navigates to the chip
                if mouse.row == 1 {
                    for (i, seg) in self.breadcrumb_segments.iter().enumerate() {
                        if mouse.column >= seg.area.x && mouse.column < seg.area.x + seg.area.width
                        {
                            let target_path = seg.path.clone();
                            self.focus = Focus::Breadcrumb;
                            self.breadcrumb_selected = i;
                            self.navigate_to(target_path);
                            self.focus = Focus::MainTable;
                            return;
                        }
                    }
                    self.focus = Focus::Breadcrumb;
                    return;
                }

                // 2. Sidebar panel: click selects, double-click opens
                if self.sidebar_rect.width > 0
                    && mouse.column >= self.sidebar_rect.x
                    && mouse.column < self.sidebar_rect.x + self.sidebar_rect.width
                    && mouse.row >= self.sidebar_rect.y
                    && mouse.row < self.sidebar_rect.y + self.sidebar_rect.height
                {
                    self.focus = Focus::Sidebar;
                    let inner_y = self.sidebar_rect.y + 1; // top border/title row
                    let idx = self.sidebar_scroll_offset + (mouse.row - inner_y) as usize;
                    if let Some(Some(path)) = self.sidebar_rendered_paths.get(idx).cloned() {
                        self.sidebar_selected_index = idx;
                        if is_double_click {
                            self.navigate_to(path);
                            self.focus = Focus::MainTable;
                        }
                    }
                    return;
                }

                // 3. Preview panel: click moves focus
                if self.preview_rect.width > 0
                    && mouse.column >= self.preview_rect.x
                    && mouse.column < self.preview_rect.x + self.preview_rect.width
                    && mouse.row >= self.preview_rect.y
                    && mouse.row < self.preview_rect.y + self.preview_rect.height
                {
                    self.focus = Focus::Preview;
                    return;
                }

                // 3b. Dual-pane: clicking the inactive pane focuses it first
                if self.dual_pane && self.tabs.len() >= 2 {
                    let in_left = Self::point_in_rect(mouse.column, mouse.row, self.pane_rects[0]);
                    let in_right = Self::point_in_rect(mouse.column, mouse.row, self.pane_rects[1]);
                    if in_left || in_right {
                        let pane = if in_left { 0 } else { 1 };
                        if pane != self.active_tab {
                            self.switch_to_tab(pane);
                        }
                        // Active-pane clicks fall through to row handling below
                    }
                }

                // 4. Table headers (column sorting or resize)
                if mouse.row == self.table_rect.y + 1 && !self.table_header_rects.is_empty() {
                    // Separator between Name and Size column starts a drag-resize
                    if let Some(size_col) = self.table_header_rects.first() {
                        let sep_x = size_col.x;
                        if mouse.column.abs_diff(sep_x) <= 1 {
                            if is_double_click {
                                // Auto-fit: drop manual override
                                self.name_column_width_override = None;
                            } else {
                                self.column_drag = Some(ColumnDrag {
                                    start_x: mouse.column,
                                    base_width: self.name_column_effective_width,
                                });
                            }
                            return;
                        }
                    }
                    for header in &self.table_header_rects {
                        if mouse.column >= header.x && mouse.column < header.x + header.width {
                            self.set_sort(header.column);
                            return;
                        }
                    }
                }

                // 5. Table rows: click selects, double-click opens; clicking
                // empty space below the last row still moves focus here
                if mouse.row > self.table_rect.y + 1
                    && mouse.row < self.table_rect.y + self.table_rect.height - 1
                {
                    let row_idx = (mouse.row - self.table_rect.y - 2) as usize;
                    let target_idx = self.tab().table_scroll_offset + row_idx;

                    if target_idx < self.visible_len() {
                        self.focus = Focus::MainTable;

                        // Shift+Click extends the range selection from the anchor
                        if mouse.modifiers.contains(KeyModifiers::SHIFT) {
                            self.extend_selection(target_idx);
                            self.drag_candidate = None;
                        } else {
                            self.tab_mut().table_selected_index = target_idx;
                            if is_double_click {
                                self.open_selected();
                                self.drag_candidate = None;
                            } else if let Some(entry) = self.tab().visible_entry_at(target_idx) {
                                // Press on a row may become a drag gesture
                                let paths = if self.tab().multi_selected.contains(&entry.path) {
                                    let mut sel: Vec<PathBuf> =
                                        self.tab().multi_selected.iter().cloned().collect();
                                    sel.sort();
                                    sel
                                } else {
                                    vec![entry.path.clone()]
                                };
                                self.drag_candidate = Some(DragCandidate {
                                    start: (mouse.column, mouse.row),
                                    paths,
                                });
                            }
                        }
                    } else {
                        self.focus = Focus::MainTable;
                        self.drag_candidate = None;
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                // Right-click a sidebar tree/bookmark row: select it and open
                // a path-targeted context menu
                if Self::point_in_rect(mouse.column, mouse.row, self.sidebar_rect) {
                    let idx =
                        self.sidebar_scroll_offset + (mouse.row - self.sidebar_rect.y - 1) as usize;
                    if let Some(Some(path)) = self.sidebar_rendered_paths.get(idx).cloned() {
                        self.focus = Focus::Sidebar;
                        self.sidebar_selected_index = idx;
                        self.open_context_menu_for_path(path, mouse.column, mouse.row);
                    }
                    return;
                }

                // Right-click a breadcrumb chip to open its sibling dropdown
                if mouse.row == 1 {
                    for (i, seg) in self.breadcrumb_segments.iter().enumerate() {
                        if mouse.column >= seg.area.x && mouse.column < seg.area.x + seg.area.width
                        {
                            self.focus = Focus::Breadcrumb;
                            self.breadcrumb_selected = i;
                            self.open_breadcrumb_siblings();
                            return;
                        }
                    }
                }

                // Right-click a table row: select it and open the context menu
                if mouse.row > self.table_rect.y + 1
                    && mouse.row < self.table_rect.y + self.table_rect.height - 1
                {
                    let target_idx = self.tab().table_scroll_offset
                        + (mouse.row - self.table_rect.y - 2) as usize;
                    if target_idx < self.visible_len() {
                        self.tab_mut().table_selected_index = target_idx;
                        self.focus = Focus::MainTable;
                    }
                    let anchor_y = self.table_rect.y.saturating_add(2).saturating_add(
                        (self
                            .tab()
                            .table_selected_index
                            .saturating_sub(self.tab().table_scroll_offset))
                            as u16,
                    );
                    self.open_context_menu_at(mouse.column, anchor_y);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(drag) = &self.column_drag {
                    let inner_width = self.table_rect.width.saturating_sub(2);
                    let min_w = 10u16;
                    let max_w = inner_width.saturating_sub(42).max(min_w);
                    let delta = (mouse.column as i32) - (drag.start_x as i32);
                    let new_w =
                        (drag.base_width as i32 + delta).clamp(min_w as i32, max_w as i32) as u16;
                    self.name_column_width_override = Some(new_w);
                } else if let Some(state) = &mut self.drag_drop {
                    state.hover = (mouse.column, mouse.row);
                    state.copy = mouse.modifiers.contains(KeyModifiers::CONTROL);
                } else if let Some(candidate) = &self.drag_candidate {
                    // Activate the drag once the cursor clearly leaves the press point
                    let dx = mouse.column.abs_diff(candidate.start.0);
                    let dy = mouse.row.abs_diff(candidate.start.1);
                    if dx.max(dy) >= 3 {
                        self.drag_drop = Some(DragDropState {
                            start: candidate.start,
                            paths: candidate.paths.clone(),
                            copy: mouse.modifiers.contains(KeyModifiers::CONTROL),
                            hover: (mouse.column, mouse.row),
                        });
                    }
                }
            }
            MouseEventKind::Up(_) => {
                self.column_drag = None;
                self.finish_drag_drop();
                self.drag_candidate = None;
            }
            MouseEventKind::ScrollDown => {
                if self.context_menu.is_some() {
                    self.context_menu_move_selection(1);
                } else if Self::point_in_rect(mouse.column, mouse.row, self.sidebar_rect) {
                    self.scroll_sidebar(1);
                } else if self.visible_len() > 0 {
                    self.tab_mut().table_selected_index =
                        (self.tab().table_selected_index + 3).min(self.visible_len() - 1);
                }
            }
            MouseEventKind::ScrollUp => {
                if self.context_menu.is_some() {
                    self.context_menu_move_selection(-1);
                } else if Self::point_in_rect(mouse.column, mouse.row, self.sidebar_rect) {
                    self.scroll_sidebar(-1);
                } else {
                    self.tab_mut().table_selected_index =
                        self.tab().table_selected_index.saturating_sub(3);
                }
            }
            // Horizontal scroll = history navigation (physical Back/Forward
            // mouse buttons are not decoded by crossterm upstream)
            MouseEventKind::ScrollLeft => {
                if self.breadcrumb_popover.is_some() {
                    self.breadcrumb_popover_scroll(-1);
                } else {
                    self.navigate_back();
                }
            }
            MouseEventKind::ScrollRight => {
                if self.breadcrumb_popover.is_some() {
                    self.breadcrumb_popover_scroll(1);
                } else {
                    self.navigate_forward();
                }
            }
            // Middle-click toggles selection (mouse capture prevents terminal paste)
            MouseEventKind::Down(MouseButton::Middle) => {
                if mouse.row > self.table_rect.y + 1
                    && mouse.row < self.table_rect.y + self.table_rect.height - 1
                {
                    let target_idx = self.tab().table_scroll_offset
                        + (mouse.row - self.table_rect.y - 2) as usize;
                    if target_idx < self.visible_len() {
                        self.tab_mut().table_selected_index = target_idx;
                        self.focus = Focus::MainTable;
                        self.toggle_selection();
                    }
                } else if Self::point_in_rect(mouse.column, mouse.row, self.sidebar_rect) {
                    self.focus = Focus::Sidebar;
                    let idx =
                        self.sidebar_scroll_offset + (mouse.row - self.sidebar_rect.y - 1) as usize;
                    if self
                        .sidebar_rendered_paths
                        .get(idx)
                        .is_some_and(|p| p.is_some())
                    {
                        self.sidebar_selected_index = idx;
                    }
                }
            }
            _ => {}
        }
    }

    /// Scrolls the open sibling popover by whole items (horizontal wheel).
    fn breadcrumb_popover_scroll(&mut self, delta: i32) {
        let Some(pop) = &mut self.breadcrumb_popover else {
            return;
        };
        let max_offset = pop
            .items
            .len()
            .saturating_sub(pop.max_visible.min(pop.items.len().max(1)));
        pop.scroll_offset = if delta >= 0 {
            (pop.scroll_offset + delta as usize).min(max_offset)
        } else {
            pop.scroll_offset.saturating_sub((-delta) as usize)
        };
        // Keep the selected item within the visible window
        if pop.selected < pop.scroll_offset {
            pop.selected = pop.scroll_offset;
        } else if pop.selected >= pop.scroll_offset + pop.max_visible.min(pop.items.len()) {
            pop.selected = pop.scroll_offset + pop.max_visible.min(pop.items.len()) - 1;
        }
    }

    fn point_in_rect(x: u16, y: u16, rect: Rect) -> bool {
        x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
    }

    /// Moves the sidebar cursor by `delta` selectable entries (skips headers).
    fn scroll_sidebar(&mut self, delta: i32) {
        let len = self.sidebar_rendered_paths.len();
        if len == 0 {
            return;
        }
        let mut idx = self.sidebar_selected_index as i32;
        let step = if delta >= 0 { 1 } else { -1 };
        let mut remaining = delta.abs();
        while remaining > 0 {
            idx += step;
            if idx < 0 || idx >= len as i32 {
                return;
            }
            if self.sidebar_rendered_paths[idx as usize].is_some() {
                self.sidebar_selected_index = idx as usize;
                remaining -= 1;
            }
        }
    }

    /// Shows the modifier discovery popup for an unbound chord: lists every
    /// binding that uses these modifiers (context-filtered). Auto-expires
    /// via `tick` since bare modifier release is undetectable without the
    /// Kitty keyboard protocol.
    pub fn show_modifier_hint(&mut self, mods: KeyModifiers) {
        let focus_ctx = crate::keys::focus_to_context(self.focus);
        if !crate::keys::bindings_using_modifiers(mods, focus_ctx).is_empty() {
            self.modifier_hint = Some((mods, Instant::now()));
        }
    }

    /// Human-readable names of the modifiers currently shown in the popup.
    pub fn modifier_hint_names(&self) -> Vec<&'static str> {
        let Some((mods, _)) = self.modifier_hint else {
            return Vec::new();
        };
        let mut names = Vec::new();
        if mods.contains(KeyModifiers::CONTROL) {
            names.push("Ctrl");
        }
        if mods.contains(KeyModifiers::ALT) {
            names.push("Alt");
        }
        names
    }

    pub fn tick(&mut self) {
        if let Some(ref msg) = self.status_message {
            if msg.created_at.elapsed() > msg.duration {
                self.status_message = None;
            }
        }

        // Modifier discovery popup expires on its own (no keyup events exist
        // without the Kitty keyboard protocol)
        if let Some((_, shown_at)) = self.modifier_hint {
            if shown_at.elapsed() > MODIFIER_HINT_DURATION {
                self.modifier_hint = None;
            }
        }

        // Coalesce filesystem change bursts into a single refresh once quiet
        if let Some(t) = self.pending_refresh {
            if t.elapsed() >= REFRESH_DEBOUNCE {
                self.pending_refresh = None;
                self.refresh();
            }
        }
    }
}

/// Human-friendly summary for dialog messages: a quoted name for one item,
/// "N items" with a couple of example names for batches.
fn summarize_paths(paths: &[PathBuf]) -> String {
    if paths.len() == 1 {
        format!(
            "\"{}\"",
            paths[0].file_name().unwrap_or_default().to_string_lossy()
        )
    } else {
        let mut examples: Vec<String> = paths
            .iter()
            .take(2)
            .map(|p| {
                format!(
                    "\"{}\"",
                    p.file_name().unwrap_or_default().to_string_lossy()
                )
            })
            .collect();
        if paths.len() > 3 {
            examples.push("…".to_string());
        } else if paths.len() == 3 {
            examples.push(format!(
                "\"{}\"",
                paths[2].file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        format!("{} ({})", paths.len(), examples.join(", "))
    }
}

/// Checks whether an executable exists on `$PATH` without spawning it.
fn which_exists(prog: &str) -> bool {
    if prog.contains('/') {
        return Path::new(prog).exists();
    }
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(prog);
                candidate.is_file() && is_executable(&candidate)
            })
        })
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
