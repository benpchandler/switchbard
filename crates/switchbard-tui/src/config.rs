//! User configuration: `default.lua` baked in, `~/.switchbard/tui.lua` layered over it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mlua::{Lua, Table, Value};
use ratatui::style::Color;

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

#[derive(Debug, Clone)]
pub struct Theme {
    pub accent: Color,
    pub header: Color,
    pub dim: Color,
    pub selected: Color,
    pub border: Color,
    /// Section headings when grouped; must not share a color with painted rows.
    pub heading: Color,
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
    theme: HashMap<String, String>,
    glyphs: HashMap<String, HashMap<String, String>>,
    palette: Vec<String>,
    palette_name: Option<String>,
    palettes: Vec<(String, Vec<String>)>,
}

impl RawConfig {
    fn from_lua(source: &str) -> mlua::Result<RawConfig> {
        let lua = Lua::new();
        let table: Table = lua.load(source).eval()?;
        Ok(RawConfig {
            keys: string_map(&table, "keys")?,
            theme: string_map(&table, "theme")?,
            glyphs: nested_string_map(&table, "glyphs")?,
            palette: string_list(&table, "palette")?,
            palette_name: table.get::<Option<String>>("palette").ok().flatten(),
            palettes: named_string_lists(&table, "palettes")?,
        })
    }

    fn merge(&mut self, over: RawConfig) {
        self.keys.extend(over.keys);
        self.theme.extend(over.theme);
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
        let mut color = |name: &str, fallback: Color| match self.theme.get(name) {
            Some(text) => Color::from_str(text).unwrap_or_else(|_| {
                warnings.push(format!("bad color '{text}' for theme.{name}"));
                fallback
            }),
            None => fallback,
        };
        let theme = Theme {
            accent: color("accent", Color::Cyan),
            header: color("header", Color::Yellow),
            dim: color("dim", Color::Gray),
            selected: color("selected", Color::Indexed(236)),
            border: color("border", Color::DarkGray),
            heading: color("heading", Color::White),
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
        Config {
            keys,
            theme,
            glyphs,
            palette,
            palettes,
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
