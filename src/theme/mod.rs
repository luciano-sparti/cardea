use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub mod syntax;
pub mod watcher;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub border: Color,
    pub border_focus: Color,

    // Tree / Sidebar
    pub tree_branch: Color,
    pub tree_folder: Color,
    pub tree_folder_expanded: Color,
    pub tree_bookmark: Color,

    // Details Table
    pub table_header_bg: Color,
    pub table_header_fg: Color,
    pub table_selected_bg: Color,
    pub table_selected_fg: Color,
    pub table_row_alt_bg: Color,

    // Breadcrumb Bar
    pub breadcrumb_bg: Color,
    pub breadcrumb_fg: Color,
    pub breadcrumb_active_bg: Color,
    pub breadcrumb_active_fg: Color,
    pub breadcrumb_arrow: Color,

    // Status Bar
    pub status_bg: Color,
    pub status_fg: Color,
    pub status_key: Color,
    pub status_info: Color,
    pub status_warn: Color,
    pub status_error: Color,

    // File Types
    pub file_dir: Color,
    pub file_exec: Color,
    pub file_symlink: Color,
    pub file_archive: Color,
    pub file_image: Color,
    pub file_doc: Color,
    pub file_regular: Color,
    pub file_hidden: Color,

    // Filter / Search
    pub filter_match: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::catppuccin_mocha()
    }
}

impl Theme {
    pub fn catppuccin_mocha() -> Self {
        Self {
            name: "Catppuccin Mocha".to_string(),
            bg: Color::Rgb(30, 30, 46),              // #1e1e2e Base
            fg: Color::Rgb(205, 214, 244),           // #cdd6f4 Text
            accent: Color::Rgb(137, 180, 250),       // #89b4fa Blue
            selection_bg: Color::Rgb(69, 71, 90),    // #45475a Surface1
            selection_fg: Color::Rgb(245, 224, 220), // #f5e0dc Rosewater
            border: Color::Rgb(49, 50, 68),          // #313244 Surface0
            border_focus: Color::Rgb(137, 180, 250), // #89b4fa Blue

            tree_branch: Color::Rgb(88, 91, 112), // #585b70 Surface2
            tree_folder: Color::Rgb(137, 180, 250), // #89b4fa Blue
            tree_folder_expanded: Color::Rgb(116, 199, 236), // #74c7ec Sapphire
            tree_bookmark: Color::Rgb(249, 226, 175), // #f9e2af Yellow

            table_header_bg: Color::Rgb(24, 24, 37), // #181825 Mantle
            table_header_fg: Color::Rgb(186, 194, 222), // #bac2de Subtext1
            table_selected_bg: Color::Rgb(69, 71, 90), // #45475a Surface1
            table_selected_fg: Color::Rgb(245, 245, 245),
            table_row_alt_bg: Color::Rgb(24, 24, 37), // #181825 Mantle

            breadcrumb_bg: Color::Rgb(24, 24, 37),
            breadcrumb_fg: Color::Rgb(166, 173, 200),
            breadcrumb_active_bg: Color::Rgb(49, 50, 68),
            breadcrumb_active_fg: Color::Rgb(245, 224, 220),
            breadcrumb_arrow: Color::Rgb(108, 112, 134),

            status_bg: Color::Rgb(17, 17, 27),      // #11111b Crust
            status_fg: Color::Rgb(166, 173, 200),   // #a6adc8 Subtext0
            status_key: Color::Rgb(166, 227, 161),  // #a6e3a1 Green
            status_info: Color::Rgb(137, 180, 250), // #89b4fa Blue
            status_warn: Color::Rgb(249, 226, 175), // #f9e2af Yellow
            status_error: Color::Rgb(243, 139, 168), // #f38ba8 Red

            file_dir: Color::Rgb(137, 180, 250),     // Blue
            file_exec: Color::Rgb(166, 227, 161),    // Green
            file_symlink: Color::Rgb(148, 226, 213), // Teal
            file_archive: Color::Rgb(250, 179, 135), // Peach
            file_image: Color::Rgb(203, 166, 247),   // Mauve
            file_doc: Color::Rgb(245, 224, 220),     // Rosewater
            file_regular: Color::Rgb(205, 214, 244), // Text
            file_hidden: Color::Rgb(108, 112, 134),  // Overlay0

            filter_match: Color::Rgb(249, 226, 175), // Yellow
        }
    }

    pub fn tokyo_night() -> Self {
        Self {
            name: "Tokyo Night".to_string(),
            bg: Color::Rgb(26, 27, 38),
            fg: Color::Rgb(192, 202, 245),
            accent: Color::Rgb(122, 162, 247),
            selection_bg: Color::Rgb(41, 46, 66),
            selection_fg: Color::Rgb(255, 255, 255),
            border: Color::Rgb(41, 46, 66),
            border_focus: Color::Rgb(122, 162, 247),

            tree_branch: Color::Rgb(86, 95, 137),
            tree_folder: Color::Rgb(122, 162, 247),
            tree_folder_expanded: Color::Rgb(125, 207, 255),
            tree_bookmark: Color::Rgb(224, 175, 104),

            table_header_bg: Color::Rgb(22, 22, 30),
            table_header_fg: Color::Rgb(169, 177, 214),
            table_selected_bg: Color::Rgb(41, 46, 66),
            table_selected_fg: Color::Rgb(255, 255, 255),
            table_row_alt_bg: Color::Rgb(22, 22, 30),

            breadcrumb_bg: Color::Rgb(22, 22, 30),
            breadcrumb_fg: Color::Rgb(169, 177, 214),
            breadcrumb_active_bg: Color::Rgb(41, 46, 66),
            breadcrumb_active_fg: Color::Rgb(255, 255, 255),
            breadcrumb_arrow: Color::Rgb(86, 95, 137),

            status_bg: Color::Rgb(19, 19, 26),
            status_fg: Color::Rgb(169, 177, 214),
            status_key: Color::Rgb(158, 206, 106),
            status_info: Color::Rgb(122, 162, 247),
            status_warn: Color::Rgb(224, 175, 104),
            status_error: Color::Rgb(247, 118, 142),

            file_dir: Color::Rgb(122, 162, 247),
            file_exec: Color::Rgb(158, 206, 106),
            file_symlink: Color::Rgb(115, 218, 202),
            file_archive: Color::Rgb(255, 158, 100),
            file_image: Color::Rgb(187, 154, 247),
            file_doc: Color::Rgb(192, 202, 245),
            file_regular: Color::Rgb(192, 202, 245),
            file_hidden: Color::Rgb(86, 95, 137),

            filter_match: Color::Rgb(224, 175, 104),
        }
    }

    pub fn gruvbox_dark() -> Self {
        Self {
            name: "Gruvbox Dark".to_string(),
            bg: Color::Rgb(40, 40, 40),
            fg: Color::Rgb(235, 219, 178),
            accent: Color::Rgb(131, 165, 152),
            selection_bg: Color::Rgb(60, 56, 54),
            selection_fg: Color::Rgb(251, 241, 199),
            border: Color::Rgb(60, 56, 54),
            border_focus: Color::Rgb(131, 165, 152),

            tree_branch: Color::Rgb(146, 131, 116),
            tree_folder: Color::Rgb(131, 165, 152),
            tree_folder_expanded: Color::Rgb(142, 192, 124),
            tree_bookmark: Color::Rgb(250, 189, 47),

            table_header_bg: Color::Rgb(29, 32, 33),
            table_header_fg: Color::Rgb(213, 196, 161),
            table_selected_bg: Color::Rgb(60, 56, 54),
            table_selected_fg: Color::Rgb(251, 241, 199),
            table_row_alt_bg: Color::Rgb(34, 36, 37),

            breadcrumb_bg: Color::Rgb(29, 32, 33),
            breadcrumb_fg: Color::Rgb(213, 196, 161),
            breadcrumb_active_bg: Color::Rgb(60, 56, 54),
            breadcrumb_active_fg: Color::Rgb(251, 241, 199),
            breadcrumb_arrow: Color::Rgb(146, 131, 116),

            status_bg: Color::Rgb(29, 32, 33),
            status_fg: Color::Rgb(213, 196, 161),
            status_key: Color::Rgb(184, 187, 38),
            status_info: Color::Rgb(131, 165, 152),
            status_warn: Color::Rgb(250, 189, 47),
            status_error: Color::Rgb(251, 73, 52),

            file_dir: Color::Rgb(131, 165, 152),
            file_exec: Color::Rgb(184, 187, 38),
            file_symlink: Color::Rgb(142, 192, 124),
            file_archive: Color::Rgb(254, 128, 25),
            file_image: Color::Rgb(211, 134, 155),
            file_doc: Color::Rgb(235, 219, 178),
            file_regular: Color::Rgb(235, 219, 178),
            file_hidden: Color::Rgb(146, 131, 116),

            filter_match: Color::Rgb(250, 189, 47),
        }
    }

    pub fn nord() -> Self {
        Self {
            name: "Nord".to_string(),
            bg: Color::Rgb(46, 52, 64),
            fg: Color::Rgb(236, 239, 244),
            accent: Color::Rgb(136, 192, 208),
            selection_bg: Color::Rgb(67, 76, 94),
            selection_fg: Color::Rgb(255, 255, 255),
            border: Color::Rgb(59, 66, 82),
            border_focus: Color::Rgb(136, 192, 208),

            tree_branch: Color::Rgb(94, 129, 172),
            tree_folder: Color::Rgb(129, 161, 193),
            tree_folder_expanded: Color::Rgb(136, 192, 208),
            tree_bookmark: Color::Rgb(235, 203, 139),

            table_header_bg: Color::Rgb(36, 41, 51),
            table_header_fg: Color::Rgb(216, 222, 233),
            table_selected_bg: Color::Rgb(67, 76, 94),
            table_selected_fg: Color::Rgb(255, 255, 255),
            table_row_alt_bg: Color::Rgb(40, 45, 56),

            breadcrumb_bg: Color::Rgb(36, 41, 51),
            breadcrumb_fg: Color::Rgb(216, 222, 233),
            breadcrumb_active_bg: Color::Rgb(67, 76, 94),
            breadcrumb_active_fg: Color::Rgb(236, 239, 244),
            breadcrumb_arrow: Color::Rgb(76, 86, 106),

            status_bg: Color::Rgb(36, 41, 51),
            status_fg: Color::Rgb(216, 222, 233),
            status_key: Color::Rgb(163, 190, 140),
            status_info: Color::Rgb(136, 192, 208),
            status_warn: Color::Rgb(235, 203, 139),
            status_error: Color::Rgb(191, 97, 106),

            file_dir: Color::Rgb(129, 161, 193),
            file_exec: Color::Rgb(163, 190, 140),
            file_symlink: Color::Rgb(143, 188, 187),
            file_archive: Color::Rgb(208, 135, 112),
            file_image: Color::Rgb(180, 142, 173),
            file_doc: Color::Rgb(236, 239, 244),
            file_regular: Color::Rgb(236, 239, 244),
            file_hidden: Color::Rgb(76, 86, 106),

            filter_match: Color::Rgb(235, 203, 139),
        }
    }

    pub fn ansi_fallback() -> Self {
        Self {
            name: "ANSI Fallback".to_string(),
            bg: Color::Reset,
            fg: Color::Reset,
            accent: Color::Cyan,
            selection_bg: Color::DarkGray,
            selection_fg: Color::White,
            border: Color::DarkGray,
            border_focus: Color::Cyan,

            tree_branch: Color::DarkGray,
            tree_folder: Color::Blue,
            tree_folder_expanded: Color::Cyan,
            tree_bookmark: Color::Yellow,

            table_header_bg: Color::Black,
            table_header_fg: Color::White,
            table_selected_bg: Color::DarkGray,
            table_selected_fg: Color::White,
            table_row_alt_bg: Color::Reset,

            breadcrumb_bg: Color::Black,
            breadcrumb_fg: Color::Gray,
            breadcrumb_active_bg: Color::DarkGray,
            breadcrumb_active_fg: Color::White,
            breadcrumb_arrow: Color::DarkGray,

            status_bg: Color::Black,
            status_fg: Color::Gray,
            status_key: Color::Green,
            status_info: Color::Cyan,
            status_warn: Color::Yellow,
            status_error: Color::Red,

            file_dir: Color::Blue,
            file_exec: Color::Green,
            file_symlink: Color::Cyan,
            file_archive: Color::Red,
            file_image: Color::Magenta,
            file_doc: Color::White,
            file_regular: Color::Reset,
            file_hidden: Color::DarkGray,

            filter_match: Color::Yellow,
        }
    }

    pub fn catppuccin_latte() -> Self {
        Self {
            name: "Catppuccin Latte".to_string(),
            bg: Color::Rgb(239, 241, 245),           // #eff1f5 Base
            fg: Color::Rgb(76, 79, 105),             // #4c4f69 Text
            accent: Color::Rgb(30, 102, 245),        // #1e66f5 Blue
            selection_bg: Color::Rgb(204, 208, 218), // #ccd0da Surface1
            selection_fg: Color::Rgb(92, 95, 119),   // #5c5f77 Subtext0
            border: Color::Rgb(188, 192, 204),       // #bcc0cc Surface0
            border_focus: Color::Rgb(30, 102, 245),  // #1e66f5 Blue

            tree_branch: Color::Rgb(156, 160, 176), // #9ca0b0 Overlay0
            tree_folder: Color::Rgb(30, 102, 245),  // #1e66f5 Blue
            tree_folder_expanded: Color::Rgb(4, 165, 229), // #04a5e5 Sky
            tree_bookmark: Color::Rgb(223, 142, 29), // #df8e1d Yellow

            table_header_bg: Color::Rgb(230, 233, 239), // #e6e9ef Mantle
            table_header_fg: Color::Rgb(92, 95, 119),   // #5c5f77 Subtext0
            table_selected_bg: Color::Rgb(204, 208, 218), // #ccd0da Surface1
            table_selected_fg: Color::Rgb(40, 44, 52),  // #2c2c34 text dark
            table_row_alt_bg: Color::Rgb(230, 233, 239), // #e6e9ef Mantle

            breadcrumb_bg: Color::Rgb(230, 233, 239),
            breadcrumb_fg: Color::Rgb(92, 95, 119),
            breadcrumb_active_bg: Color::Rgb(204, 208, 218),
            breadcrumb_active_fg: Color::Rgb(76, 79, 105),
            breadcrumb_arrow: Color::Rgb(156, 160, 176),

            status_bg: Color::Rgb(230, 233, 239), // #e6e9ef Mantle
            status_fg: Color::Rgb(92, 95, 119),   // #5c5f77 Subtext0
            status_key: Color::Rgb(64, 160, 67),  // #40a02b Green
            status_info: Color::Rgb(30, 102, 245), // #1e66f5 Blue
            status_warn: Color::Rgb(223, 142, 29), // #df8e1d Yellow
            status_error: Color::Rgb(210, 15, 57), // #d20f39 Red

            file_dir: Color::Rgb(30, 102, 245),     // Blue
            file_exec: Color::Rgb(64, 160, 67),     // Green
            file_symlink: Color::Rgb(23, 146, 153), // Teal
            file_archive: Color::Rgb(254, 100, 11), // Peach
            file_image: Color::Rgb(136, 57, 239),   // Mauve
            file_doc: Color::Rgb(220, 21, 117),     // Pink (Rosewater-ish)
            file_regular: Color::Rgb(76, 79, 105),  // Text
            file_hidden: Color::Rgb(156, 160, 176), // Overlay0

            filter_match: Color::Rgb(223, 142, 29), // Yellow
        }
    }

    pub fn solarized_dark() -> Self {
        Self {
            name: "Solarized Dark".to_string(),
            bg: Color::Rgb(0, 43, 54),               // base03
            fg: Color::Rgb(131, 148, 150),           // base0
            accent: Color::Rgb(38, 139, 210),        // blue
            selection_bg: Color::Rgb(7, 54, 66),     // base02
            selection_fg: Color::Rgb(147, 161, 161), // base1
            border: Color::Rgb(7, 54, 66),           // base02
            border_focus: Color::Rgb(38, 139, 210),  // blue

            tree_branch: Color::Rgb(88, 110, 117), // base01
            tree_folder: Color::Rgb(38, 139, 210), // blue
            tree_folder_expanded: Color::Rgb(42, 161, 152), // cyan
            tree_bookmark: Color::Rgb(181, 137, 0), // yellow

            table_header_bg: Color::Rgb(0, 32, 41), // darker base03
            table_header_fg: Color::Rgb(147, 161, 161), // base1
            table_selected_bg: Color::Rgb(7, 54, 66), // base02
            table_selected_fg: Color::Rgb(253, 246, 227), // base3
            table_row_alt_bg: Color::Rgb(0, 32, 41),

            breadcrumb_bg: Color::Rgb(0, 32, 41),
            breadcrumb_fg: Color::Rgb(147, 161, 161),
            breadcrumb_active_bg: Color::Rgb(7, 54, 66),
            breadcrumb_active_fg: Color::Rgb(253, 246, 227),
            breadcrumb_arrow: Color::Rgb(88, 110, 117),

            status_bg: Color::Rgb(0, 32, 41),
            status_fg: Color::Rgb(131, 148, 150),
            status_key: Color::Rgb(133, 153, 0),   // green
            status_info: Color::Rgb(38, 139, 210), // blue
            status_warn: Color::Rgb(181, 137, 0),  // yellow
            status_error: Color::Rgb(220, 50, 47), // red

            file_dir: Color::Rgb(38, 139, 210),      // blue
            file_exec: Color::Rgb(133, 153, 0),      // green
            file_symlink: Color::Rgb(42, 161, 152),  // cyan
            file_archive: Color::Rgb(203, 75, 22),   // orange
            file_image: Color::Rgb(211, 54, 130),    // magenta
            file_doc: Color::Rgb(147, 161, 161),     // base1
            file_regular: Color::Rgb(131, 148, 150), // base0
            file_hidden: Color::Rgb(88, 110, 117),   // base01

            filter_match: Color::Rgb(181, 137, 0), // yellow
        }
    }

    pub fn solarized_light() -> Self {
        Self {
            name: "Solarized Light".to_string(),
            bg: Color::Rgb(253, 246, 227),           // base3
            fg: Color::Rgb(101, 123, 131),           // base00
            accent: Color::Rgb(38, 139, 210),        // blue
            selection_bg: Color::Rgb(238, 232, 213), // base2
            selection_fg: Color::Rgb(88, 110, 117),  // base01
            border: Color::Rgb(238, 232, 213),       // base2
            border_focus: Color::Rgb(38, 139, 210),  // blue

            tree_branch: Color::Rgb(147, 161, 161), // base1
            tree_folder: Color::Rgb(38, 139, 210),  // blue
            tree_folder_expanded: Color::Rgb(42, 161, 152), // cyan
            tree_bookmark: Color::Rgb(181, 137, 0), // yellow

            table_header_bg: Color::Rgb(238, 232, 213), // base2
            table_header_fg: Color::Rgb(88, 110, 117),  // base01
            table_selected_bg: Color::Rgb(238, 232, 213), // base2
            table_selected_fg: Color::Rgb(0, 43, 54),   // base03
            table_row_alt_bg: Color::Rgb(253, 246, 227),

            breadcrumb_bg: Color::Rgb(238, 232, 213),
            breadcrumb_fg: Color::Rgb(88, 110, 117),
            breadcrumb_active_bg: Color::Rgb(238, 232, 213),
            breadcrumb_active_fg: Color::Rgb(0, 43, 54),
            breadcrumb_arrow: Color::Rgb(147, 161, 161),

            status_bg: Color::Rgb(238, 232, 213),
            status_fg: Color::Rgb(88, 110, 117),
            status_key: Color::Rgb(133, 153, 0),   // green
            status_info: Color::Rgb(38, 139, 210), // blue
            status_warn: Color::Rgb(181, 137, 0),  // yellow
            status_error: Color::Rgb(220, 50, 47), // red

            file_dir: Color::Rgb(38, 139, 210),      // blue
            file_exec: Color::Rgb(133, 153, 0),      // green
            file_symlink: Color::Rgb(42, 161, 152),  // cyan
            file_archive: Color::Rgb(203, 75, 22),   // orange
            file_image: Color::Rgb(211, 54, 130),    // magenta
            file_doc: Color::Rgb(88, 110, 117),      // base01
            file_regular: Color::Rgb(101, 123, 131), // base00
            file_hidden: Color::Rgb(147, 161, 161),  // base1

            filter_match: Color::Rgb(181, 137, 0), // yellow
        }
    }

    /// Maximum-contrast palette for accessibility / low-vision users.
    pub fn high_contrast() -> Self {
        Self {
            name: "High Contrast".to_string(),
            bg: Color::Rgb(0, 0, 0),
            fg: Color::Rgb(255, 255, 255),
            accent: Color::Rgb(255, 255, 0), // pure yellow
            selection_bg: Color::Rgb(255, 255, 255),
            selection_fg: Color::Rgb(0, 0, 0),
            border: Color::Rgb(128, 128, 128),
            border_focus: Color::Rgb(255, 255, 0), // yellow

            tree_branch: Color::Rgb(192, 192, 192),
            tree_folder: Color::Rgb(112, 219, 255), // bright cyan
            tree_folder_expanded: Color::Rgb(255, 255, 0),
            tree_bookmark: Color::Rgb(255, 200, 0),

            table_header_bg: Color::Rgb(32, 32, 32),
            table_header_fg: Color::Rgb(255, 255, 255),
            table_selected_bg: Color::Rgb(255, 255, 255),
            table_selected_fg: Color::Rgb(0, 0, 0),
            table_row_alt_bg: Color::Rgb(16, 16, 16),

            breadcrumb_bg: Color::Rgb(32, 32, 32),
            breadcrumb_fg: Color::Rgb(255, 255, 255),
            breadcrumb_active_bg: Color::Rgb(255, 255, 0),
            breadcrumb_active_fg: Color::Rgb(0, 0, 0),
            breadcrumb_arrow: Color::Rgb(192, 192, 192),

            status_bg: Color::Rgb(0, 0, 0),
            status_fg: Color::Rgb(255, 255, 255),
            status_key: Color::Rgb(0, 255, 0),
            status_info: Color::Rgb(113, 219, 255),
            status_warn: Color::Rgb(255, 255, 0),
            status_error: Color::Rgb(255, 96, 96),

            file_dir: Color::Rgb(112, 219, 255),
            file_exec: Color::Rgb(0, 255, 0),
            file_symlink: Color::Rgb(0, 255, 255),
            file_archive: Color::Rgb(255, 165, 0),
            file_image: Color::Rgb(255, 160, 255),
            file_doc: Color::Rgb(255, 255, 255),
            file_regular: Color::Rgb(255, 255, 255),
            file_hidden: Color::Rgb(160, 160, 160),

            filter_match: Color::Rgb(255, 255, 0),
        }
    }

    pub fn from_name(name: &str) -> Self {
        Self::try_from_name(name).unwrap_or_else(Self::catppuccin_mocha)
    }

    /// Strict palette lookup: `None` for unknown names (callers decide
    /// whether to fall back or keep the last valid palette).
    pub fn try_from_name(name: &str) -> Option<Self> {
        let normalized = name.trim().to_lowercase().replace([' ', '_'], "-");
        match normalized.as_str() {
            "catppuccin-mocha" | "catppuccin" | "mocha" => Some(Self::catppuccin_mocha()),
            "catppuccin-latte" | "latte" => Some(Self::catppuccin_latte()),
            "tokyo-night" | "tokyonight" => Some(Self::tokyo_night()),
            "gruvbox" | "gruvbox-dark" => Some(Self::gruvbox_dark()),
            "nord" => Some(Self::nord()),
            "solarized-dark" | "solarized" => Some(Self::solarized_dark()),
            "solarized-light" => Some(Self::solarized_light()),
            "high-contrast" | "highcontrast" => Some(Self::high_contrast()),
            "ansi" => Some(Self::ansi_fallback()),
            _ => None,
        }
    }

    /// Omarchy theme → palette. Supports both layouts:
    /// v4 (state dir with a real `colors.toml`) and legacy (`current/theme`
    /// containing a built-in palette name). Unknown/malformed contents yield
    /// `None` so callers can keep the last valid palette.
    pub fn try_load_omarchy() -> Option<Self> {
        let home = std::env::var("HOME").ok()?;

        // Omarchy v4 (Aether): ~/.local/state/omarchy/current/theme{.name,/colors.toml}
        let state_dir = Path::new(&home).join(".local/state/omarchy/current");
        let colors_path = state_dir.join("theme/colors.toml");
        if colors_path.is_file() {
            let name = std::fs::read_to_string(state_dir.join("theme.name"))
                .map(|s| s.trim().to_string())
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "omarchy".to_string());
            if let Ok(content) = std::fs::read_to_string(&colors_path) {
                if let Some(theme) = parse_omarchy_v4(&name, &content) {
                    return Some(theme);
                }
                tracing::warn!("Failed to parse Omarchy v4 colors.toml; trying legacy layout");
            }
        }

        // Legacy: ~/.config/omarchy/current/theme holds a palette name
        let omarchy_theme_path = Path::new(&home).join(".config/omarchy/current/theme");
        if !omarchy_theme_path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&omarchy_theme_path).ok()?;
        let theme_name = content.trim();
        if theme_name.is_empty() {
            return None;
        }

        Self::try_from_name(theme_name)
    }

    // ---- Environment Adaptation ----

    /// `$NO_COLOR` spec: present and non-empty disables custom coloring.
    pub fn no_color_enabled() -> bool {
        match std::env::var_os("NO_COLOR") {
            Some(v) => !v.is_empty(),
            None => false,
        }
    }

    /// Truecolor detection from terminal environment variables.
    pub fn supports_truecolor(colorterm: Option<&str>, term: Option<&str>) -> bool {
        if let Some(ct) = colorterm {
            let ct = ct.to_lowercase();
            if ct.contains("truecolor") || ct.contains("24bit") {
                return true;
            }
        }
        match term {
            // Direct-color and modern terminals that handle RGB escapes well
            Some(t) => {
                let t = t.to_lowercase();
                t.contains("direct") || t.contains("kitty") || t.contains("wezterm")
            }
            None => false,
        }
    }

    fn truecolor_detected() -> bool {
        let colorterm = std::env::var("COLORTERM").ok();
        let term = std::env::var("TERM").ok();
        Self::supports_truecolor(colorterm.as_deref(), term.as_deref())
    }

    /// Applies environment-based degradation to any palette: `$NO_COLOR`
    /// strips all color; non-truecolor terminals get nearest-ANSI mapping.
    pub fn effective_with_terminal(base: Theme, no_color: bool, truecolor: bool) -> Theme {
        if no_color {
            base.stripped()
        } else if !truecolor {
            base.degraded_to_ansi()
        } else {
            base
        }
    }

    pub fn effective(base: Theme, no_color: bool) -> Theme {
        Self::effective_with_terminal(base, no_color, Self::truecolor_detected())
    }

    /// Removes all custom coloring (every field becomes `Color::Reset`).
    pub fn stripped(&self) -> Theme {
        let mut t = self.clone();
        t.name = format!("{} (no color)", self.name);
        for color in t.color_fields_mut() {
            *color = Color::Reset;
        }
        t
    }

    /// Maps every RGB color to the nearest of the 16 ANSI colors; named /
    /// Reset colors pass through unchanged. Used on 256-color or worse
    /// terminals where raw RGB escapes render poorly.
    pub fn degraded_to_ansi(&self) -> Theme {
        let mut t = self.clone();
        t.name = format!("{} (ansi)", self.name);
        for color in t.color_fields_mut() {
            if let Color::Rgb(r, g, b) = *color {
                *color = nearest_ansi(r, g, b);
            }
        }
        t
    }

    // ---- Custom TOML Theme Overrides ----
    /// Applies hex-string overrides keyed by theme field name
    /// (`{"bg": "#1e1e2e", "accent": "#89b4fa"}`). Unknown keys or invalid
    /// values abort the whole override set atomically (nothing is applied).
    pub fn apply_overrides(
        &mut self,
        overrides: &HashMap<String, String>,
    ) -> Result<(), Vec<String>> {
        let names = Self::color_field_names();
        let mut slots: Vec<Option<Color>> = vec![None; names.len()];
        let mut errors: Vec<String> = Vec::new();

        // Pass 1: validate everything before touching any field
        for (key, value) in overrides {
            match names.iter().position(|n| n.eq_ignore_ascii_case(key)) {
                None => errors.push(format!("unknown theme key: {}", key)),
                Some(idx) => match parse_hex_color(value) {
                    Some(c) => slots[idx] = Some(c),
                    None => errors.push(format!("invalid hex color for {}: {:?}", key, value)),
                },
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }

        // Pass 2: commit
        for (field, slot) in self.color_fields_mut().into_iter().zip(slots) {
            if let Some(c) = slot {
                *field = c;
            }
        }
        Ok(())
    }

    fn color_fields_mut(&mut self) -> Vec<&mut Color> {
        vec![
            &mut self.bg,
            &mut self.fg,
            &mut self.accent,
            &mut self.selection_bg,
            &mut self.selection_fg,
            &mut self.border,
            &mut self.border_focus,
            &mut self.tree_branch,
            &mut self.tree_folder,
            &mut self.tree_folder_expanded,
            &mut self.tree_bookmark,
            &mut self.table_header_bg,
            &mut self.table_header_fg,
            &mut self.table_selected_bg,
            &mut self.table_selected_fg,
            &mut self.table_row_alt_bg,
            &mut self.breadcrumb_bg,
            &mut self.breadcrumb_fg,
            &mut self.breadcrumb_active_bg,
            &mut self.breadcrumb_active_fg,
            &mut self.breadcrumb_arrow,
            &mut self.status_bg,
            &mut self.status_fg,
            &mut self.status_key,
            &mut self.status_info,
            &mut self.status_warn,
            &mut self.status_error,
            &mut self.file_dir,
            &mut self.file_exec,
            &mut self.file_symlink,
            &mut self.file_archive,
            &mut self.file_image,
            &mut self.file_doc,
            &mut self.file_regular,
            &mut self.file_hidden,
            &mut self.filter_match,
        ]
    }

    /// Field names in the same order as [`Theme::color_fields_mut`].
    pub fn color_field_names() -> &'static [&'static str] {
        &[
            "bg",
            "fg",
            "accent",
            "selection_bg",
            "selection_fg",
            "border",
            "border_focus",
            "tree_branch",
            "tree_folder",
            "tree_folder_expanded",
            "tree_bookmark",
            "table_header_bg",
            "table_header_fg",
            "table_selected_bg",
            "table_selected_fg",
            "table_row_alt_bg",
            "breadcrumb_bg",
            "breadcrumb_fg",
            "breadcrumb_active_bg",
            "breadcrumb_active_fg",
            "breadcrumb_arrow",
            "status_bg",
            "status_fg",
            "status_key",
            "status_info",
            "status_warn",
            "status_error",
            "file_dir",
            "file_exec",
            "file_symlink",
            "file_archive",
            "file_image",
            "file_doc",
            "file_regular",
            "file_hidden",
            "filter_match",
        ]
    }

    // Style helper methods
    pub fn style_base(&self) -> Style {
        Style::default().bg(self.bg).fg(self.fg)
    }

    pub fn style_border(&self, focused: bool) -> Style {
        if focused {
            Style::default().fg(self.border_focus)
        } else {
            Style::default().fg(self.border)
        }
    }

    pub fn style_header(&self) -> Style {
        Style::default()
            .bg(self.table_header_bg)
            .fg(self.table_header_fg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn style_selected(&self) -> Style {
        Style::default()
            .bg(self.table_selected_bg)
            .fg(self.table_selected_fg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn style_status(&self) -> Style {
        Style::default().bg(self.status_bg).fg(self.status_fg)
    }

    pub fn style_breadcrumb(&self, is_active: bool) -> Style {
        if is_active {
            Style::default()
                .bg(self.breadcrumb_active_bg)
                .fg(self.breadcrumb_active_fg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .bg(self.breadcrumb_bg)
                .fg(self.breadcrumb_fg)
        }
    }
}

/// Parses `#rgb`, `#rrggbb`, `rgb`, and `rrggbb` forms into `Color::Rgb`.
pub fn parse_hex_color(s: &str) -> Option<Color> {
    let hex = s.trim().trim_start_matches('#');
    match hex.len() {
        3 => {
            let vals: Vec<u8> = hex
                .chars()
                .map(|c| u8::from_str_radix(&c.to_string(), 16).ok())
                .collect::<Option<_>>()?;
            Some(Color::Rgb(vals[0] * 17, vals[1] * 17, vals[2] * 17))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

/// The 16 standard ANSI colors with their canonical xterm RGB values.
const ANSI16: &[(Color, [u8; 3])] = &[
    (Color::Black, [0, 0, 0]),
    (Color::Red, [205, 49, 49]),
    (Color::Green, [13, 188, 121]),
    (Color::Yellow, [229, 229, 16]),
    (Color::Blue, [36, 114, 200]),
    (Color::Magenta, [188, 63, 188]),
    (Color::Cyan, [17, 168, 205]),
    (Color::Gray, [229, 229, 229]),
    (Color::DarkGray, [102, 102, 102]),
    (Color::LightRed, [241, 76, 76]),
    (Color::LightGreen, [35, 209, 139]),
    (Color::LightYellow, [245, 245, 67]),
    (Color::LightBlue, [59, 142, 234]),
    (Color::LightMagenta, [214, 112, 214]),
    (Color::LightCyan, [41, 184, 219]),
    (Color::White, [255, 255, 255]),
];

/// Nearest-ANSI mapping by squared RGB distance.
/// Adapts an arbitrary RGB color (e.g. from syntax highlighting) to the
/// terminal's capabilities: `$NO_COLOR` strips it entirely, non-truecolor
/// terminals get the nearest-of-16 ANSI mapping, truecolor passes through.
/// Applied identically at load time and on theme hot-reload.
pub fn adapt_syntax_color(color: Color) -> Color {
    if Theme::no_color_enabled() {
        return Color::Reset;
    }
    match color {
        Color::Rgb(r, g, b) if !Theme::truecolor_detected() => nearest_ansi(r, g, b),
        other => other,
    }
}

fn nearest_ansi(r: u8, g: u8, b: u8) -> Color {
    let (r, g, b) = (r as i32, g as i32, b as i32);
    ANSI16
        .iter()
        .min_by_key(|(_, c)| {
            let (cr, cg, cb) = (c[0] as i32, c[1] as i32, c[2] as i32);
            (r - cr).pow(2) + (g - cg).pow(2) + (b - cb).pow(2)
        })
        .map(|(color, _)| *color)
        .unwrap_or(Color::Reset)
}

/// Parses an Omarchy v4 / Aether `colors.toml` into a palette. Every
/// recognized key overrides the Catppuccin Mocha base; missing keys keep the
/// base values, so partial files still produce a usable theme.
pub fn parse_omarchy_v4(name: &str, content: &str) -> Option<Theme> {
    let value: toml::Value = toml::from_str(content).ok()?;

    // Colors may live at the top level or under [colors]
    let table = value
        .as_table()
        .cloned()
        .or_else(|| value.get("colors").and_then(|c| c.as_table()).cloned())?;

    let hex = |key: &str| -> Option<Color> {
        table
            .get(key)
            .and_then(|v| v.as_str())
            .and_then(parse_hex_color)
    };
    // First present key wins (handles aether vs aetheria key styles)
    let hex_any = |keys: &[&str]| -> Option<Color> { keys.iter().find_map(|k| hex(k)) };

    let background = hex_any(&["background", "background_color", "color0_bg"]);
    let foreground = hex_any(&["foreground", "foreground_color"]);

    let mut t = Theme::catppuccin_mocha();
    t.name = format!("Omarchy: {}", name);

    // Require at least one recognizable color, else treat the file as
    // unrelated/malformed and let the caller keep its current palette
    let mut mapped = 0usize;
    macro_rules! set {
        ($field:expr, $color:expr) => {
            if let Some(c) = $color {
                $field = c;
                mapped += 1;
            }
        };
    }

    set!(t.bg, background);
    set!(t.fg, foreground);
    if let Some(c) = hex("accent") {
        t.accent = c;
        t.border_focus = c;
        mapped += 1;
    }
    set!(
        t.selection_bg,
        hex_any(&["selection_background", "selection"])
    );
    if let Some(c) = hex_any(&["selection_background", "selection"]) {
        t.table_selected_bg = c;
        t.breadcrumb_active_bg = c;
    }
    set!(t.selection_fg, hex("selection_foreground"));
    if let Some(c) = hex("selection_foreground") {
        t.table_selected_fg = c;
    }
    if let Some(c) = hex_any(&["muted", "comment"]) {
        t.border = c;
        t.tree_branch = c;
        t.breadcrumb_arrow = c;
        t.file_hidden = c;
        mapped += 1;
    }
    set!(
        t.status_bg,
        hex_any(&["darker_background", "dark_background"])
    );
    set!(t.table_header_bg, hex("dark_background"));
    if let Some(c) = hex("dark_background") {
        t.table_row_alt_bg = c;
        t.breadcrumb_bg = c;
    }
    set!(t.table_header_fg, hex_any(&["dark_foreground"]));
    if let Some(c) = hex("dark_foreground") {
        t.breadcrumb_fg = c;
        t.status_fg = c;
    }

    set!(t.tree_folder, hex("blue"));
    if let Some(c) = hex("blue") {
        t.file_dir = c;
        t.status_info = c;
    }
    set!(t.tree_folder_expanded, hex("cyan"));
    if let Some(c) = hex("cyan") {
        t.file_symlink = c;
    }
    set!(t.file_exec, hex("green"));
    if let Some(c) = hex("green") {
        t.status_key = c;
    }
    set!(t.tree_bookmark, hex("yellow"));
    if let Some(c) = hex("yellow") {
        t.filter_match = c;
        t.status_warn = c;
    }
    set!(t.status_error, hex("red"));
    set!(t.file_archive, hex("orange"));
    set!(t.file_image, hex("magenta"));
    set!(t.file_doc, hex_any(&["brown", "pink"]));

    if mapped == 0 {
        return None;
    }

    Some(t)
}
