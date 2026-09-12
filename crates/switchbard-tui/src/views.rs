//! Saved views: numbered slots of `ViewState` (filter, sort, columns, glyphs, paint,
//! grouping, row layout, and name).
//! Slot 1 is what `sbt` opens on. The same Lua record serializes a slot on disk and
//! the live state across a self-restart, so one place enumerates the fields.
//! Global slots live in `~/.switchbard/views.lua`; each repo can override slots in
//! `~/.switchbard/views/<repo path>.lua`. `v s <n>` writes the repo file, `v g <n>`
//! promotes a repo slot to the global file so every repo sees it.
//!
//! A slot may also carry a user-given `name`, page-agnostic and rides along with
//! the rest of the record. `v n` names a slot; it writes wherever the slot's
//! effective definition already lives - the repo file if a repo override exists
//! for that slot, otherwise the global file. `v x` deletes a slot the same way.
//! An unnamed slot's display label falls back to `ViewState::label()`, the old
//! derived-from-contents name.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mlua::{Lua, Table};

use crate::columns::{Column, ColumnRegistry};
use crate::filter::Filter;
use crate::group::Grouping;
use crate::list_settings::ListSettings;
use crate::paint::{parse_rules, rules_text, PaintRule};
use crate::sort::Sort;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewState {
    pub filter: String,
    pub sort: Option<Sort>,
    /// Shown columns in display order.
    pub columns: Vec<Column>,
    /// Columns shown as glyphs instead of text.
    pub glyph_columns: Vec<Column>,
    /// Columns shown in their short form (bare id, H/M/L).
    pub abbreviated: Vec<Column>,
    pub paint: Vec<PaintRule>,
    /// The column the list is sectioned by, if any.
    pub group: Grouping,
    /// Whether the top list sits as its own first section.
    pub pin_top: bool,
    /// Task title wrapping and vertical separation, saved with this view.
    pub row_layout: crate::row_layout::RowLayout,
    /// A user-given name for this slot; empty means unnamed (fall back to `label()`).
    pub name: String,
}

impl ViewState {
    /// The name shown for this slot: the user-given `name` if set, else the
    /// derived `label()`.
    pub fn display_name(&self, registry: &ColumnRegistry) -> String {
        if self.name.is_empty() {
            self.label(registry)
        } else {
            self.name.clone()
        }
    }

    /// A view is named by what it does, so it reads the same in every repo.
    pub fn label(&self, registry: &ColumnRegistry) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.filter.is_empty() {
            parts.push(self.filter.clone());
        }
        if let Some(sort) = self.sort {
            parts.push(sort.label(registry));
        }
        if let Some(columns) = self.columns_label(registry) {
            parts.push(columns);
        }
        if !self.glyph_columns.is_empty() {
            parts.push(format!(
                "glyphs:{}",
                columns_text(&self.glyph_columns, registry)
            ));
        }
        if let Some(label) = self.abbreviated_label(registry) {
            parts.push(label);
        }
        if !self.paint.is_empty() {
            parts.push(format!("paint:{}", self.paint.len()));
        }
        if !self.group.is_flat() {
            // A saved slot's own resolved `auto` levels aren't known here —
            // that only exists once a loaded view is live and its filter has
            // run — so this names the grouping, not the decorated label the
            // active title bar shows (`Grouping::label`).
            parts.push(format!("outline:{}", self.group.name(registry)));
        }
        if !self.pin_top {
            parts.push("nopin".to_string());
        }
        if let Some(label) = self.row_layout.label() {
            parts.push(label);
        }
        if parts.is_empty() {
            "all".to_string()
        } else {
            parts.join(" ")
        }
    }

    /// `abbr:none` or `abbr:id` when the short-form set differs from the default, else nothing.
    pub fn abbreviated_label(&self, registry: &ColumnRegistry) -> Option<String> {
        let mut mine = self.abbreviated.clone();
        mine.sort_by_key(|c| c.name(registry));
        let mut default = Column::DEFAULT_ABBREVIATED.to_vec();
        default.sort_by_key(|c| c.name(registry));
        if mine == default {
            return None;
        }
        if mine.is_empty() {
            return Some("abbr:none".to_string());
        }
        Some(format!("abbr:{}", columns_text(&mine, registry)))
    }

    /// `cols:id,title` when the columns differ from the default set, else nothing.
    pub fn columns_label(&self, registry: &ColumnRegistry) -> Option<String> {
        if self.columns == Column::DEFAULT_SHOWN {
            return None;
        }
        Some(format!("cols:{}", columns_text(&self.columns, registry)))
    }
}

/// How a column list reads on screen: bare names, a declared field included.
pub fn columns_text(columns: &[Column], registry: &ColumnRegistry) -> String {
    columns
        .iter()
        .map(|column| column.name(registry))
        .collect::<Vec<_>>()
        .join(",")
}

/// How a column list is written to `views.lua`: a declared field carries
/// `columns::FIELD_PREFIX` so a later load can tell sbt's own dropped field
/// from a column only a newer build knows.
fn columns_save_text(columns: &[Column], registry: &ColumnRegistry) -> String {
    columns
        .iter()
        .map(|column| column.save_name(registry))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn parse_columns(text: &str, registry: &ColumnRegistry) -> Vec<Column> {
    let columns: Vec<Column> = text
        .split(',')
        .filter_map(|name| registry.parse(name))
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
    dirs::home_dir().map(|home| {
        home.join(".switchbard")
            .join("views")
            .join(format!("{}.lua", repo_file_key(repo_root)))
    })
}

/// A repo root as a file name: `Users_bpc_Dev_switchbard`. Views and settings
/// both key their per-repo files by it.
pub fn repo_file_key(repo_root: &Path) -> String {
    repo_root
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
        .collect()
}

pub fn starter_views() -> Vec<ViewState> {
    [
        "",
        "status:todo",
        "status:inprogress",
        "label:tui",
        "ball:me",
    ]
    .into_iter()
    .map(|filter| ViewState {
        filter: filter.to_string(),
        sort: None,
        columns: Column::DEFAULT_SHOWN.to_vec(),
        glyph_columns: Vec::new(),
        abbreviated: Column::DEFAULT_ABBREVIATED.to_vec(),
        paint: Vec::new(),
        group: Grouping::flat(),
        pin_top: true,
        row_layout: crate::row_layout::RowLayout::default(),
        name: String::new(),
    })
    .collect()
}

#[derive(Clone)]
pub struct ViewStore {
    /// What columns exist while this store reads, writes, and names its slots.
    /// Shared rather than owned: `App` rebuilds one registry per repo load and
    /// both its stores read that same answer.
    registry: Arc<ColumnRegistry>,
    global_source: SourceGuard,
    repo_source: SourceGuard,
    global_path: Option<PathBuf>,
    repo_path: Option<PathBuf>,
    global: Vec<ViewState>,
    /// Zero-based slot -> this repo's override.
    repo: BTreeMap<usize, ViewState>,
}

impl ViewStore {
    /// Reads both files; anything missing or broken falls back and is reported.
    pub fn load(
        registry: Arc<ColumnRegistry>,
        global_path: Option<PathBuf>,
        repo_path: Option<PathBuf>,
    ) -> (ViewStore, Vec<String>) {
        Self::load_with_defaults(registry, global_path, repo_path, starter_views())
    }

    pub fn load_with_defaults(
        registry: Arc<ColumnRegistry>,
        global_path: Option<PathBuf>,
        repo_path: Option<PathBuf>,
        defaults: Vec<ViewState>,
    ) -> (ViewStore, Vec<String>) {
        let mut warnings = Vec::new();
        let mut dropped: Vec<String> = Vec::new();
        let mut global_source = SourceGuard::capture(global_path.as_deref());
        let mut repo_source = SourceGuard::capture(repo_path.as_deref());
        let global = match global_path
            .as_deref()
            .map(|path| read_lua(path, |table| parse_sequence(table, &registry, &mut dropped)))
        {
            Some(Ok(Some(views))) if !views.is_empty() => views,
            Some(Ok(_)) | None => defaults.clone(),
            Some(Err(error)) => {
                global_source.blocked = true;
                warnings.push(error);
                defaults.clone()
            }
        };
        let repo = match repo_path.as_deref().map(|path| {
            read_lua(path, |table| {
                parse_overrides(table, &registry, &mut dropped)
            })
        }) {
            Some(Ok(Some(overrides))) => overrides,
            Some(Ok(None)) | None => BTreeMap::new(),
            Some(Err(error)) => {
                repo_source.blocked = true;
                warnings.push(error);
                BTreeMap::new()
            }
        };
        if !dropped.is_empty() {
            warnings.push(format!(
                "this repo no longer declares {}; dropped from the saved view",
                dropped.join(", ")
            ));
        }
        let store = ViewStore {
            registry,
            global_source,
            repo_source,
            global_path,
            repo_path,
            global,
            repo,
        };
        (store, warnings)
    }

    pub fn load_for_page(
        registry: Arc<ColumnRegistry>,
        global: Option<PathBuf>,
        repo: Option<PathBuf>,
        page: crate::page::Page,
    ) -> (Self, Vec<String>) {
        let Some(scope) = ListSettings::for_page(page) else {
            return Self::load_with_defaults(registry, None, None, Vec::new());
        };
        let (mut store, warnings) = Self::load_with_defaults(
            Arc::clone(&registry),
            global.map(|p| scope.path(&p)),
            repo.map(|p| scope.path(&p)),
            scope.defaults(),
        );
        let mut warnings = warnings;
        let global_unsupported = store
            .global
            .iter()
            .any(|v| v.unsupported_on(scope, &registry));
        let repo_unsupported = store
            .repo
            .values()
            .any(|v| v.unsupported_on(scope, &registry));
        store.global_source.blocked |= global_unsupported;
        store.repo_source.blocked |= repo_unsupported;
        if global_unsupported || repo_unsupported {
            warnings.push("saved views contain settings unsupported on this page; source preserved, repair and reopen before saving".into());
        }
        store.sanitize(page);
        (store, warnings)
    }

    pub fn sanitize(&mut self, page: crate::page::Page) {
        let registry = Arc::clone(&self.registry);
        for state in &mut self.global {
            state.sanitize(page, &registry);
        }
        for state in self.repo.values_mut() {
            state.sanitize(page, &registry);
        }
    }

    /// What columns existed when this store was loaded.
    pub fn registry(&self) -> &Arc<ColumnRegistry> {
        &self.registry
    }

    /// Take up the registry `App` rebuilt for a repo load, so naming and saving
    /// a slot read the same catalog the table does.
    pub fn set_registry(&mut self, registry: Arc<ColumnRegistry>) {
        self.registry = registry;
    }

    /// The slots as the user sees them: repo overrides win, global fills the rest.
    pub fn slots(&self) -> Vec<(usize, ViewState, Scope)> {
        (0..MAX_SLOTS)
            .filter_map(|slot| match (self.repo.get(&slot), self.global.get(slot)) {
                (Some(view), _) => Some((slot, view.clone(), Scope::Repo)),
                (None, Some(view)) => Some((slot, view.clone(), Scope::Global)),
                (None, None) => None,
            })
            .collect()
    }

    pub fn get(&self, slot: usize) -> Option<ViewState> {
        self.repo
            .get(&slot)
            .or_else(|| self.global.get(slot))
            .cloned()
    }

    pub fn len(&self) -> usize {
        self.slots().len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots().is_empty()
    }

    /// Saves into this repo's overrides and writes the repo file.
    pub fn save_repo(&mut self, slot: usize, view: ViewState) -> Result<(), String> {
        self.repo_source.check(self.repo_path.as_deref())?;
        let mut next = self.clone();
        next.repo.insert(slot, view);
        next.write_repo()?;
        next.repo_source = SourceGuard::capture(next.repo_path.as_deref());
        *self = next;
        Ok(())
    }

    /// Copies the effective slot into the global file and drops the repo override.
    pub fn promote(&mut self, slot: usize) -> Result<(), String> {
        self.global_source.check(self.global_path.as_deref())?;
        self.repo_source.check(self.repo_path.as_deref())?;
        let mut next = self.clone();
        next.prepare_promotion(slot)?;
        next.write_global()?;
        self.global = next.global.clone();
        self.global_source = SourceGuard::capture(self.global_path.as_deref());
        // A successful global write is confirmed state even if the second file
        // cannot be written. Keep the old override until its removal succeeds.
        next.repo_source
            .check(next.repo_path.as_deref())
            .and_then(|()| next.write_repo())
            .map_err(|error| {
                format!("global saved; repo override retained; retry after repair: {error}")
            })?;
        next.global_source = self.global_source.clone();
        next.repo_source = SourceGuard::capture(next.repo_path.as_deref());
        *self = next;
        Ok(())
    }

    /// Names a slot wherever its effective definition lives: the repo file if
    /// this repo overrides that slot, otherwise the global file.
    pub fn set_name(&mut self, slot: usize, name: String) -> Result<(), String> {
        if self.repo.contains_key(&slot) {
            self.repo_source.check(self.repo_path.as_deref())?;
            let mut next = self.clone();
            let mut view = next
                .repo
                .get(&slot)
                .cloned()
                .ok_or_else(|| format!("no view in slot {}", slot + 1))?;
            view.name = name;
            next.repo.insert(slot, view);
            next.write_repo()?;
            next.repo_source = SourceGuard::capture(next.repo_path.as_deref());
            *self = next;
            Ok(())
        } else if let Some(mut view) = self.global.get(slot).cloned() {
            self.global_source.check(self.global_path.as_deref())?;
            let mut next = self.clone();
            view.name = name;
            next.global[slot] = view;
            next.write_global()?;
            next.global_source = SourceGuard::capture(next.global_path.as_deref());
            *self = next;
            Ok(())
        } else {
            Err(format!("no view in slot {}", slot + 1))
        }
    }

    /// Removes a slot wherever it lives: the repo override if this repo has
    /// one, otherwise the global entry (shifting later global slots down,
    /// the natural consequence of the global list's contiguous numbering).
    pub fn delete(&mut self, slot: usize) -> Result<(), String> {
        if self.repo.contains_key(&slot) {
            self.repo_source.check(self.repo_path.as_deref())?;
            let mut next = self.clone();
            next.repo.remove(&slot);
            next.write_repo()?;
            next.repo_source = SourceGuard::capture(next.repo_path.as_deref());
            *self = next;
            Ok(())
        } else if slot < self.global.len() {
            self.global_source.check(self.global_path.as_deref())?;
            let mut next = self.clone();
            next.global.remove(slot);
            next.write_global()?;
            next.global_source = SourceGuard::capture(next.global_path.as_deref());
            *self = next;
            Ok(())
        } else {
            Err(format!("no view in slot {}", slot + 1))
        }
    }

    fn prepare_promotion(&mut self, slot: usize) -> Result<(), String> {
        let selected = self
            .get(slot)
            .ok_or_else(|| format!("no view in slot {}", slot + 1))?;
        // Global records require contiguous slots; repo records may have holes.
        // Fill only from actual preceding views, never invented defaults.
        if slot >= self.global.len() {
            let additions = (self.global.len()..=slot)
                .map(|index| {
                    self.get(index).ok_or_else(|| {
                        "promote preceding slots first; global slots cannot have gaps".to_string()
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            self.global.extend(additions);
        } else {
            self.global[slot] = selected;
        }
        self.repo.remove(&slot);
        Ok(())
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
            text.push_str(&format!("  {},\n", lua_view(view, &self.registry)));
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
            text.push_str(&format!(
                "  [{}] = {},\n",
                slot + 1,
                lua_view(view, &self.registry)
            ));
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

fn parse_sequence(
    table: &Table,
    registry: &ColumnRegistry,
    dropped: &mut Vec<String>,
) -> Result<Vec<ViewState>, String> {
    let slots = parse_overrides(table, registry, dropped)?;
    let mut views = Vec::with_capacity(slots.len());
    for (expected, (slot, view)) in slots.into_iter().enumerate() {
        if expected != slot {
            return Err("global view slots must be contiguous from slot 1".into());
        }
        views.push(view);
    }
    Ok(views)
}

fn parse_overrides(
    table: &Table,
    registry: &ColumnRegistry,
    dropped: &mut Vec<String>,
) -> Result<BTreeMap<usize, ViewState>, String> {
    let mut views = BTreeMap::new();
    for pair in table.pairs::<usize, Table>().take(MAX_SLOTS + 1) {
        let (slot, entry) = pair.map_err(|e| e.to_string())?;
        if !(1..=MAX_SLOTS).contains(&slot) {
            return Err("unsupported view slot".into());
        }
        views.insert(slot - 1, parse_view(&entry, registry, dropped)?);
    }
    Ok(views)
}

impl ViewState {
    /// The Lua record form, `{ filter = "...", sort = "...", ... }`.
    pub fn to_lua(&self, registry: &ColumnRegistry) -> String {
        lua_view(self, registry)
    }

    /// Parses the record form; anything unreadable yields the default state.
    pub fn from_lua(text: &str, registry: &ColumnRegistry) -> ViewState {
        let lua = Lua::new();
        let mut dropped = Vec::new();
        lua.load(format!("return {text}"))
            .eval::<Table>()
            .ok()
            .and_then(|table| parse_view(&table, registry, &mut dropped).ok())
            .unwrap_or_default()
    }
}

impl ViewState {
    pub fn sanitize(&mut self, page: crate::page::Page, registry: &ColumnRegistry) {
        let Some(scope) = ListSettings::for_page(page) else {
            return;
        };
        let catalog = scope.catalog(registry);
        let canonical = |column| scope.canonical(column);
        self.columns = self
            .columns
            .iter()
            .copied()
            .map(canonical)
            .filter(|c| catalog.contains(c))
            .collect();
        let mut seen = Vec::new();
        self.columns.retain(|column| {
            if seen.contains(column) {
                false
            } else {
                seen.push(*column);
                true
            }
        });
        if self.columns.is_empty() {
            self.columns = scope.default_columns().to_vec();
        }
        self.glyph_columns = self
            .glyph_columns
            .iter()
            .copied()
            .map(canonical)
            .filter(|c| catalog.contains(c) && c.filter_field(registry).is_some())
            .collect();
        self.sort = self
            .sort
            .map(|sort| Sort {
                column: canonical(sort.column),
                ..sort
            })
            .filter(|sort| catalog.contains(&sort.column));
        self.paint.retain_mut(|rule| match rule {
            PaintRule::ByColumn { column, .. } | PaintRule::Column { column, .. } => {
                *column = canonical(*column);
                catalog.contains(column)
            }
            PaintRule::Rows { .. } => true,
        });
        if !scope.supports_row_layout() {
            self.row_layout = crate::row_layout::RowLayout::default();
        }
        // Flat when the page has no sections at all, and flat when a level
        // names a column this page no longer offers — a declared field that
        // left `backlog/config.yml` takes its section level with it.
        let ungroupable = self
            .group
            .levels()
            .iter()
            .any(|column| !catalog.contains(column));
        if !scope.supports_grouping() || ungroupable {
            self.group = Grouping::flat();
        }
        if !scope.supports_top_list() {
            self.pin_top = false;
        }
        if !scope.supports_abbreviation() {
            self.abbreviated.clear();
        }
        self.abbreviated.retain(|column| catalog.contains(column));
    }

    pub fn pull_requests() -> Self {
        Self {
            columns: Column::PR_DEFAULT.to_vec(),
            abbreviated: Vec::new(),
            pin_top: false,
            ..Self::default()
        }
    }
}

impl Default for ViewState {
    fn default() -> ViewState {
        ViewState {
            filter: String::new(),
            sort: None,
            columns: Column::DEFAULT_SHOWN.to_vec(),
            glyph_columns: Vec::new(),
            abbreviated: Column::DEFAULT_ABBREVIATED.to_vec(),
            paint: Vec::new(),
            group: Grouping::flat(),
            pin_top: true,
            row_layout: crate::row_layout::RowLayout::default(),
            name: String::new(),
        }
    }
}

fn parse_view(
    entry: &Table,
    registry: &ColumnRegistry,
    dropped: &mut Vec<String>,
) -> Result<ViewState, String> {
    validate_view(entry, registry)?;
    let field = |key: &str| -> Result<String, String> {
        entry
            .get::<Option<String>>(key)
            .map(Option::unwrap_or_default)
            .map_err(|e| e.to_string())
    };
    let columns = field("columns")?;
    let glyphs = field("glyphs")?;
    let sort = field("sort")?;
    let group = field("group")?;
    let paint = field("paint")?;
    let abbreviated = entry
        .get::<Option<String>>("abbreviated")
        .map_err(|e| e.to_string())?;
    // Which of this view's declared-field references the repo has since
    // dropped. Everything that resolves stays; see `prune_filter` for why the
    // filter is pruned against this list rather than judged on its own.
    let orphans = undeclared_fields_in(
        &[
            columns.as_str(),
            glyphs.as_str(),
            &sort,
            &group,
            &paint,
            abbreviated.as_deref().unwrap_or_default(),
        ]
        .join(" "),
        registry,
    );
    for name in &orphans {
        if !dropped.contains(name) {
            dropped.push(name.clone());
        }
    }
    Ok(ViewState {
        filter: prune_filter(&field("filter")?, &orphans),
        sort: Sort::parse(&sort, registry),
        columns: parse_columns(&columns, registry),
        glyph_columns: glyphs
            .split(',')
            .filter_map(|name| registry.parse(name))
            .collect(),
        paint: parse_rules(&paint, registry),
        group: prune_group(&group, registry).unwrap_or_default(),
        pin_top: entry
            .get::<Option<bool>>("pin")
            .map_err(|e| e.to_string())?
            .unwrap_or(true),
        // Absent means the default set; an explicit "" means none.
        abbreviated: match abbreviated {
            None => Column::DEFAULT_ABBREVIATED.to_vec(),
            Some(text) => text
                .split(',')
                .filter_map(|name| registry.parse(name))
                .filter(|column| column.abbreviable())
                .collect(),
        },
        name: field("name")?,
        row_layout: crate::row_layout::RowLayout::from_lua(entry)?,
    })
}

/// Every `field:<name>` in `text` naming a field the repo no longer declares.
/// Reads the prefix directly rather than tokenizing, because the same prefix
/// appears inside a sort (`field:x:semantic`), a paint rule (`by:field:x=…`)
/// and a plain list, and only sbt ever writes it.
fn undeclared_fields_in(text: &str, registry: &ColumnRegistry) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(crate::columns::FIELD_PREFIX) {
        rest = &rest[at + crate::columns::FIELD_PREFIX.len()..];
        let end = rest
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
            .unwrap_or(rest.len());
        let (name, tail) = rest.split_at(end);
        rest = tail;
        let reference = format!("{}{name}", crate::columns::FIELD_PREFIX);
        if registry.unknown_field_name(&reference) && !names.iter().any(|seen| seen == name) {
            names.push(name.to_string());
        }
    }
    names
}

/// A saved filter minus any `name:value` term naming one of `orphans`. Left in
/// place, such a term would silently match nothing and hide the whole list.
///
/// Pruned against the view's own dropped columns rather than judged term by
/// term: a filter is typed by hand and saved verbatim, so an unrecognized
/// keyword is just as likely to be a bare text search (`labels:ui`) as a field
/// that has gone. The view naming the same field under `columns::FIELD_PREFIX`
/// elsewhere is what makes the answer certain.
fn prune_filter(text: &str, orphans: &[String]) -> String {
    if orphans.is_empty() {
        return text.to_string();
    }
    text.split_whitespace()
        .filter(|word| {
            word.split_once(':').is_none_or(|(keyword, _)| {
                !orphans.iter().any(|name| name == &keyword.to_lowercase())
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A saved grouping minus any level naming a field this repo no longer declares;
/// the levels that survive keep their order, and losing every level is flat.
fn prune_group(text: &str, registry: &ColumnRegistry) -> Option<Grouping> {
    if let Some(grouping) = Grouping::parse(text, registry) {
        return Some(grouping);
    }
    let kept: Vec<&str> = text
        .split([',', '\u{203a}'])
        .map(str::trim)
        .filter(|name| !registry.unknown_field_name(name))
        .collect();
    Grouping::parse(&kept.join(","), registry)
}

fn validate_view(entry: &Table, registry: &ColumnRegistry) -> Result<(), String> {
    const KEYS: [&str; 11] = [
        "filter",
        "sort",
        "columns",
        "glyphs",
        "paint",
        "group",
        "pin",
        "abbreviated",
        "name",
        "title_lines",
        "row_spacing",
    ];
    for pair in entry.pairs::<String, mlua::Value>().take(KEYS.len() + 1) {
        let (key, _) = pair.map_err(|e| e.to_string())?;
        if !KEYS.contains(&key.as_str()) {
            return Err(format!("unsupported view field: {key}"));
        }
    }
    for key in ["columns", "glyphs", "abbreviated"] {
        let text = entry
            .get::<Option<String>>(key)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        // A name this repo no longer declares is dropped with a note, not an
        // error: the field left `backlog/config.yml`, the file is not broken.
        if text
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .any(|s| registry.parse(s).is_none() && !registry.unknown_field_name(s))
        {
            return Err(format!("unsupported {key} column; source preserved"));
        }
    }
    validate_view_rules(entry, registry)
}

fn validate_view_rules(entry: &Table, registry: &ColumnRegistry) -> Result<(), String> {
    let field = |key| {
        entry
            .get::<Option<String>>(key)
            .map(|v| v.unwrap_or_default())
            .map_err(|e| e.to_string())
    };
    let sort = field("sort")?;
    if !sort.is_empty()
        && Sort::parse(&sort, registry).is_none()
        && !names_an_undeclared_field(&sort, registry)
    {
        return Err("unsupported saved sort".into());
    }
    let group = field("group")?;
    if prune_group(&group, registry).is_none() {
        return Err("unsupported saved grouping".into());
    }
    if field("paint")?
        .split(';')
        .filter(|s| !s.trim().is_empty())
        .any(|s| !valid_saved_paint(s, registry) && !names_an_undeclared_field(s, registry))
    {
        return Err("unsupported saved paint".into());
    }
    Ok(())
}

/// Whether the only thing wrong with a saved sort or paint rule is that it names
/// a field this repo no longer declares — the case that degrades rather than
/// blocking the file. `by:counterparty=nick:yellow` and `counterparty:semantic`
/// both hide their column name between a prefix and a separator.
fn names_an_undeclared_field(text: &str, registry: &ColumnRegistry) -> bool {
    !undeclared_fields_in(text, registry).is_empty()
}

fn valid_saved_paint(text: &str, registry: &ColumnRegistry) -> bool {
    match PaintRule::parse(text, registry) {
        Some(PaintRule::ByColumn { colors, .. }) => text
            .split_once('=')
            .is_some_and(|(_, rhs)| rhs.is_empty() || colors.len() == rhs.split(',').count()),
        Some(_) => true,
        None => false,
    }
}

impl ViewState {
    fn unsupported_on(&self, scope: ListSettings, registry: &ColumnRegistry) -> bool {
        let catalog = scope.catalog(registry);
        let unsupported = |column| !catalog.contains(&scope.canonical(column));
        self.columns
            .iter()
            .chain(&self.glyph_columns)
            .any(|c| unsupported(*c))
            || self
                .glyph_columns
                .iter()
                .any(|c| scope.canonical(*c).filter_field(registry).is_none())
            || Filter::parse(&self.filter, registry)
                .fields()
                .any(|field| unsupported(field.column()))
            || self.sort.is_some_and(|s| unsupported(s.column))
            || (!scope.supports_row_layout()
                && self.row_layout != crate::row_layout::RowLayout::default())
            || (!scope.supports_grouping() && !self.group.is_flat())
            || self.paint.iter().any(|rule| match rule {
                PaintRule::ByColumn { column, .. } | PaintRule::Column { column, .. } => {
                    unsupported(*column)
                }
                PaintRule::Rows { .. } => false,
            })
    }
}

fn lua_view(view: &ViewState, registry: &ColumnRegistry) -> String {
    let glyphs = if view.glyph_columns.is_empty() {
        String::new()
    } else {
        format!(
            ", glyphs = {}",
            lua_string(&columns_save_text(&view.glyph_columns, registry))
        )
    };
    let paint = if view.paint.is_empty() {
        String::new()
    } else {
        format!(
            ", paint = {}",
            lua_string(&rules_text(&view.paint, registry))
        )
    };
    let pin = if view.pin_top {
        String::new()
    } else {
        ", pin = false".to_string()
    };
    let abbreviated = if view.abbreviated_label(registry).is_none() {
        String::new()
    } else {
        format!(
            ", abbreviated = {}",
            lua_string(&columns_save_text(&view.abbreviated, registry))
        )
    };
    let group = if view.group.is_flat() {
        String::new()
    } else {
        format!(", group = {}", lua_string(&view.group.text(registry)))
    };
    let row_layout = view.row_layout.to_lua();
    let name = if view.name.is_empty() {
        String::new()
    } else {
        format!(", name = {}", lua_string(&view.name))
    };
    format!(
        "{{ filter = {}, sort = {}, columns = {}{glyphs}{paint}{group}{abbreviated}{pin}{name}{row_layout} }}",
        lua_string(&view.filter),
        lua_string(
            &view
                .sort
                .map(|sort| sort.to_text(registry))
                .unwrap_or_default()
        ),
        lua_string(&columns_save_text(&view.columns, registry)),
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

/// Preserve unreadable files and changes made after startup. Recovery is an
/// explicit external repair followed by reopening, never an implicit overwrite.
#[derive(Clone)]
struct SourceGuard {
    source: Result<Option<String>, String>,
    blocked: bool,
}

impl SourceGuard {
    fn capture(path: Option<&Path>) -> Self {
        let source = path
            .map(|p| match std::fs::read_to_string(p) {
                Ok(text) => Ok(Some(text)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.to_string()),
            })
            .unwrap_or(Ok(None));
        Self {
            source,
            blocked: false,
        }
    }

    fn check(&self, path: Option<&Path>) -> Result<(), String> {
        if self.blocked || self.source.is_err() {
            return Err(
                "saved views unreadable or unsupported; repair file and reopen before saving"
                    .into(),
            );
        }
        if self.source != Self::capture(path).source {
            return Err("saved views changed on disk; reopen before saving".into());
        }
        Ok(())
    }
}
