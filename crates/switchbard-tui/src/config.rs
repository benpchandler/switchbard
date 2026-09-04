//! User configuration: `default.lua` baked in, `~/.switchbard/tui.lua` layered over it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mlua::{Lua, Table, Value};
use ratatui::style::{Color, Modifier, Style};

use crate::columns::Column;

const DEFAULT_LUA: &str = include_str!("default.lua");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Down,
    Up,
    Top,
    Bottom,
    PageDown,
    PageUp,
    Open,
    Back,
    Filter,
    FilterColumn,
    SortColumn,
    Columns,
    Paint,
    Ball,
    Command,
    Reload,
    Help,
    Quit,
    View,
    Group,
    Settings,
    /// The task chord: rank digits, `t`/`d`/`p` for the top list, `g` for goals.
    Rank,
}

impl Action {
    fn parse(text: &str) -> Option<Action> {
        Some(match text {
            "down" => Action::Down,
            "up" => Action::Up,
            "top" => Action::Top,
            "bottom" => Action::Bottom,
            "page_down" => Action::PageDown,
            "page_up" => Action::PageUp,
            "open" => Action::Open,
            "back" => Action::Back,
            "filter" => Action::Filter,
            "filter_column" => Action::FilterColumn,
            "sort_column" => Action::SortColumn,
            "columns" => Action::Columns,
            "paint" => Action::Paint,
            "ball" => Action::Ball,
            "command" => Action::Command,
            "reload" => Action::Reload,
            "help" => Action::Help,
            "quit" => Action::Quit,
            "view" => Action::View,
            "group" => Action::Group,
            "settings" => Action::Settings,
            "task" | "rank" => Action::Rank,
            _ => return None,
        })
    }

    pub fn name(&self) -> String {
        match self {
            Action::Down => "down".to_string(),
            Action::Up => "up".to_string(),
            Action::Top => "top".to_string(),
            Action::Bottom => "bottom".to_string(),
            Action::PageDown => "page_down".to_string(),
            Action::PageUp => "page_up".to_string(),
            Action::Open => "open".to_string(),
            Action::Back => "back".to_string(),
            Action::Filter => "filter".to_string(),
            Action::FilterColumn => "filter_column".to_string(),
            Action::SortColumn => "sort_column".to_string(),
            Action::Columns => "columns".to_string(),
            Action::Paint => "paint".to_string(),
            Action::Ball => "ball".to_string(),
            Action::Command => "command".to_string(),
            Action::Reload => "reload".to_string(),
            Action::Help => "help".to_string(),
            Action::Quit => "quit".to_string(),
            Action::View => "view".to_string(),
            Action::Group => "group".to_string(),
            Action::Settings => "settings".to_string(),
            Action::Rank => "task".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub code: KeyCode,
    pub ctrl: bool,
}

impl KeyChord {
    pub fn parse(text: &str) -> Option<KeyChord> {
        let (ctrl, rest) = match text.strip_prefix("ctrl-") {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let code = match rest {
            "enter" => KeyCode::Enter,
            "esc" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "backspace" => KeyCode::Backspace,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "space" => KeyCode::Char(' '),
            single if single.chars().count() == 1 => KeyCode::Char(single.chars().next()?),
            _ => return None,
        };
        Some(KeyChord { code, ctrl })
    }

    pub fn from_event(event: &KeyEvent) -> KeyChord {
        KeyChord {
            code: event.code,
            ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        }
    }

    pub fn label(&self) -> String {
        let key = match self.code {
            KeyCode::Char(' ') => "space".to_string(),
            KeyCode::Char(c) => c.to_string(),
            other => format!("{other:?}").to_lowercase(),
        };
        if self.ctrl {
            format!("ctrl-{key}")
        } else {
            key
        }
    }
}

/// A named area of the screen. The Lua `theme` table shades each one: a bare
/// color string is its foreground, a table sets `fg`, `bg`, `bold`, `underline`,
/// `italic`, `dim`, `reverse`. Which columns wear `label` or `link` is
/// `theme.columns = { id = "label", project = "link" }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Surface {
    /// The repo name at the top left: the "ticker" chip.
    TitleRepo,
    /// The rest of the title: view, filter, sort, counts.
    Title,
    Border,
    /// The numbered column header row.
    Header,
    /// A section heading when grouped.
    Heading,
    /// The cursor row.
    Selected,
    /// Key columns (id by default): identifies the row.
    Label,
    /// Ordinary cell text.
    Text,
    /// Columns that name something elsewhere (project by default).
    Link,
    /// The active filter in the footer, and other "in effect" chips.
    Chip,
    /// Key letters in footer hints and in `?`.
    Keys,
    /// Explanatory text: hints, counts, secondary lines.
    Hint,
    /// The status line after an action.
    Status,
    /// Picker highlight, cursors, checkmarks.
    Accent,
}

impl Surface {
    fn parse(name: &str) -> Option<Surface> {
        Some(match name {
            "title_repo" => Surface::TitleRepo,
            "title" => Surface::Title,
            "border" => Surface::Border,
            "header" => Surface::Header,
            "heading" => Surface::Heading,
            "selected" => Surface::Selected,
            "label" => Surface::Label,
            "text" => Surface::Text,
            "link" => Surface::Link,
            "chip" => Surface::Chip,
            "keys" => Surface::Keys,
            "hint" | "dim" => Surface::Hint,
            "status" => Surface::Status,
            "accent" => Surface::Accent,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Theme {
    styles: HashMap<Surface, Style>,
    columns: HashMap<Column, Surface>,
}

impl Theme {
    pub fn style(&self, surface: Surface) -> Style {
        self.styles.get(&surface).copied().unwrap_or_default()
    }

    /// The surface a column's cells wear before paint: label, link, or text.
    pub fn column_style(&self, column: Column) -> Style {
        self.style(self.columns.get(&column).copied().unwrap_or(Surface::Text))
    }
}

/// A preset's surfaces and its column-to-surface map, before validation.
type RawTheme = (HashMap<String, RawStyle>, HashMap<String, String>);

/// One surface as the Lua file spells it, before validation.
#[derive(Debug, Clone, Default)]
struct RawStyle {
    fg: Option<String>,
    bg: Option<String>,
    bold: bool,
    underline: bool,
    italic: bool,
    dim: bool,
    reverse: bool,
}

impl RawStyle {
    fn from_value(value: Value) -> mlua::Result<RawStyle> {
        match value {
            Value::String(text) => Ok(RawStyle {
                fg: Some(text.to_str()?.to_string()),
                ..RawStyle::default()
            }),
            Value::Table(table) => Ok(RawStyle {
                fg: table.get::<Option<String>>("fg")?,
                bg: table.get::<Option<String>>("bg")?,
                bold: table.get::<Option<bool>>("bold")?.unwrap_or(false),
                underline: table.get::<Option<bool>>("underline")?.unwrap_or(false),
                italic: table.get::<Option<bool>>("italic")?.unwrap_or(false),
                dim: table.get::<Option<bool>>("dim")?.unwrap_or(false),
                reverse: table.get::<Option<bool>>("reverse")?.unwrap_or(false),
            }),
            other => Err(mlua::Error::runtime(format!(
                "a theme entry is a color string or a table, not {}",
                other.type_name()
            ))),
        }
    }

    fn into_style(self, name: &str, warnings: &mut Vec<String>) -> Style {
        let mut color = |which: &str, text: Option<String>| -> Option<Color> {
            let text = text?;
            match Color::from_str(&text) {
                Ok(color) => Some(color),
                Err(_) => {
                    warnings.push(format!("bad color '{text}' for theme.{name}.{which}"));
                    None
                }
            }
        };
        let mut style = Style::default();
        if let Some(fg) = color("fg", self.fg) {
            style = style.fg(fg);
        }
        if let Some(bg) = color("bg", self.bg) {
            style = style.bg(bg);
        }
        for (on, modifier) in [
            (self.bold, Modifier::BOLD),
            (self.underline, Modifier::UNDERLINED),
            (self.italic, Modifier::ITALIC),
            (self.dim, Modifier::DIM),
            (self.reverse, Modifier::REVERSED),
        ] {
            if on {
                style = style.add_modifier(modifier);
            }
        }
        style
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub keys: HashMap<KeyChord, Action>,
    pub theme: Theme,
    /// Column -> loose value -> glyph, for columns shown in glyph mode.
    pub glyphs: HashMap<Column, HashMap<String, String>>,
    /// What `p <col> 1` (auto) hands out, in order: the chosen preset or a user list.
    pub palette: Vec<String>,
    /// The presets `:palette <name>` and `palette = "<name>"` choose from.
    pub palettes: Vec<(String, Vec<String>)>,
    /// Where `:bug` and `:idea` file: sbt's own repo, not the one being browsed.
    /// `None` files into the current repo.
    pub report_repo: Option<PathBuf>,
    pub warnings: Vec<String>,
}

impl Config {
    /// The glyph for `value` in `column`: configured, else its first letter.
    pub fn glyph(&self, column: Column, value: &str) -> String {
        let key = crate::tasks::Filter::loose_key(value);
        self.glyphs
            .get(&column)
            .and_then(|map| map.get(&key))
            .filter(|glyph| !glyph.is_empty())
            .cloned()
            .unwrap_or_else(|| {
                value
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default()
            })
    }

    pub fn bindings_for(&self, action: &Action) -> Vec<String> {
        let mut keys: Vec<String> = self
            .keys
            .iter()
            .filter(|(_, bound)| *bound == action)
            .map(|(chord, _)| chord.label())
            .collect();
        keys.sort();
        keys
    }
}

/// Where the user's overrides live. `None` when no home directory exists.
pub fn user_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".switchbard").join("tui.lua"))
}

pub fn modified_at(path: &Path) -> Option<SystemTime> {
    path.metadata().and_then(|meta| meta.modified()).ok()
}

/// Loads the baked-in defaults, then layers the user's file over them.
/// Never fails: a broken user file yields the defaults plus a warning.
pub fn load(user_path: Option<&Path>) -> Config {
    let mut raw = RawConfig::from_lua(DEFAULT_LUA).expect("default.lua must evaluate");
    let mut warnings = Vec::new();
    if let Some(path) = user_path {
        match std::fs::read_to_string(path) {
            Ok(source) => match RawConfig::from_lua(&source) {
                Ok(user) => raw.merge(user),
                Err(error) => warnings.push(format!("{}: {error}", path.display())),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => warnings.push(format!("{}: {error}", path.display())),
        }
    }
    raw.into_config(warnings)
}

#[derive(Default)]
struct RawConfig {
    keys: HashMap<String, String>,
    /// Surface overrides from a `theme = { ... }` table.
    theme: HashMap<String, RawStyle>,
    theme_columns: HashMap<String, String>,
    /// `theme = "<name>"`: which entry of `themes` to start from.
    theme_name: Option<String>,
    themes: HashMap<String, RawTheme>,
    glyphs: HashMap<String, HashMap<String, String>>,
    palette: Vec<String>,
    palette_name: Option<String>,
    report_repo: Option<String>,
    palettes: Vec<(String, Vec<String>)>,
}

impl RawConfig {
    fn from_lua(source: &str) -> mlua::Result<RawConfig> {
        let lua = Lua::new();
        let table: Table = lua.load(source).eval()?;
        Ok(RawConfig {
            keys: string_map(&table, "keys")?,
            theme: theme_map(&table)?,
            theme_columns: theme_columns(&table)?,
            theme_name: table.get::<Option<String>>("theme").ok().flatten(),
            themes: theme_presets(&table)?,
            glyphs: nested_string_map(&table, "glyphs")?,
            palette: string_list(&table, "palette")?,
            palette_name: table.get::<Option<String>>("palette").ok().flatten(),
            report_repo: table.get::<Option<String>>("report_repo").ok().flatten(),
            palettes: named_string_lists(&table, "palettes")?,
        })
    }

    fn merge(&mut self, over: RawConfig) {
        self.keys.extend(over.keys);
        if over.theme_name.is_some() {
            self.theme_name = over.theme_name;
            self.theme.clear();
            self.theme_columns.clear();
        }
        self.theme.extend(over.theme);
        self.theme_columns.extend(over.theme_columns);
        self.themes.extend(over.themes);
        for (column, map) in over.glyphs {
            self.glyphs.entry(column).or_default().extend(map);
        }
        if !over.palette.is_empty() {
            self.palette = over.palette;
            self.palette_name = None;
        } else if over.palette_name.is_some() {
            self.palette_name = over.palette_name;
            self.palette = Vec::new();
        }
        if over.report_repo.is_some() {
            self.report_repo = over.report_repo;
        }
        for (name, colors) in over.palettes {
            self.palettes.retain(|(known, _)| *known != name);
            self.palettes.push((name, colors));
        }
    }

    fn into_config(self, mut warnings: Vec<String>) -> Config {
        let mut keys = HashMap::new();
        for (key, action) in self.keys {
            match (KeyChord::parse(&key), Action::parse(&action)) {
                (Some(chord), Some(action)) => {
                    keys.insert(chord, action);
                }
                (None, _) => warnings.push(format!("unknown key '{key}'")),
                (_, None) => warnings.push(format!("unknown action '{action}' for key '{key}'")),
            }
        }
        let (mut raw_styles, mut raw_columns) = match self.theme_name.as_deref() {
            Some(name) => match self.themes.get(name) {
                Some((styles, columns)) => (styles.clone(), columns.clone()),
                None => {
                    warnings.push(format!("unknown theme '{name}': one of {}", {
                        let mut names: Vec<&String> = self.themes.keys().collect();
                        names.sort();
                        names
                            .iter()
                            .map(|n| n.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }));
                    (HashMap::new(), HashMap::new())
                }
            },
            None => (HashMap::new(), HashMap::new()),
        };
        raw_styles.extend(self.theme);
        raw_columns.extend(self.theme_columns);
        let mut styles = HashMap::new();
        for (name, raw) in raw_styles {
            match Surface::parse(&name) {
                Some(surface) => {
                    styles.insert(surface, raw.into_style(&name, &mut warnings));
                }
                None => warnings.push(format!("unknown theme surface '{name}'")),
            }
        }
        let mut theme_columns = HashMap::new();
        for (column_name, surface_name) in raw_columns {
            match (Column::parse(&column_name), Surface::parse(&surface_name)) {
                (Some(column), Some(surface)) => {
                    theme_columns.insert(column, surface);
                }
                _ => warnings.push(format!(
                    "theme.columns: '{column_name} = {surface_name}' names no column or surface"
                )),
            }
        }
        let theme = Theme {
            styles,
            columns: theme_columns,
        };
        let mut glyphs: HashMap<Column, HashMap<String, String>> = HashMap::new();
        for (column_name, map) in self.glyphs {
            match Column::parse(&column_name) {
                Some(column) => {
                    let entry = glyphs.entry(column).or_default();
                    for (value, glyph) in map {
                        entry.insert(crate::tasks::Filter::loose_key(&value), glyph);
                    }
                }
                None => warnings.push(format!("unknown column '{column_name}' in glyphs")),
            }
        }
        let mut palettes: Vec<(String, Vec<String>)> = Vec::new();
        for (name, colors) in self.palettes {
            let mut kept = Vec::new();
            for text in colors {
                if Color::from_str(&text).is_ok() {
                    kept.push(text);
                } else {
                    warnings.push(format!("bad color '{text}' in palettes.{name}"));
                }
            }
            palettes.push((name, kept));
        }
        let mut palette = Vec::new();
        for text in self.palette {
            if Color::from_str(&text).is_ok() {
                palette.push(text);
            } else {
                warnings.push(format!("bad color '{text}' in palette"));
            }
        }
        if palette.is_empty() {
            let name = self.palette_name.unwrap_or_default();
            match palettes.iter().find(|(known, _)| *known == name) {
                Some((_, colors)) => palette = colors.clone(),
                None => {
                    if !name.is_empty() {
                        warnings.push(format!("unknown palette '{name}'"));
                    }
                    if let Some((_, colors)) = palettes.first() {
                        palette = colors.clone();
                    }
                }
            }
        }
        let report_repo = self.report_repo.map(|text| expand_home(&text));
        Config {
            keys,
            theme,
            glyphs,
            palette,
            palettes,
            report_repo,
            warnings,
        }
    }
}

fn nested_string_map(
    table: &Table,
    key: &str,
) -> mlua::Result<HashMap<String, HashMap<String, String>>> {
    let mut out = HashMap::new();
    let Value::Table(inner) = table.get::<Value>(key)? else {
        return Ok(out);
    };
    for pair in inner.pairs::<String, Table>() {
        let (name, values) = pair?;
        let mut map = HashMap::new();
        for entry in values.pairs::<String, String>() {
            let (value, glyph) = entry?;
            map.insert(value, glyph);
        }
        out.insert(name, map);
    }
    Ok(out)
}

/// `key = { name = { "..." }, ... }`, in the order the file names them.
fn named_string_lists(table: &Table, key: &str) -> mlua::Result<Vec<(String, Vec<String>)>> {
    let Value::Table(inner) = table.get::<Value>(key)? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for pair in inner.pairs::<String, Table>() {
        let (name, colors) = pair?;
        out.push((
            name,
            colors
                .sequence_values::<String>()
                .collect::<mlua::Result<_>>()?,
        ));
    }
    out.sort();
    Ok(out)
}

/// `theme = { surface = "color" | { fg=, bg=, bold= ... }, columns = { id = "label" } }`.
/// A `theme = "<name>"` string is read elsewhere and yields no overrides here.
fn theme_map(table: &Table) -> mlua::Result<HashMap<String, RawStyle>> {
    match table.get::<Value>("theme")? {
        Value::Table(inner) => surface_map(&inner),
        _ => Ok(HashMap::new()),
    }
}

fn surface_map(theme: &Table) -> mlua::Result<HashMap<String, RawStyle>> {
    let mut out = HashMap::new();
    for pair in theme.pairs::<String, Value>() {
        let (name, value) = pair?;
        if name == "columns" {
            continue;
        }
        out.insert(name, RawStyle::from_value(value)?);
    }
    Ok(out)
}

fn theme_columns(table: &Table) -> mlua::Result<HashMap<String, String>> {
    match table.get::<Value>("theme")? {
        Value::Table(theme) => string_map(&theme, "columns"),
        _ => Ok(HashMap::new()),
    }
}

/// `themes = { name = { <surfaces>, columns = {...} }, ... }`.
fn theme_presets(table: &Table) -> mlua::Result<HashMap<String, RawTheme>> {
    let mut out = HashMap::new();
    let Value::Table(inner) = table.get::<Value>("themes")? else {
        return Ok(out);
    };
    for pair in inner.pairs::<String, Table>() {
        let (name, theme) = pair?;
        out.insert(name, (surface_map(&theme)?, string_map(&theme, "columns")?));
    }
    Ok(out)
}

/// `~/x` -> `$HOME/x`; anything else is taken as written.
fn expand_home(text: &str) -> PathBuf {
    match text.strip_prefix("~/") {
        Some(rest) => dirs::home_dir()
            .map(|home| home.join(rest))
            .unwrap_or_else(|| PathBuf::from(text)),
        None => PathBuf::from(text),
    }
}

fn string_list(table: &Table, key: &str) -> mlua::Result<Vec<String>> {
    let Value::Table(inner) = table.get::<Value>(key)? else {
        return Ok(Vec::new());
    };
    inner.sequence_values::<String>().collect()
}

fn string_map(table: &Table, key: &str) -> mlua::Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    let Value::Table(inner) = table.get::<Value>(key)? else {
        return Ok(out);
    };
    for pair in inner.pairs::<String, String>() {
        let (name, value) = pair?;
        out.insert(name, value);
    }
    Ok(out)
}
