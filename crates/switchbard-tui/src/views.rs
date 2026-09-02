//! Saved views: numbered slots of filter + sort. Slot 1 is what `sbt` opens on.
//! Global slots live in `~/.switchbard/views.lua`; each repo can override slots in
//! `~/.switchbard/views/<repo path>.lua`. `v s <n>` writes the repo file, `v g <n>`
//! promotes a repo slot to the global file so every repo sees it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mlua::{Lua, Table};

use crate::config::Column;
use crate::paint::{parse_rules, rules_text, PaintRule};
use crate::sort::Sort;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedView {
    pub filter: String,
    pub sort: Option<Sort>,
    /// Shown columns in display order.
    pub columns: Vec<Column>,
    /// Columns shown as glyphs instead of text.
    pub glyph_columns: Vec<Column>,
    pub paint: Vec<PaintRule>,
}

impl SavedView {
    /// A view is named by what it does, so it reads the same in every repo.
    pub fn name(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.filter.is_empty() {
            parts.push(self.filter.clone());
        }
        if let Some(sort) = self.sort {
            parts.push(sort.label());
        }
        if let Some(columns) = self.columns_label() {
            parts.push(columns);
        }
        if !self.glyph_columns.is_empty() {
            parts.push(format!("glyphs:{}", columns_text(&self.glyph_columns)));
        }
        if !self.paint.is_empty() {
            parts.push(format!("paint:{}", self.paint.len()));
        }
        if parts.is_empty() {
            "all".to_string()
        } else {
            parts.join(" ")
        }
    }

    /// `cols:id,title` when the columns differ from the default set, else nothing.
    pub fn columns_label(&self) -> Option<String> {
        if self.columns == Column::DEFAULT_SHOWN {
            return None;
        }
        Some(format!("cols:{}", columns_text(&self.columns)))
    }
}

pub fn columns_text(columns: &[Column]) -> String {
    columns
        .iter()
        .map(|column| column.name())
        .collect::<Vec<_>>()
        .join(",")
}

pub fn parse_columns(text: &str) -> Vec<Column> {
    let columns: Vec<Column> = text
        .split(',')
        .filter_map(|name| Column::parse(name.trim()))
        .collect();
    if columns.is_empty() {
        Column::DEFAULT_SHOWN.to_vec()
    } else {
        columns
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Repo,
}

pub const MAX_SLOTS: usize = 9;

pub fn global_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".switchbard").join("views.lua"))
}

pub fn repo_path(repo_root: &Path) -> Option<PathBuf> {
    let key: String = repo_root
        .to_string_lossy()
        .trim_start_matches('/')
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dirs::home_dir().map(|home| {
        home.join(".switchbard")
            .join("views")
            .join(format!("{key}.lua"))
    })
}

pub fn starter_views() -> Vec<SavedView> {
    [
        "",
        "status:todo",
        "status:inprogress",
        "label:tui",
        "ball:me",
    ]
    .into_iter()
    .map(|filter| SavedView {
        filter: filter.to_string(),
        sort: None,
        columns: Column::DEFAULT_SHOWN.to_vec(),
        glyph_columns: Vec::new(),
        paint: Vec::new(),
    })
    .collect()
}

pub struct ViewStore {
    global_path: Option<PathBuf>,
    repo_path: Option<PathBuf>,
    global: Vec<SavedView>,
    /// Zero-based slot -> this repo's override.
    repo: BTreeMap<usize, SavedView>,
}

impl ViewStore {
    /// Reads both files; anything missing or broken falls back and is reported.
    pub fn load(
        global_path: Option<PathBuf>,
        repo_path: Option<PathBuf>,
    ) -> (ViewStore, Vec<String>) {
        let mut warnings = Vec::new();
        let global = match global_path
            .as_deref()
            .map(|path| read_lua(path, parse_sequence))
        {
            Some(Ok(Some(views))) if !views.is_empty() => views,
            Some(Ok(_)) | None => starter_views(),
            Some(Err(error)) => {
                warnings.push(error);
                starter_views()
            }
        };
        let repo = match repo_path
            .as_deref()
            .map(|path| read_lua(path, parse_overrides))
        {
            Some(Ok(Some(overrides))) => overrides,
            Some(Ok(None)) | None => BTreeMap::new(),
            Some(Err(error)) => {
                warnings.push(error);
                BTreeMap::new()
            }
        };
        let store = ViewStore {
            global_path,
            repo_path,
            global,
            repo,
        };
        (store, warnings)
    }

    /// The slots as the user sees them: repo overrides win, global fills the rest.
    pub fn slots(&self) -> Vec<(SavedView, Scope)> {
        let count = self
            .global
            .len()
            .max(self.repo.keys().last().map(|last| last + 1).unwrap_or(0));
        (0..count)
            .filter_map(|slot| match (self.repo.get(&slot), self.global.get(slot)) {
                (Some(view), _) => Some((view.clone(), Scope::Repo)),
                (None, Some(view)) => Some((view.clone(), Scope::Global)),
                (None, None) => None,
            })
            .collect()
    }

    pub fn get(&self, slot: usize) -> Option<SavedView> {
        self.slots().get(slot).map(|(view, _)| view.clone())
    }

    pub fn len(&self) -> usize {
        self.slots().len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots().is_empty()
    }

    /// Saves into this repo's overrides and writes the repo file.
    pub fn save_repo(&mut self, slot: usize, view: SavedView) -> Result<(), String> {
        self.repo.insert(slot, view);
        self.write_repo()
    }

    /// Copies the effective slot into the global file and drops the repo override.
    pub fn promote(&mut self, slot: usize) -> Result<(), String> {
        let effective = self.slots();
        for (index, (view, _)) in effective.iter().enumerate().take(slot + 1) {
            if index < self.global.len() {
                if index == slot {
                    self.global[index] = view.clone();
                }
            } else {
                self.global.push(view.clone());
            }
        }
        self.repo.remove(&slot);
        self.write_global()?;
        self.write_repo()
    }

    fn write_global(&self) -> Result<(), String> {
        let Some(path) = self.global_path.as_deref() else {
            return Ok(());
        };
        let mut text = String::from(
            "-- Global sbt views, one per slot. Slot 1 opens by default in every repo.\n\
             -- `v g <n>` inside sbt promotes a repo slot here. Editing by hand is fine.\n\
             return {\n",
        );
        for view in &self.global {
            text.push_str(&format!("  {},\n", lua_view(view)));
        }
        text.push_str("}\n");
        write_atomically(path, &text)
    }

    fn write_repo(&self) -> Result<(), String> {
        let Some(path) = self.repo_path.as_deref() else {
            return Ok(());
        };
        let mut text = String::from(
            "-- This repo's sbt view overrides, keyed by slot number; other slots fall\n\
             -- through to ~/.switchbard/views.lua. `v s <n>` writes here. Hand edits are fine.\n\
             return {\n",
        );
        for (slot, view) in &self.repo {
            text.push_str(&format!("  [{}] = {},\n", slot + 1, lua_view(view)));
        }
        text.push_str("}\n");
        write_atomically(path, &text)
    }
}

/// Evaluates the file and decodes it while the Lua state is still alive.
fn read_lua<T>(
    path: &Path,
    decode: impl FnOnce(&Table) -> Result<T, String>,
) -> Result<Option<T>, String> {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let lua = Lua::new();
    let table: Table = lua
        .load(&source)
        .eval()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    decode(&table)
        .map(Some)
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn parse_sequence(table: &Table) -> Result<Vec<SavedView>, String> {
    let mut views = Vec::new();
    for entry in table.sequence_values::<Table>() {
        views.push(parse_view(&entry.map_err(|e| e.to_string())?)?);
    }
    Ok(views.into_iter().take(MAX_SLOTS).collect())
}

fn parse_overrides(table: &Table) -> Result<BTreeMap<usize, SavedView>, String> {
    let mut views = BTreeMap::new();
    for pair in table.pairs::<usize, Table>() {
        let (slot, entry) = pair.map_err(|e| e.to_string())?;
        if (1..=MAX_SLOTS).contains(&slot) {
            views.insert(slot - 1, parse_view(&entry)?);
        }
    }
    Ok(views)
}

fn parse_view(entry: &Table) -> Result<SavedView, String> {
    let field = |key: &str| -> Result<String, String> {
        entry
            .get::<Option<String>>(key)
            .map(Option::unwrap_or_default)
            .map_err(|e| e.to_string())
    };
    Ok(SavedView {
        filter: field("filter")?,
        sort: Sort::parse(&field("sort")?),
        columns: parse_columns(&field("columns")?),
        glyph_columns: field("glyphs")?
            .split(',')
            .filter_map(|name| Column::parse(name.trim()))
            .collect(),
        paint: parse_rules(&field("paint")?),
    })
}

fn lua_view(view: &SavedView) -> String {
    let glyphs = if view.glyph_columns.is_empty() {
        String::new()
    } else {
        format!(
            ", glyphs = {}",
            lua_string(&columns_text(&view.glyph_columns))
        )
    };
    let paint = if view.paint.is_empty() {
        String::new()
    } else {
        format!(", paint = {}", lua_string(&rules_text(&view.paint)))
    };
    format!(
        "{{ filter = {}, sort = {}, columns = {}{glyphs}{paint} }}",
        lua_string(&view.filter),
        lua_string(&view.sort.map(|sort| sort.to_text()).unwrap_or_default()),
        lua_string(&columns_text(&view.columns)),
    )
}

fn lua_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn write_atomically(path: &Path, text: &str) -> Result<(), String> {
    let attempt = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("lua.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    };
    attempt().map_err(|error| format!("could not write {}: {error}", path.display()))
}
