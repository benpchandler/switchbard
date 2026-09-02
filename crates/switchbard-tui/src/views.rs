//! Saved views: numbered slots of filter + sort. Slot 1 is what `sbt` opens on.
//! Stored as Lua data in `~/.switchbard/views.lua`, rewritten by `v s <n>`.

use std::path::{Path, PathBuf};

use mlua::{Lua, Table, Value};

use crate::sort::Sort;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedView {
    pub name: String,
    pub filter: String,
    pub sort: Option<Sort>,
}

pub const MAX_SLOTS: usize = 9;

pub fn default_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".switchbard").join("views.lua"))
}

pub fn starter_views() -> Vec<SavedView> {
    [
        ("all", ""),
        ("todo", "status:todo"),
        ("active", "status:inprogress"),
        ("tui", "label:tui"),
    ]
    .into_iter()
    .map(|(name, filter)| SavedView {
        name: name.to_string(),
        filter: filter.to_string(),
        sort: None,
    })
    .collect()
}

/// Loads the slots, or the starter set when the file is absent or unreadable.
pub fn load(path: Option<&Path>) -> (Vec<SavedView>, Option<String>) {
    let Some(path) = path else {
        return (starter_views(), None);
    };
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (starter_views(), None)
        }
        Err(error) => {
            return (
                starter_views(),
                Some(format!("{}: {error}", path.display())),
            )
        }
    };
    match parse(&source) {
        Ok(views) if views.is_empty() => (starter_views(), None),
        Ok(views) => (views, None),
        Err(error) => (
            starter_views(),
            Some(format!("{}: {error}", path.display())),
        ),
    }
}

fn parse(source: &str) -> mlua::Result<Vec<SavedView>> {
    let lua = Lua::new();
    let table: Table = lua.load(source).eval()?;
    let mut views = Vec::new();
    for entry in table.sequence_values::<Table>() {
        let entry = entry?;
        let sort_text: String = entry.get::<Option<String>>("sort")?.unwrap_or_default();
        views.push(SavedView {
            name: entry.get::<Option<String>>("name")?.unwrap_or_default(),
            filter: entry.get::<Option<String>>("filter")?.unwrap_or_default(),
            sort: Sort::parse(&sort_text),
        });
    }
    let _ = Value::Nil;
    Ok(views.into_iter().take(MAX_SLOTS).collect())
}

pub fn save(path: &Path, views: &[SavedView]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = String::from(
        "-- Saved sbt views. Slot 1 opens by default. `v <n>` opens a slot, `v s <n>` saves\n\
         -- the current filter and sort into it. sbt rewrites this file; editing by hand is fine.\n\
         return {\n",
    );
    for view in views {
        text.push_str(&format!(
            "  {{ name = {}, filter = {}, sort = {} }},\n",
            lua_string(&view.name),
            lua_string(&view.filter),
            lua_string(&view.sort.map(|sort| sort.to_text()).unwrap_or_default()),
        ));
    }
    text.push_str("}\n");
    let tmp = path.with_extension("lua.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

fn lua_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}
