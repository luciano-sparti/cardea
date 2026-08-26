use crossterm::event::KeyModifiers;

/// Category grouping used by the help overlay and the discovery popups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Navigation,
    Selection,
    Operations,
    TabsPanes,
    Views,
    Desktop,
}

impl Category {
    pub fn title(&self) -> &'static str {
        match self {
            Category::Navigation => "NAVIGATION",
            Category::Selection => "SORTING & SELECTION",
            Category::Operations => "OPERATIONS",
            Category::TabsPanes => "TABS & DUAL-PANE",
            Category::Views => "VIEWS & FILTERS",
            Category::Desktop => "DESKTOP INTEGRATION",
        }
    }
}

/// Bitflags for which focus contexts a binding is active in.
/// The modifier-hint popup filters by these so irrelevant bindings
/// are hidden when focused on a specific panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusContext(u8);

impl FocusContext {
    pub const GLOBAL: FocusContext = FocusContext(0xFF);
    pub const TABLE: FocusContext = FocusContext(0x01);
    pub const SIDEBAR: FocusContext = FocusContext(0x02);
    pub const PREVIEW: FocusContext = FocusContext(0x04);
    pub const BREADCRUMB: FocusContext = FocusContext(0x08);
    pub const PATH_INPUT: FocusContext = FocusContext(0x10);
    pub const FILTER_INPUT: FocusContext = FocusContext(0x20);

    pub const fn contains(self, other: FocusContext) -> bool {
        (self.0 & other.0) != 0
    }
}

/// One documented keybinding. The registry mirrors the dispatch tables in
/// `app.rs` and drives both the cheat sheet and the modifier-hint popup so
/// descriptions live in exactly one place.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    /// Modifier(s) this chord uses (matching also accepts supersets)
    pub mask: KeyModifiers,
    pub key: &'static str,
    pub desc: &'static str,
    pub category: Category,
    /// Which focus contexts this binding is active in.
    /// `GLOBAL` means always shown; otherwise only shown when the
    /// focused panel matches one of the specified contexts.
    pub contexts: FocusContext,
}

impl Binding {
    /// True when the pressed modifiers include at least one of this
    /// binding's modifiers (Ctrl/Alt only; Shift alone never triggers).
    pub fn matches_modifier(&self, pressed: KeyModifiers) -> bool {
        let relevant = pressed.intersection(KeyModifiers::CONTROL | KeyModifiers::ALT);
        !relevant.is_empty() && self.mask.intersects(relevant)
    }
}

/// Maps `crate::app::Focus` variants to `FocusContext` bitflags for filtering.
pub fn focus_to_context(focus: crate::app::Focus) -> FocusContext {
    match focus {
        crate::app::Focus::MainTable => FocusContext::TABLE,
        crate::app::Focus::Sidebar => FocusContext::SIDEBAR,
        crate::app::Focus::Preview => FocusContext::PREVIEW,
        crate::app::Focus::Breadcrumb => FocusContext::BREADCRUMB,
        crate::app::Focus::PathInput => FocusContext::PATH_INPUT,
        crate::app::Focus::FilterInput => FocusContext::FILTER_INPUT,
    }
}

pub const BINDINGS: &[Binding] = &[
    // Navigation
    Binding {
        mask: KeyModifiers::NONE,
        key: "j / ↓",
        desc: "Move cursor down",
        category: Category::Navigation,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "k / ↑",
        desc: "Move cursor up",
        category: Category::Navigation,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "h / ← / Backspace",
        desc: "Go to parent directory",
        category: Category::Navigation,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "l / → / Enter",
        desc: "Open folder or file",
        category: Category::Navigation,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::ALT,
        key: "← / →",
        desc: "History back / forward",
        category: Category::Navigation,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::ALT,
        key: "↑",
        desc: "Go to parent directory",
        category: Category::Navigation,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "d / u",
        desc: "Page down / up",
        category: Category::Navigation,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "g / Home",
        desc: "Jump to first item",
        category: Category::Navigation,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::SHIFT,
        key: "G / End",
        desc: "Jump to last item",
        category: Category::Navigation,
        contexts: FocusContext::TABLE,
    },
    // Views & filters
    Binding {
        mask: KeyModifiers::NONE,
        key: "/",
        desc: "Quick filter in current directory",
        category: Category::Views,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "f",
        desc: "Recursive search across subdirectories",
        category: Category::Views,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "h",
        desc: "Toggle hidden files",
        category: Category::Views,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: ".",
        desc: "Toggle hidden files",
        category: Category::Views,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "s",
        desc: "Cycle sort column",
        category: Category::Views,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "r / S",
        desc: "Reverse sort direction",
        category: Category::Views,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "F7",
        desc: "Toggle preview panel",
        category: Category::Views,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::SHIFT,
        key: "F7",
        desc: "Cycle preview dock (side/bottom)",
        category: Category::Views,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "v",
        desc: "Quick look (open preview)",
        category: Category::Views,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "b / F9",
        desc: "Toggle sidebar",
        category: Category::Views,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "l",
        desc: "Edit path bar",
        category: Category::Views,
        contexts: FocusContext::TABLE,
    },
    // Selection
    Binding {
        mask: KeyModifiers::NONE,
        key: "Space",
        desc: "Toggle selection",
        category: Category::Selection,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "a",
        desc: "Select all",
        category: Category::Selection,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "*",
        desc: "Invert selection",
        category: Category::Selection,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "Esc",
        desc: "Clear selection / close overlays",
        category: Category::Selection,
        contexts: FocusContext::GLOBAL,
    },
    // Operations
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "c",
        desc: "Copy selection to clipboard",
        category: Category::Operations,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "x",
        desc: "Cut selection",
        category: Category::Operations,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "v",
        desc: "Paste into current directory",
        category: Category::Operations,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "Delete",
        desc: "Move to Trash",
        category: Category::Operations,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "d",
        desc: "Move to Trash with confirmation",
        category: Category::Operations,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::SHIFT,
        key: "Delete",
        desc: "Delete permanently (confirmed)",
        category: Category::Operations,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "F2",
        desc: "Rename selected item",
        category: Category::Operations,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "n",
        desc: "New folder",
        category: Category::Operations,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "Shift+n",
        desc: "New empty file",
        category: Category::Operations,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "m / Menu / S-F10",
        desc: "Context menu",
        category: Category::Operations,
        contexts: FocusContext::TABLE,
    },
    // Tabs & panes
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "t",
        desc: "New tab",
        category: Category::TabsPanes,
        contexts: FocusContext::GLOBAL,
    },
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "w",
        desc: "Close tab",
        category: Category::TabsPanes,
        contexts: FocusContext::GLOBAL,
    },
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "Tab",
        desc: "Cycle tabs forward",
        category: Category::TabsPanes,
        contexts: FocusContext::GLOBAL,
    },
    Binding {
        mask: KeyModifiers::ALT,
        key: "1..9",
        desc: "Jump to tab N",
        category: Category::TabsPanes,
        contexts: FocusContext::GLOBAL,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "F3",
        desc: "Toggle dual-pane view",
        category: Category::TabsPanes,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "F5 / F6",
        desc: "Copy / move to opposite pane",
        category: Category::TabsPanes,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::CONTROL,
        key: "j",
        desc: "Background job queue",
        category: Category::TabsPanes,
        contexts: FocusContext::GLOBAL,
    },
    // Desktop
    Binding {
        mask: KeyModifiers::NONE,
        key: "` / F4",
        desc: "Open terminal here",
        category: Category::Desktop,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "e",
        desc: "Edit file in $EDITOR/$VISUAL",
        category: Category::Desktop,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "p",
        desc: "View file in $PAGER",
        category: Category::Desktop,
        contexts: FocusContext::TABLE,
    },
    Binding {
        mask: KeyModifiers::NONE,
        key: "? / F1",
        desc: "Full keyboard shortcuts",
        category: Category::Desktop,
        contexts: FocusContext::GLOBAL,
    },
];

/// All bindings that use at least one of the given modifiers, filtered by
/// the current focus context. Bindings with `GLOBAL` context are always
/// shown; others only when the focused panel matches.
pub fn bindings_using_modifiers(pressed: KeyModifiers, focus_ctx: FocusContext) -> Vec<&'static Binding> {
    BINDINGS
        .iter()
        .filter(|b| b.matches_modifier(pressed) && b.contexts.contains(focus_ctx))
        .collect()
}
