//! User configuration: `default.lua` baked in, `~/.switchbard/tui.lua` layered over it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mlua::{Lua, Table, Value};
use ratatui::style::{Color, Modifier, Style};

use crate::columns::{Column, ColumnRegistry};

const DEFAULT_LUA: &str = include_str!("default.lua");
pub const EMPHASIS_ROLES: [&str; 5] = ["quiet", "strong", "alert", "band", "struck"];
/// A working row pulses: bright, fading out, fading back in, once per period,
/// redrawn `frames` times per period.
const DEFAULT_WORK_PERIOD_MS: u64 = 3000;
const DEFAULT_WORK_FRAMES: u64 = 30;
/// A small lift toward the theme's ink pole accompanies the working band.
/// Dark canvases lift toward white; light canvases deepen toward black.
const WORKING_TEXT_LIFT: f64 = 0.12;
/// How hard the pulse is clipped: 0 is a pure sine, larger holds the peak and the dark longer.
const DEFAULT_WORK_FLATTEN: f64 = 2.0;

pub use crate::shortcuts::Action;

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
            "shift-tab" | "backtab" => KeyCode::BackTab,
            "pagedown" => KeyCode::PageDown,
            "pageup" => KeyCode::PageUp,
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
            code: if event.code == KeyCode::Tab && event.modifiers.contains(KeyModifiers::SHIFT) {
                KeyCode::BackTab
            } else {
                event.code
            },
            ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        }
    }

    pub fn label(&self) -> String {
        let key = match self.code {
            KeyCode::BackTab => "shift-tab".to_string(),
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
    NavigationActive,
    Context,
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
    /// Persistent navigation counts that call attention to a destination.
    AttentionBadge,
    /// Key letters in footer hints and in `?`.
    Keys,
    /// Explanatory text: hints, counts, secondary lines.
    Hint,
    /// The status line after an action.
    Status,
    /// Picker highlight, cursors, checkmarks.
    Accent,
    /// A row a live agent session is working, visible throughout its pulse.
    Working,
}

impl Surface {
    fn parse(name: &str) -> Option<Surface> {
        Some(match name {
            "title_repo" => Surface::TitleRepo,
            "title" => Surface::Title,
            "navigation_active" => Surface::NavigationActive,
            "context" => Surface::Context,
            "border" => Surface::Border,
            "header" => Surface::Header,
            "heading" => Surface::Heading,
            "selected" => Surface::Selected,
            "label" => Surface::Label,
            "text" => Surface::Text,
            "link" => Surface::Link,
            "chip" => Surface::Chip,
            "attention_badge" => Surface::AttentionBadge,
            "keys" => Surface::Keys,
            "hint" | "dim" => Surface::Hint,
            "status" => Surface::Status,
            "accent" => Surface::Accent,
            "working" => Surface::Working,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Theme {
    styles: HashMap<Surface, Style>,
    columns: HashMap<Column, Surface>,
    emphasis: HashMap<String, Style>,
    background: Option<Color>,
}

impl Theme {
    fn fill_emphasis_defaults(&mut self) {
        for role in EMPHASIS_ROLES {
            let configured = self.emphasis.get(role).copied().unwrap_or_default();
            let base = match role {
                "quiet" => match configured.fg.or(self.style(Surface::Hint).fg) {
                    Some(fg) => Style::default().fg(fg),
                    None => Style::default().add_modifier(Modifier::DIM),
                },
                "strong" => Style::default().add_modifier(Modifier::BOLD),
                "alert" => {
                    let mut style = Style::default().add_modifier(Modifier::BOLD);
                    style.fg = self.style(Surface::Accent).fg;
                    style
                }
                "band" => match configured.bg.or(self.style(Surface::Working).bg) {
                    Some(bg) => Style::default().bg(bg),
                    None => Style::default().add_modifier(Modifier::REVERSED),
                },
                "struck" => Style::default().add_modifier(Modifier::CROSSED_OUT),
                _ => unreachable!("the emphasis role vocabulary is fixed"),
            };
            self.emphasis
                .insert(role.to_string(), base.patch(configured));
        }
    }

    /// The declared canvas color. Plain themes leave the terminal in control.
    pub fn background(&self) -> Option<Color> {
        self.background
    }

    pub fn canvas_style(&self) -> Style {
        let style = self.style(Surface::Text);
        self.background
            .map_or(style, |background| style.bg(background))
    }

    /// Compose semantic roles and legacy colors in order. The paint layer owns
    /// the trailing importance marker; it does not change a role's appearance.
    pub fn emphasis_style(&self, token: &str, palette: &[String]) -> Option<Style> {
        let mut style = Style::default();
        for part in token.split('+') {
            let part = part.trim().trim_end_matches('!');
            let next = self.emphasis.get(part).copied().or_else(|| {
                crate::paint::resolve_color(part, palette).map(|color| Style::default().fg(color))
            })?;
            style = style.patch(next);
        }
        Some(style)
    }

    /// A claimed row keeps its band and modifiers throughout the cycle. Its
    /// linear-light luminance changes by 20%, never disappearing at the trough.
    pub fn working_style(&self, glow: f64) -> Style {
        let full = self.style(Surface::Working);
        match full.bg {
            Some(Color::Rgb(r, g, b)) => {
                let scale = (0.8 + 0.2 * glow.clamp(0.0, 1.0)).powf(1.0 / 2.4);
                let channel = |value: u8| (f64::from(value) * scale).round() as u8;
                full.bg(Color::Rgb(channel(r), channel(g), channel(b)))
            }
            _ => full,
        }
    }

    /// Preserve rest ink at the trough, then increase its contrast gently.
    /// Terminal-owned foregrounds remain terminal-owned throughout the cycle.
    pub fn working_fg(&self, rest: Option<Color>, glow: f64) -> Color {
        let (r, g, b) = match rest {
            Some(Color::Rgb(r, g, b)) => (r, g, b),
            Some(other) => return other,
            None => return Color::Reset,
        };
        let lift = glow.clamp(0.0, 1.0) * WORKING_TEXT_LIFT;
        let target = match self.background {
            Some(Color::Rgb(r, g, b)) if u32::from(r) + u32::from(g) + u32::from(b) > 384 => 0.0,
            _ => 255.0,
        };
        let channel = |value: u8| {
            let value = f64::from(value);
            (value + (target - value) * lift).round() as u8
        };
        Color::Rgb(channel(r), channel(g), channel(b))
    }

    pub fn style(&self, surface: Surface) -> Style {
        self.styles
            .get(&surface)
            .or_else(|| {
                (surface == Surface::AttentionBadge)
                    .then(|| self.styles.get(&Surface::Chip))
                    .flatten()
            })
            .copied()
            .unwrap_or_default()
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
    bold: Option<bool>,
    underline: Option<bool>,
    italic: Option<bool>,
    dim: Option<bool>,
    reverse: Option<bool>,
    strikethrough: Option<bool>,
}

impl RawStyle {
    fn overlay(&mut self, other: RawStyle) {
        self.fg = other.fg.or(self.fg.take());
        self.bg = other.bg.or(self.bg.take());
        self.bold = other.bold.or(self.bold);
        self.underline = other.underline.or(self.underline);
        self.italic = other.italic.or(self.italic);
        self.dim = other.dim.or(self.dim);
        self.reverse = other.reverse.or(self.reverse);
        self.strikethrough = other.strikethrough.or(self.strikethrough);
    }

    fn from_value(value: Value) -> mlua::Result<RawStyle> {
        match value {
            Value::String(text) => Ok(RawStyle {
                fg: Some(text.to_str()?.to_string()),
                ..RawStyle::default()
            }),
            Value::Table(table) => Ok(RawStyle {
                fg: table.get::<Option<String>>("fg")?,
                bg: table.get::<Option<String>>("bg")?,
                bold: table.get::<Option<bool>>("bold")?,
                underline: table.get::<Option<bool>>("underline")?,
                italic: table.get::<Option<bool>>("italic")?,
                dim: table.get::<Option<bool>>("dim")?,
                reverse: table.get::<Option<bool>>("reverse")?,
                strikethrough: table.get::<Option<bool>>("strikethrough")?,
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
            (self.strikethrough, Modifier::CROSSED_OUT),
        ] {
            match on {
                Some(true) => style = style.add_modifier(modifier),
                Some(false) => style = style.remove_modifier(modifier),
                None => {}
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
    /// The presets `:theme <name>` and `theme = "<name>"` choose from, resolved
    /// at load so switching is a lookup. Sorted, because the failure message
    /// lists them and an unstable order would make it unreadable.
    pub themes: Vec<(String, Theme)>,
    /// Where `:bug` and `:idea` file: sbt's own repo, not the one being browsed.
    /// `None` files into the current repo.
    pub report_repo: Option<PathBuf>,
    pub inbox_keys: HashMap<KeyChord, String>,
    pub bug_codex_binary: String,
    pub bug_gate_command: String,
    /// The working-row pulse period; 0 keeps the row lit steadily.
    pub work_period_ms: u64,
    /// Redraws per period: how smooth the fade is.
    pub work_frames: u64,
    pub pr_refresh_seconds: u64,
    /// Soft-clip strength of the pulse: 0 is a pure sine, 2 flattens the tops and bottoms.
    pub work_flatten: f64,
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
        if *action == Action::NewTask {
            keys.extend(
                self.keys
                    .iter()
                    .filter(|(_, bound)| **bound == Action::Rank)
                    .map(|(key, _)| format!("{} n", key.label())),
            );
        }
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
pub fn load(user_path: Option<&Path>, registry: &ColumnRegistry) -> Config {
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
    let mut config = raw.into_config(warnings, registry);
    if let Some(path) = user_path {
        for warning in &mut config.warnings {
            if !warning.starts_with(&path.display().to_string()) {
                *warning = format!("{}: {warning}", path.display());
            }
        }
    }
    config
}

/// `work = { <key> = <ms> }`, absent when the table or key is missing.
fn work_setting(table: &Table, key: &str) -> Option<u64> {
    table
        .get::<Option<Table>>("work")
        .ok()
        .flatten()
        .and_then(|work| work.get::<Option<u64>>(key).ok().flatten())
}

fn work_setting_f64(table: &Table, key: &str) -> Option<f64> {
    table
        .get::<Option<Table>>("work")
        .ok()
        .flatten()
        .and_then(|work| work.get::<Option<f64>>(key).ok().flatten())
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
    inbox_keys: HashMap<String, String>,
    bug_codex_binary: Option<String>,
    bug_gate_command: Option<String>,
    work_period_ms: Option<u64>,
    work_frames: Option<u64>,
    pr_refresh_seconds: Option<u64>,
    work_flatten: Option<f64>,
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
            inbox_keys: string_map(&table, "inbox_keys")?,
            bug_codex_binary: table.get::<Option<String>>("bug_codex_binary")?,
            bug_gate_command: table.get::<Option<String>>("bug_gate_command")?,
            work_period_ms: work_setting(&table, "period_ms"),
            work_frames: work_setting(&table, "frames"),
            pr_refresh_seconds: table
                .get::<Option<u64>>("pr_refresh_seconds")
                .ok()
                .flatten(),
            work_flatten: work_setting_f64(&table, "flatten"),
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
        self.inbox_keys.extend(over.inbox_keys);
        if over.bug_codex_binary.is_some() {
            self.bug_codex_binary = over.bug_codex_binary;
        }
        if over.bug_gate_command.is_some() {
            self.bug_gate_command = over.bug_gate_command;
        }
        if over.report_repo.is_some() {
            self.report_repo = over.report_repo;
        }
        if over.work_period_ms.is_some() {
            self.work_period_ms = over.work_period_ms;
        }
        if over.pr_refresh_seconds.is_some() {
            self.pr_refresh_seconds = over.pr_refresh_seconds;
        }
        if over.work_frames.is_some() {
            self.work_frames = over.work_frames;
        }
        if over.work_flatten.is_some() {
            self.work_flatten = over.work_flatten;
        }
        for (name, colors) in over.palettes {
            self.palettes.retain(|(known, _)| *known != name);
            self.palettes.push((name, colors));
        }
    }

    fn into_config(self, mut warnings: Vec<String>, registry: &ColumnRegistry) -> Config {
        let mut inbox_keys = HashMap::new();
        for (key, action) in self.inbox_keys {
            if action == "none" {
                continue;
            }
            match KeyChord::parse(&key) {
                Some(chord)
                    if matches!(action.as_str(), "diff" | "publish" | "retry" | "reconcile") =>
                {
                    inbox_keys.insert(chord, action);
                }
                _ => warnings.push(format!("invalid Inbox binding {key} = {action}")),
            }
        }
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
        for (name, style) in self.theme {
            raw_styles.entry(name).or_default().overlay(style);
        }
        raw_columns.extend(self.theme_columns);

        // Every preset is resolved, not just the selected one, so `:theme` can
        // switch by lookup and a broken user preset is reported at load rather
        // than the first time it is picked. `palettes` already validates all of
        // its entries this way; a theme that only warned once chosen would be
        // the odd one out.
        let mut themes: Vec<(String, Theme)> = self
            .themes
            .iter()
            .map(|(name, (styles, columns))| {
                (
                    name.clone(),
                    resolve_theme(styles.clone(), columns.clone(), &mut warnings, registry),
                )
            })
            .collect();
        themes.sort_by(|(left, _), (right, _)| left.cmp(right));

        let theme = resolve_theme(raw_styles, raw_columns, &mut warnings, registry);
        let mut glyphs: HashMap<Column, HashMap<String, String>> = HashMap::new();
        for (column_name, map) in self.glyphs {
            match registry.parse(&column_name) {
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
            themes,
            report_repo,
            inbox_keys,
            bug_codex_binary: self.bug_codex_binary.unwrap_or_else(|| "codex".into()),
            bug_gate_command: self
                .bug_gate_command
                .unwrap_or_else(|| "mise run ci".into()),
            work_period_ms: self.work_period_ms.unwrap_or(DEFAULT_WORK_PERIOD_MS),
            work_frames: self.work_frames.unwrap_or(DEFAULT_WORK_FRAMES).max(1),
            pr_refresh_seconds: self.pr_refresh_seconds.unwrap_or(60).clamp(30, 3600),
            work_flatten: self.work_flatten.unwrap_or(DEFAULT_WORK_FLATTEN).max(0.0),
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

/// Turns one preset's raw surface/column tables into a `Theme`. Shared by config
/// load and `:theme` so a preset can never resolve two different ways.
fn resolve_theme(
    raw_styles: HashMap<String, RawStyle>,
    raw_columns: HashMap<String, String>,
    warnings: &mut Vec<String>,
    registry: &ColumnRegistry,
) -> Theme {
    let mut styles = HashMap::new();
    let mut emphasis = HashMap::new();
    let mut background = None;
    for (name, mut raw) in raw_styles {
        if name == "background" {
            background = raw.into_style(&name, warnings).fg;
            continue;
        }
        if let Some(role) = name.strip_prefix("emphasis.") {
            if EMPHASIS_ROLES.contains(&role) {
                if role != "band" {
                    if raw.bg.take().is_some() {
                        warnings.push(format!(
                            "theme.emphasis.{role}.bg is reserved for the band role"
                        ));
                    }
                    if raw.reverse == Some(true) {
                        raw.reverse = None;
                        warnings.push(format!(
                            "theme.emphasis.{role}.reverse is reserved for the band role"
                        ));
                    }
                }
                emphasis.insert(role.to_string(), raw.into_style(&name, warnings));
            } else {
                warnings.push(format!("unknown theme.emphasis.{role}"));
            }
            continue;
        }
        match Surface::parse(&name) {
            Some(surface) => {
                styles.insert(surface, raw.into_style(&name, warnings));
            }
            None => warnings.push(format!("unknown theme surface '{name}'")),
        }
    }
    let mut columns = HashMap::new();
    for (column_name, surface_name) in raw_columns {
        match (registry.parse(&column_name), Surface::parse(&surface_name)) {
            (Some(column), Some(surface)) => {
                columns.insert(column, surface);
            }
            _ => warnings.push(format!(
                "theme.columns: '{column_name} = {surface_name}' names no column or surface"
            )),
        }
    }
    let mut theme = Theme {
        styles,
        columns,
        emphasis,
        background,
    };
    theme.fill_emphasis_defaults();
    theme
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
        if name == "emphasis" {
            let Value::Table(roles) = value else {
                return Err(mlua::Error::runtime("theme.emphasis must be a table"));
            };
            for pair in roles.pairs::<String, Value>() {
                let (role, value) = pair?;
                let style = RawStyle::from_value(value).map_err(|error| {
                    mlua::Error::runtime(format!("theme.emphasis.{role}: {error}"))
                })?;
                out.insert(format!("emphasis.{role}"), style);
            }
        } else {
            out.insert(name, RawStyle::from_value(value)?);
        }
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
