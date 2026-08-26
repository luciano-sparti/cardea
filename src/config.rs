use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

/// Current configuration schema version. Bump when making breaking changes;
/// `Config::load` migrates older files forward on save.
pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortColumn {
    #[default]
    Name,
    Size,
    Modified,
    Extension,
    Permissions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewDock {
    #[default]
    Side,
    Bottom,
}

impl SortDirection {
    pub fn toggle(&self) -> Self {
        match self {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            SortDirection::Ascending => "▲",
            SortDirection::Descending => "▼",
        }
    }
}

/// Context for expanding user-action argument placeholders.
#[derive(Debug, Clone, Default)]
pub struct ActionContext {
    /// The focused entry's full path (also used by `{path}`)
    pub file: Option<PathBuf>,
    /// Multi-selection snapshot; `{selected}` prefers this over `{file}`
    pub selected: Vec<PathBuf>,
    /// Current working directory (`{dir}`)
    pub dir: PathBuf,
}

impl ActionContext {
    pub fn new(dir: PathBuf, file: Option<PathBuf>, selected: Vec<PathBuf>) -> Self {
        Self {
            file,
            selected,
            dir,
        }
    }
}

/// A user-defined launcher action. `command` + `args` are executed as a raw
/// argument vector — never through a shell — so paths containing spaces or
/// metacharacters are always safe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserAction {
    /// Display name (shown in menus and the help sheet)
    pub name: String,
    /// Optional keybinding, e.g. "ctrl+e", "alt+shift+t" (empty = menu only)
    #[serde(default)]
    pub key: Option<String>,
    /// Executable name or path (argv[0])
    pub command: String,
    /// Argument vector; supports `{file}`, `{selected}`, `{path}`, `{dir}`
    #[serde(default)]
    pub args: Vec<String>,
}

impl UserAction {
    /// Expands placeholders into a concrete argv. Unrecognized placeholders
    /// are left untouched. `{file}`/`{path}` → focused entry, `{selected}` →
    /// first selected path (falls back to the focused entry), `{dir}` → cwd.
    pub fn expand_args(&self, ctx: &ActionContext) -> Vec<String> {
        let file = ctx.file.as_deref().map(path_to_string);
        let selected = ctx
            .selected
            .first()
            .map(|p| path_to_string(p))
            .or_else(|| file.clone());
        let dir = path_to_string(&ctx.dir);

        self.args
            .iter()
            .map(|arg| {
                arg.replace("{file}", file.as_deref().unwrap_or(""))
                    .replace("{path}", file.as_deref().unwrap_or(""))
                    .replace("{selected}", selected.as_deref().unwrap_or(""))
                    .replace("{dir}", &dir)
            })
            .collect()
    }

    /// Parses `key` ("ctrl+shift+n", "alt+f4", "f5") into modifiers + code.
    /// Returns `None` when unset or unparseable.
    pub fn parse_key(&self) -> Option<(crossterm::event::KeyModifiers, crossterm::event::KeyCode)> {
        parse_key_str(self.key.as_deref()?)
    }
}

fn path_to_string(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

/// Minimal keybinding-string parser: `+`-separated modifiers plus a single
/// key (single character, "f1".."f12", "enter", "esc", "tab", "space",
/// "backspace", "delete", "home", "end", "left"/"right"/"up"/"down").
pub fn parse_key_str(
    s: &str,
) -> Option<(crossterm::event::KeyModifiers, crossterm::event::KeyCode)> {
    use crossterm::event::{KeyCode, KeyModifiers};

    let mut mods = KeyModifiers::NONE;
    let mut key: Option<KeyCode> = None;

    for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        let lower = part.to_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => mods |= KeyModifiers::CONTROL,
            "alt" | "opt" | "option" => mods |= KeyModifiers::ALT,
            "shift" => mods |= KeyModifiers::SHIFT,
            "super" | "meta" | "cmd" => mods |= KeyModifiers::SUPER,
            _ if key.is_some() => return None, // more than one key given
            _ => {
                key = Some(match lower.as_str() {
                    "enter" | "return" => KeyCode::Enter,
                    "esc" | "escape" => KeyCode::Esc,
                    "tab" => KeyCode::Tab,
                    "backtab" => KeyCode::BackTab,
                    "space" => KeyCode::Char(' '),
                    "backspace" => KeyCode::Backspace,
                    "delete" | "del" => KeyCode::Delete,
                    "insert" | "ins" => KeyCode::Insert,
                    "home" => KeyCode::Home,
                    "end" => KeyCode::End,
                    "up" => KeyCode::Up,
                    "down" => KeyCode::Down,
                    "left" => KeyCode::Left,
                    "right" => KeyCode::Right,
                    f if f.len() >= 2
                        && f.starts_with('f')
                        && f[1..].chars().all(|c| c.is_ascii_digit()) =>
                    {
                        let n: u8 = f[1..].parse().ok()?;
                        if !(1..=12).contains(&n) {
                            return None;
                        }
                        KeyCode::F(n)
                    }
                    c if c.chars().count() == 1 => KeyCode::Char(c.chars().next()?),
                    _ => return None,
                });
            }
        }
    }

    // Modifiers alone don't make a binding
    key.map(|k| (mods, k))
}

/// User-defined palette overrides from the `[custom_theme]` config section.
/// `base` selects the starting built-in palette (default: `general.theme`);
/// color keys are theme field names with hex values.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomTheme {
    /// Base palette name ("catppuccin-mocha", "nord", …); falls back to
    /// `general.theme` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Color overrides keyed by theme field name ("bg": "#1e1e2e")
    #[serde(flatten)]
    pub colors: std::collections::HashMap<String, String>,
}

/// A keybinding remap: when `from` is pressed, it is treated as if `to` was
/// pressed instead. Both are parsed using the same syntax as `[[actions]]` keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRemap {
    /// The key combination to intercept (e.g., "ctrl+z")
    pub from: String,
    /// The key combination to substitute (e.g., "ctrl+v")
    pub to: String,
}

impl KeyRemap {
    /// Pre-parses both sides into (from_mods, from_code, to_mods, to_code).
    /// Returns `None` if either side fails to parse.
    pub fn parsed(
        &self,
    ) -> Option<(
        crossterm::event::KeyModifiers,
        crossterm::event::KeyCode,
        crossterm::event::KeyModifiers,
        crossterm::event::KeyCode,
    )> {
        let (from_mods, from_code) = parse_key_str(&self.from)?;
        let (to_mods, to_code) = parse_key_str(&self.to)?;
        Some((from_mods, from_code, to_mods, to_code))
    }
}

impl CustomTheme {
    /// Resolves this definition into a concrete palette. Invalid entries
    /// produce errors; on error the base palette is returned untouched
    /// (callers decide whether to warn).
    pub fn resolve(&self, fallback_base: &str) -> Result<crate::theme::Theme, Vec<String>> {
        let base_name = self.base.as_deref().unwrap_or(fallback_base);
        let mut theme = crate::theme::Theme::try_from_name(base_name)
            .or_else(|| crate::theme::Theme::try_from_name(fallback_base))
            .unwrap_or_else(crate::theme::Theme::catppuccin_mocha);
        theme.apply_overrides(&self.colors)?;
        Ok(theme)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default)]
    pub dirs_first: bool,
    #[serde(default)]
    pub natural_sort: bool,
    #[serde(default)]
    pub mouse_enabled: bool,
    /// Icon rendering style: "nerd" (Nerd Fonts v3), "unicode", or "ascii"
    #[serde(default = "default_icon_style")]
    pub icon_style: String,
    /// Color filenames from `$LS_COLORS` (standard Unix colored-names scheme)
    #[serde(default = "default_ls_colors_enabled")]
    pub ls_colors_enabled: bool,
    #[serde(default)]
    pub default_sort_column: SortColumn,
    #[serde(default)]
    pub default_sort_direction: SortDirection,
}

fn default_icon_style() -> String {
    "nerd".to_string()
}

fn default_ls_colors_enabled() -> bool {
    true
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            theme: "catppuccin-mocha".to_string(),
            show_hidden: false,
            dirs_first: true,
            natural_sort: true,
            mouse_enabled: true,
            icon_style: "nerd".to_string(),
            ls_colors_enabled: true,
            default_sort_column: SortColumn::Name,
            default_sort_direction: SortDirection::Ascending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    #[serde(default)]
    pub show_sidebar: bool,
    #[serde(default)]
    pub sidebar_width_percent: u16,
    #[serde(default)]
    pub show_preview: bool,
    #[serde(default)]
    pub preview_width_percent: u16,
    /// Preview dock position: "side" (right column) or "bottom" (row below table)
    #[serde(default)]
    pub preview_dock: PreviewDock,
    /// Preview height percentage when docked at the bottom (default: 30%)
    #[serde(default = "default_preview_height_percent")]
    pub preview_height_percent: u16,
    #[serde(default)]
    pub show_status_bar: bool,
    #[serde(default)]
    pub name_column_min_width: u16,
}

fn default_preview_height_percent() -> u16 {
    30
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            show_sidebar: true,
            sidebar_width_percent: 22,
            show_preview: false,
            preview_width_percent: 35,
            preview_dock: PreviewDock::Side,
            preview_height_percent: 30,
            show_status_bar: true,
            name_column_min_width: 24,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Schema version; absent on legacy (pre-versioning) files and treated
    /// as v1 content. Rewritten with the current version on save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub layout: LayoutConfig,
    /// User-defined launcher actions ([[actions]] tables)
    #[serde(default)]
    pub actions: Vec<UserAction>,
    /// User palette overrides ([custom_theme] section)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_theme: Option<CustomTheme>,
    /// Keybinding remaps ([[remap]] tables): intercept `from` and treat as `to`
    #[serde(default)]
    pub remap: Vec<KeyRemap>,
}

impl Config {
    pub fn config_path() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        Some(Path::new(&home).join(".config/fenestra/config.toml"))
    }

    pub fn load() -> Self {
        match Self::config_path() {
            Some(p) => Self::load_from(&p),
            None => Self::default(),
        }
    }

    /// Loads from an explicit path; missing files fall back to defaults,
    /// malformed files are reported and replaced by defaults (never fatal).
    pub fn load_from(path: &Path) -> Self {
        if !path.exists() {
            let default_config = Self::default();
            let _ = default_config.save_to(path);
            return default_config;
        }

        match std::fs::read_to_string(path) {
            Ok(content) => match toml::from_str::<Config>(&content) {
                Ok(mut cfg) => {
                    cfg.migrate(path);
                    cfg
                }
                Err(e) => {
                    error!("Error parsing config file at {:?}: {}", path, e);
                    Self::default()
                }
            },
            Err(e) => {
                error!("Error reading config file at {:?}: {}", path, e);
                Self::default()
            }
        }
    }

    /// Applies forward-migrations and logs anomalies. Currently: legacy files
    /// without a version stamp adopt the current version silently; files from
    /// the future are loaded best-effort with a warning.
    fn migrate(&mut self, path: &Path) {
        match self.version {
            None => {
                self.version = Some(CONFIG_VERSION);
                info!(
                    "Migrated legacy config {:?} to version {}",
                    path, CONFIG_VERSION
                );
            }
            Some(v) if v > CONFIG_VERSION => {
                warn!(
                    "Config {:?} declares version {} (supported: {}); loading best-effort",
                    path, v, CONFIG_VERSION
                );
            }
            Some(_) => {}
        }
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        match Self::config_path() {
            Some(p) => self.save_to(&p),
            None => Ok(()),
        }
    }

    pub fn save_to(&self, path: &Path) -> Result<(), std::io::Error> {
        // Always persist under the current schema version
        let stamped = Config {
            version: Some(CONFIG_VERSION),
            general: self.general.clone(),
            layout: self.layout.clone(),
            actions: self.actions.clone(),
            custom_theme: self.custom_theme.clone(),
            remap: self.remap.clone(),
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let toml_str = toml::to_string_pretty(&stamped)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, toml_str)?;
        info!("Saved configuration to {:?}", path);

        Ok(())
    }
}

impl Config {
    /// Resolves the configured palette: `custom_theme` (when present)
    /// layered over the base named theme. Falls back to the plain base
    /// palette if the custom definition is invalid.
    pub fn resolved_theme(&self) -> crate::theme::Theme {
        match &self.custom_theme {
            Some(custom) => match custom.resolve(&self.general.theme) {
                Ok(t) => t,
                Err(errors) => {
                    for e in errors {
                        error!("custom_theme: {}", e);
                    }
                    warn!("custom_theme ignored due to errors; using built-in palette");
                    crate::theme::Theme::from_name(&self.general.theme)
                }
            },
            None => crate::theme::Theme::from_name(&self.general.theme),
        }
    }
}
