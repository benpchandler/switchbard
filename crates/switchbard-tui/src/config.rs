//! User configuration: `default.lua` baked in, `~/.switchbard/tui.lua` layered over it.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mlua::{Lua, Table, Value};
use ratatui::style::Color;

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
    Command,
    Reload,
    Help,
    Quit,
    View(String),
}

impl Action {
    fn parse(text: &str) -> Option<Action> {
        if let Some(name) = text.strip_prefix("view:") {
            return Some(Action::View(name.to_string()));
        }
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
            "command" => Action::Command,
            "reload" => Action::Reload,
            "help" => Action::Help,
            "quit" => Action::Quit,
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
            Action::Command => "command".to_string(),
            Action::Reload => "reload".to_string(),
            Action::Help => "help".to_string(),
            Action::Quit => "quit".to_string(),
            Action::View(name) => format!("view:{name}"),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Id,
    Status,
    Priority,
    Title,
    Labels,
    Project,
}

impl Column {
    fn parse(text: &str) -> Option<Column> {
        Some(match text {
            "id" => Column::Id,
            "status" => Column::Status,
            "priority" => Column::Priority,
            "title" => Column::Title,
            "labels" => Column::Labels,
            "project" => Column::Project,
            _ => return None,
        })
    }

    pub fn header(self) -> &'static str {
        match self {
            Column::Id => "id",
            Column::Status => "status",
            Column::Priority => "pri",
            Column::Title => "title",
            Column::Labels => "labels",
            Column::Project => "project",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub accent: Color,
    pub dim: Color,
    pub selected: Color,
    pub border: Color,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub keys: HashMap<KeyChord, Action>,
    pub theme: Theme,
    pub columns: Vec<Column>,
    pub views: BTreeMap<String, String>,
    pub default_view: String,
    pub warnings: Vec<String>,
}

impl Config {
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
    theme: HashMap<String, String>,
    columns: Option<Vec<String>>,
    views: HashMap<String, String>,
    default_view: Option<String>,
}

impl RawConfig {
    fn from_lua(source: &str) -> mlua::Result<RawConfig> {
        let lua = Lua::new();
        let table: Table = lua.load(source).eval()?;
        Ok(RawConfig {
            keys: string_map(&table, "keys")?,
            theme: string_map(&table, "theme")?,
            columns: string_list(&table, "columns")?,
            views: string_map(&table, "views")?,
            default_view: table.get("default_view")?,
        })
    }

    fn merge(&mut self, over: RawConfig) {
        self.keys.extend(over.keys);
        self.theme.extend(over.theme);
        self.views.extend(over.views);
        if over.columns.is_some() {
            self.columns = over.columns;
        }
        if over.default_view.is_some() {
            self.default_view = over.default_view;
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
        let mut color = |name: &str, fallback: Color| match self.theme.get(name) {
            Some(text) => Color::from_str(text).unwrap_or_else(|_| {
                warnings.push(format!("bad color '{text}' for theme.{name}"));
                fallback
            }),
            None => fallback,
        };
        let theme = Theme {
            accent: color("accent", Color::Cyan),
            dim: color("dim", Color::DarkGray),
            selected: color("selected", Color::Indexed(236)),
            border: color("border", Color::DarkGray),
        };
        let columns = self
            .columns
            .unwrap_or_default()
            .iter()
            .filter_map(|name| {
                let column = Column::parse(name);
                if column.is_none() {
                    warnings.push(format!("unknown column '{name}'"));
                }
                column
            })
            .collect::<Vec<_>>();
        let columns = if columns.is_empty() {
            vec![Column::Id, Column::Status, Column::Priority, Column::Title]
        } else {
            columns
        };
        let views: BTreeMap<String, String> = self.views.into_iter().collect();
        let default_view = self.default_view.unwrap_or_else(|| "all".to_string());
        if !views.contains_key(&default_view) {
            warnings.push(format!("default_view '{default_view}' is not a view"));
        }
        Config {
            keys,
            theme,
            columns,
            views,
            default_view,
            warnings,
        }
    }
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

fn string_list(table: &Table, key: &str) -> mlua::Result<Option<Vec<String>>> {
    let Value::Table(inner) = table.get::<Value>(key)? else {
        return Ok(None);
    };
    let mut out = Vec::new();
    for value in inner.sequence_values::<String>() {
        out.push(value?);
    }
    Ok(Some(out))
}
