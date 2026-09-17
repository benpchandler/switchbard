//! User configuration: `default.lua` baked in, `~/.switchbard/tui.lua` layered over it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mlua::{Lua, Table, Value};
use ratatui::style::{Color, Modifier, Style};

use crate::columns::{Column, ColumnRegistry};
use crate::paint_eval::TokenKind;

const DEFAULT_LUA: &str = include_str!("default.lua");
pub const EMPHASIS_ROLES: [&str; 5] = ["quiet", "strong", "alert", "band", "struck"];
/// A working row pulses: bright, fading out, fading back in, once per period,
/// redrawn `frames` times per period.
const DEFAULT_WORK_PERIOD_MS: u64 = 3000;
const DEFAULT_WORK_FRAMES: u64 = 30;
/// A lift toward the theme's ink pole accompanies the working band, at its
/// strongest at the pulse peak. Dark canvases lift toward white; light
/// canvases deepen toward black. Every preset's peak sits a swing
/// (`oklch::WORK_LIGHTNESS_SWING_DARK` or `_LIGHT`) away from its declared
/// color and toward the ink (`oklch::DeclaredEndpoint`), so this is sized
/// to keep every preset's peak clearing the Lc 75 working-row floor, not
/// just light's (TASK-241) — darkroom is the tightest dark preset's margin.
const WORKING_TEXT_LIFT: f64 = 0.85;
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
    ProgressFill,
    ProgressComplete,
    ProgressEmpty,
    ProgressShell,
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
            "progress_fill" => Surface::ProgressFill,
            "progress_complete" => Surface::ProgressComplete,
            "progress_empty" => Surface::ProgressEmpty,
            "progress_shell" => Surface::ProgressShell,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Theme {
    styles: HashMap<Surface, Style>,
    columns: HashMap<Column, Surface>,
    emphasis: HashMap<String, Style>,
    /// `theme.highlights`: the fills a rule names directly, keyed `h1`, `h2`,
    /// ... A slot a preset leaves out is derived from the palette on demand,
    /// so the table holds only what the theme actually declares.
    highlights: HashMap<String, Style>,
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

    /// Any role or slot may carry a fill. What is refused is a fill no ink in
    /// this theme can be read on: whatever a rule wrote over it would be
    /// unreadable, so the fill is dropped and the cells keep their canvas. A
    /// fill only some inks carry is kept, because choosing that ink is the
    /// point of a fill-and-ink rule. Terminal-owned colors support no numeric
    /// claim and are never refused.
    fn refuse_unreadable_fills(&mut self, warnings: &mut Vec<String>) {
        let inks = self.declared_inks();
        let mut refused: Vec<(&'static str, String)> = Vec::new();
        for (table, styles) in [
            ("emphasis", &self.emphasis),
            ("highlights", &self.highlights),
        ] {
            for (key, style) in styles {
                if style.bg.is_some_and(|fill| no_ink_reads(fill, &inks)) {
                    refused.push((table, key.clone()));
                }
            }
        }
        refused.sort();
        for (table, key) in refused {
            let styles = if table == "emphasis" {
                &mut self.emphasis
            } else {
                &mut self.highlights
            };
            if let Some(style) = styles.get_mut(&key) {
                style.bg = None;
            }
            warnings.push(format!(
                "theme.{table}.{key}.bg: no ink in this theme reads on that fill, so it was dropped"
            ));
        }
    }

    /// Every ink this theme can put on a fill, for the readability check above.
    fn declared_inks(&self) -> Vec<Color> {
        self.style(Surface::Text)
            .fg
            .into_iter()
            .chain(self.emphasis.values().filter_map(|style| style.fg))
            .chain(self.highlights.values().filter_map(|style| style.fg))
            .collect()
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

    /// Compose one rule's roles, highlight slots and legacy colors into the
    /// style a cell wears: the fill first, then every ink over it
    /// (`paint_eval::compose` owns which is which). `None` when a token names
    /// nothing this theme knows. The paint layer owns the trailing importance
    /// marker; it does not change how a rule looks.
    pub fn emphasis_style(&self, token: &str, palette: &[String]) -> Option<Style> {
        let composition = crate::paint_eval::compose(token, |part| self.token_kind(part, palette))?;
        let mut style = Style::default();
        for part in composition.fill().into_iter().chain(composition.ink()) {
            style = style.patch(self.token_style(part, palette)?);
        }
        Some(style)
    }

    /// A rule's roles as the theme reads them: the fill it paints, and the ink
    /// written over it. The style picker composes against this, so what it
    /// shows and what `emphasis_style` renders can never disagree. `None` when
    /// a token names nothing this theme knows.
    pub fn split_roles(
        &self,
        roles: &str,
        palette: &[String],
    ) -> Option<(Option<String>, Vec<String>)> {
        let composition = crate::paint_eval::compose(roles, |part| self.token_kind(part, palette))?;
        Some((
            composition.fill().map(str::to_string),
            composition.ink().map(str::to_string).collect(),
        ))
    }

    /// What one token does to a cell: fills it, or writes on it. `None` names
    /// nothing, which is how an invalid rule is caught.
    fn token_kind(&self, token: &str, palette: &[String]) -> Option<TokenKind> {
        let style = self.token_style(token, palette)?;
        Some(if carries_fill(&style) {
            TokenKind::Fill
        } else {
            TokenKind::Ink
        })
    }

    /// The style behind one token: an emphasis role, a highlight slot, or a
    /// color used as ink.
    fn token_style(&self, token: &str, palette: &[String]) -> Option<Style> {
        if let Some(role) = self.emphasis.get(token) {
            return Some(*role);
        }
        if let Some(slot) = crate::highlight::slot_index(token) {
            return self.highlight_style(slot, palette);
        }
        crate::paint::resolve_color(token, palette).map(|color| Style::default().fg(color))
    }

    /// Slot `index` exactly as the theme declares it. Only a slot that declares
    /// no fill of its own borrows one, so a declared fill is never merged with
    /// the fallback, whose reverse video would otherwise invert it.
    pub fn highlight_style(&self, index: usize, palette: &[String]) -> Option<Style> {
        let declared = self
            .highlights
            .get(crate::highlight::slot_token(index)?)
            .copied();
        if let Some(declared) = declared.filter(carries_fill) {
            return Some(declared);
        }
        let derived = self.derived_fill(index, palette);
        Some(match declared {
            Some(declared) => derived.patch(declared),
            None => derived,
        })
    }

    /// The fill a slot borrows when it declares none: the matching palette color
    /// moved to a fixed step off the canvas, wearing the body ink. It is
    /// computed per frame, which is what lets `:palette` retint those slots
    /// live. A theme with no declared canvas has no lightness to step from and
    /// reverses the terminal's own colors instead.
    fn derived_fill(&self, index: usize, palette: &[String]) -> Style {
        self.background
            .and_then(|canvas| {
                let seed = crate::paint::palette_color(index, palette)?;
                crate::highlight::derive_fill(seed, canvas)
            })
            .map(|fill| self.style(Surface::Text).bg(fill))
            .unwrap_or_else(|| Style::default().add_modifier(Modifier::REVERSED))
    }

    /// The slots the picker offers: every slot that resolves to a fill of its
    /// own. A theme with a canvas derives the ones it does not declare, so all
    /// of them are offered and six of the nine carry palette hues. A theme
    /// whose colors belong to the terminal can only offer what it declares,
    /// because the rest would all be the same reverse video.
    pub fn highlight_slots(&self) -> Vec<usize> {
        let derives = self.background.is_some();
        (1..=crate::highlight::MAX_SLOTS)
            .filter(|index| {
                derives
                    || crate::highlight::slot_token(*index)
                        .is_some_and(|token| self.highlights.contains_key(token))
            })
            .collect()
    }

    /// The declared canvas reads as a light background (WCAG-adjacent sum
    /// threshold already used for the working-ink pole): the one place this
    /// decides which way the pulse and the ink compensation lean, shared by
    /// `working_style` and `working_fg` so the two can never disagree.
    fn canvas_is_light(&self) -> bool {
        matches!(self.background, Some(Color::Rgb(r, g, b))
            if u32::from(r) + u32::from(g) + u32::from(b) > 384)
    }

    /// A claimed row keeps its band and modifiers throughout the cycle. The
    /// declared `working.bg` never dims (TASK-218's floor): on a dark canvas
    /// it plays the `glow` 0 trough and the pulse brightens from there by
    /// `oklch::WORK_LIGHTNESS_SWING_DARK`; on a light canvas (already
    /// closest to the ink) it plays the `glow` 1 peak and the pulse
    /// brightens toward `glow` 0 by `oklch::WORK_LIGHTNESS_SWING_LIGHT`
    /// instead. Either way the swing is in OKLCH lightness only, never hue
    /// (TASK-241); `canvas_is_light` is the one place that picks both the
    /// direction and the matching swing, so they can never disagree.
    pub fn working_style(&self, glow: f64) -> Style {
        let full = self.style(Surface::Working);
        let Some(bg) = full.bg else { return full };
        let (endpoint, swing) = if self.canvas_is_light() {
            (
                crate::oklch::DeclaredEndpoint::Peak,
                crate::oklch::WORK_LIGHTNESS_SWING_LIGHT,
            )
        } else {
            (
                crate::oklch::DeclaredEndpoint::Trough,
                crate::oklch::WORK_LIGHTNESS_SWING_DARK,
            )
        };
        full.bg(crate::oklch::pulse_lightness(bg, endpoint, swing, glow))
    }

    /// Preserve rest ink at the trough, then lift it toward the theme's ink
    /// pole as the pulse nears its peak: brightening the declared color
    /// (`working_style`) trades away some of its contrast on every preset,
    /// and this buys it back where the peak needs it most.
    /// Terminal-owned foregrounds remain terminal-owned throughout the cycle.
    pub fn working_fg(&self, rest: Option<Color>, glow: f64) -> Color {
        let (r, g, b) = match rest {
            Some(Color::Rgb(r, g, b)) => (r, g, b),
            Some(other) => return other,
            None => return Color::Reset,
        };
        let lift = glow.clamp(0.0, 1.0) * WORKING_TEXT_LIFT;
        let target = if self.canvas_is_light() { 0.0 } else { 255.0 };
        let channel = |value: u8| {
            let value = f64::from(value);
            (value + (target - value) * lift).round() as u8
        };
        Color::Rgb(channel(r), channel(g), channel(b))
    }

    pub fn style(&self, surface: Surface) -> Style {
        self.styles
            .get(&surface)
            .or_else(|| match surface {
                Surface::AttentionBadge => self.styles.get(&Surface::Chip),
                Surface::ProgressComplete => self.styles.get(&Surface::ProgressFill),
                _ => None,
            })
            .copied()
            .unwrap_or_default()
    }

    /// The surface a column's cells wear before paint: label, link, or text.
    pub fn column_style(&self, column: Column) -> Style {
        self.style(self.columns.get(&column).copied().unwrap_or(Surface::Text))
    }
}

/// Whether a style paints the cell behind the text, by color or by reversing
/// the terminal's own.
fn carries_fill(style: &Style) -> bool {
    style.bg.is_some() || style.add_modifier.contains(Modifier::REVERSED)
}

/// Whether a fill is measurable and no measurable ink clears the readable
/// floor on it. An unmeasurable pairing makes no claim in either direction.
fn no_ink_reads(fill: Color, inks: &[Color]) -> bool {
    let judged: Vec<bool> = inks
        .iter()
        .filter_map(|ink| crate::legibility::reads_on(*ink, fill))
        .collect();
    !judged.is_empty() && !judged.contains(&true)
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
    pub progress_style: crate::progress::ProgressStyle,
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
    progress_style: Option<String>,
    palette: Vec<String>,
    palette_name: Option<String>,
    report_repo: Option<String>,
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
            progress_style: table.get("progress_style")?,
            palette: string_list(&table, "palette")?,
            palette_name: table.get::<Option<String>>("palette").ok().flatten(),
            report_repo: table.get::<Option<String>>("report_repo").ok().flatten(),
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
        if over.progress_style.is_some() {
            self.progress_style = over.progress_style;
        } else if over.glyphs.contains_key("progress") {
            self.progress_style = Some("icons".into());
        }
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
        warnings.extend(crate::shortcuts::missing_locked(keys.values().copied()));
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
        let progress_style =
            crate::progress::ProgressStyle::parse(self.progress_style.as_deref(), &mut warnings);
        Config {
            keys,
            progress_style,
            theme,
            glyphs,
            palette,
            palettes,
            themes,
            report_repo,
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
    let mut highlights = HashMap::new();
    let mut background = None;
    for (name, raw) in raw_styles {
        if name == "background" {
            background = raw.into_style(&name, warnings).fg;
            continue;
        }
        if let Some(role) = name.strip_prefix("emphasis.") {
            if EMPHASIS_ROLES.contains(&role) {
                emphasis.insert(role.to_string(), raw.into_style(&name, warnings));
            } else {
                warnings.push(format!("unknown theme.emphasis.{role}"));
            }
            continue;
        }
        if let Some(slot) = name.strip_prefix("highlights.") {
            match crate::highlight::slot_index(slot).and_then(crate::highlight::slot_token) {
                Some(token) => {
                    highlights.insert(token.to_string(), raw.into_style(&name, warnings));
                }
                None => warnings.push(format!(
                    "theme.highlights.{slot} names no slot: h1 to h{}",
                    crate::highlight::MAX_SLOTS
                )),
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
        highlights,
        background,
    };
    theme.fill_emphasis_defaults();
    theme.refuse_unreadable_fills(warnings);
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
        if name == "emphasis" || name == "highlights" {
            let Value::Table(entries) = value else {
                return Err(mlua::Error::runtime(format!(
                    "theme.{name} must be a table"
                )));
            };
            for pair in entries.pairs::<String, Value>() {
                let (key, value) = pair?;
                let style = RawStyle::from_value(value).map_err(|error| {
                    mlua::Error::runtime(format!("theme.{name}.{key}: {error}"))
                })?;
                out.insert(format!("{name}.{key}"), style);
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
