//! Stack ranking - `backlog/ranking.yml`, one records file per repo
//! (trajectory: *Stack ranking*, owner-approved 2026-08-31).
//!
//! Manual rank is **hierarchy-shaped with a named exception lane**: siblings
//! rank within their parent scope (projects against projects in the repo,
//! tasks within their project, repo-root tasks as one sibling group,
//! sub-issues within their parent task), and a short `expedite` lane of task
//! ids jumps the entire computed order - true cross-project interrupts only.
//! Ranking is **sparse**: only explicitly ranked items float, in rank order,
//! above the unranked rest, which keeps sorting by [`compare_tasks`]
//! (status - priority - id). The repo-wide "next up" order is *computed*
//! by [`sort_tasks`], never stored - the roll-up discipline.
//!
//! Rank deliberately does NOT live in task frontmatter: inserting at a
//! position would mass-rewrite every file below it, breaking the
//! byte-surgical write discipline. This file is distinct from the hub repo's
//! root-level `ordering.yml` (`crate::backlog_triage::OrderingOverlay`, the
//! cross-repo triage overlay): that one ranks `repo:task` pairs across
//! repos, this one ranks siblings within one repo - hence the different
//! name, so a grep for either concept finds exactly one authority.
//!
//! Reads are **tolerant** (goals.yml posture): a missing file is an empty
//! ranking, a malformed file warns and loads empty, and entries naming
//! done/archived/missing ids - or ids whose scope has changed - are ignored
//! at sort time and pruned on the next write to their scope. Writes are
//! **line-surgical** over the file this module itself emits: an edit
//! rewrites only the affected scope's block, every other byte survives, and
//! a hand-restyled file this module cannot confidently locate its edit
//! point in fails closed with an error naming the fix, never a rewrite.

use super::parse::{compare_tasks, load_backlog_repo, source_rank, status_rank};
use super::types::{BacklogRepo, BacklogTask, BacklogTaskSource};
use super::write::{atomic_write, validated_single_line, yaml_scalar, WriteOutcome};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

const RANKING_REL: &str = "backlog/ranking.yml";

/// Walking a sub-issue chain up to its project is bounded: ids nest one
/// decimal level in practice (`TASK-7.2`), so a chain longer than this is a
/// data cycle, not a hierarchy.
const MAX_PARENT_HOPS: usize = 8;

/// The per-repo stack rank, as stored in `backlog/ranking.yml`. Every list
/// is highest-priority-first; absence from a list means "unranked, fall back
/// to the computed comparator".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoRanking {
    /// Task ids that jump the entire computed order - the exception lane.
    pub expedite: Vec<String>,
    /// Project names ranked against each other within the repo.
    pub projects: Vec<String>,
    /// Ranked task ids per project name (the sibling scope of a task whose
    /// `project:` names that project).
    pub tasks: BTreeMap<String, Vec<String>>,
    /// Ranked ids of tasks with no project and no parent - the repo-root
    /// sibling group.
    pub root_tasks: Vec<String>,
    /// Ranked sub-issue ids per parent task id.
    pub subissues: BTreeMap<String, Vec<String>>,
}

impl RepoRanking {
    pub fn is_empty(&self) -> bool {
        self.expedite.is_empty()
            && self.projects.is_empty()
            && self.tasks.is_empty()
            && self.root_tasks.is_empty()
            && self.subissues.is_empty()
    }

    /// Whether the lane names this id. Stale lane entries are pruned on
    /// write, so surfaces may badge directly off this.
    pub fn is_expedited(&self, task_id: &str) -> bool {
        self.expedite.iter().any(|id| id == task_id)
    }

    /// Explicit rank of a project name (exact match, the same identity rule
    /// as project membership everywhere else). Lower is higher priority.
    pub fn project_rank(&self, name: &str) -> Option<usize> {
        self.projects.iter().position(|p| p == name)
    }

    /// The task's raw position in its scope's ranked list; `None` means
    /// unranked. Raw, not pruned - a stale earlier entry can inflate the
    /// index, so use this for affordance state (arrow enablement, badges),
    /// never for placement math: the move/rank write ops re-derive the
    /// pruned truth themselves and no-op harmlessly on a stale click.
    pub fn task_rank_position(&self, task: &BacklogTask) -> Option<usize> {
        let list = match scope_of(task) {
            TaskScope::Project(name) => self.tasks.get(&name)?.as_slice(),
            TaskScope::Root => self.root_tasks.as_slice(),
            TaskScope::Subissue(parent) => self.subissues.get(&parent)?.as_slice(),
        };
        list.iter().position(|entry| entry == &task.id)
    }
}

/// Where to insert an item within its sibling scope's ranked list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RankPlacement {
    Top,
    Before(String),
    After(String),
}

// ---- reading ----

#[derive(Deserialize, Default)]
struct RankingFileSer {
    #[serde(default)]
    expedite: Vec<String>,
    #[serde(default)]
    projects: Vec<String>,
    #[serde(default)]
    tasks: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    root_tasks: Vec<String>,
    #[serde(default)]
    subissues: BTreeMap<String, Vec<String>>,
}

/// Load `backlog/ranking.yml`. Never fails the repo load: missing file is an
/// empty ranking; a malformed file warns and loads empty.
pub(super) fn load_ranking(root: &Path, warnings: &mut Vec<String>) -> RepoRanking {
    let path = root.join(RANKING_REL);
    let Ok(text) = fs::read_to_string(&path) else {
        return RepoRanking::default();
    };
    match serde_yaml::from_str::<RankingFileSer>(&text) {
        Ok(parsed) => RepoRanking {
            expedite: parsed.expedite,
            projects: parsed.projects,
            tasks: parsed.tasks,
            root_tasks: parsed.root_tasks,
            subissues: parsed.subissues,
        },
        Err(err) => {
            warnings.push(format!("{}: {err}", path.display()));
            RepoRanking::default()
        }
    }
}

// ---- the computed order ----

/// A task's sibling scope - the list within `RepoRanking` it ranks in.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TaskScope {
    Project(String),
    Root,
    Subissue(String),
}

fn scope_of(task: &BacklogTask) -> TaskScope {
    if let Some(parent) = parent_id(task) {
        return TaskScope::Subissue(parent);
    }
    match &task.project {
        Some(project) => TaskScope::Project(project.clone()),
        None => TaskScope::Root,
    }
}

/// A task's parent id: the `parent_task_id` frontmatter when present, else
/// derived from a decimal id (`TASK-7.2` -> `TASK-7`).
fn parent_id(task: &BacklogTask) -> Option<String> {
    if let Some(parent) = &task.parent {
        return Some(parent.clone());
    }
    task.id
        .rsplit_once('.')
        .map(|(parent, _)| parent.to_string())
}

/// Only active, unfinished tasks hold a rank - a Done/completed/archived/
/// draft entry is stale, ignored at sort time and pruned on the next write.
fn rankable(task: &BacklogTask) -> bool {
    task.source == BacklogTaskSource::Active && !task.is_done()
}

/// Sort `tasks` into the computed repo-wide order. Rank applies *within* the
/// source/status tiers of [`compare_tasks`] - an expedited To Do task leads
/// every other To Do task but never floats above In Progress work, and Done
/// tasks are untouched by rank entirely. Within a tier the order is:
/// expedite lane position, then project rank, then a walk down the two
/// tasks' ancestor chains comparing *true siblings only* at each level
/// (sibling rank, falling back to today's comparator), with an ancestor
/// always ahead of its own descendants. A sibling rank is never compared
/// across scopes - a sub-issue's rank among its siblings says nothing about
/// where its parent stands among *its* siblings. Fully unranked repos keep
/// today's comparator exactly; a partially ranked repo additionally groups
/// sub-issues under their parent, which is what "flatten the hierarchy
/// top-down" means once any rank exists.
pub(super) fn sort_tasks(tasks: &mut [BacklogTask], ranking: &RepoRanking) {
    if ranking.is_empty() {
        tasks.sort_by(compare_tasks);
        return;
    }

    let by_id: HashMap<String, BacklogTask> = tasks
        .iter()
        .map(|task| (task.id.clone(), task.clone()))
        .collect();
    let expedite: HashMap<&str, usize> = ranking
        .expedite
        .iter()
        .filter(|id| by_id.get(id.as_str()).is_some_and(rankable))
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let sibling: HashMap<String, usize> = sibling_ranks(&by_id, ranking);
    let chains: HashMap<String, Vec<String>> = tasks
        .iter()
        .map(|task| (task.id.clone(), ancestor_chain(task, &by_id)))
        .collect();
    let project_tier = |task: &BacklogTask| -> usize {
        effective_project(task, &by_id)
            .and_then(|name| ranking.project_rank(&name))
            .unwrap_or(usize::MAX)
    };

    tasks.sort_by(|a, b| {
        source_rank(a.source)
            .cmp(&source_rank(b.source))
            .then_with(|| status_rank(&a.status).cmp(&status_rank(&b.status)))
            .then_with(|| {
                expedite
                    .get(a.id.as_str())
                    .unwrap_or(&usize::MAX)
                    .cmp(expedite.get(b.id.as_str()).unwrap_or(&usize::MAX))
            })
            .then_with(|| project_tier(a).cmp(&project_tier(b)))
            .then_with(|| compare_chains(&chains[&a.id], &chains[&b.id], &sibling, &by_id))
            .then_with(|| compare_tasks(a, b))
    });
}

/// Each rankable task's position among its *own* scope's ranked siblings
/// (filtered to live entries, so stale ids neither rank nor leave gaps);
/// absent means unranked.
fn sibling_ranks(
    by_id: &HashMap<String, BacklogTask>,
    ranking: &RepoRanking,
) -> HashMap<String, usize> {
    let mut ranks = HashMap::new();
    let mut absorb = |list: &[String], scope: TaskScope| {
        let live = list.iter().filter(|id| {
            by_id
                .get(id.as_str())
                .is_some_and(|task| rankable(task) && scope_of(task) == scope)
        });
        for (index, id) in live.enumerate() {
            ranks.entry(id.clone()).or_insert(index);
        }
    };
    absorb(&ranking.root_tasks, TaskScope::Root);
    for (name, list) in &ranking.tasks {
        absorb(list, TaskScope::Project(name.clone()));
    }
    for (parent, list) in &ranking.subissues {
        absorb(list, TaskScope::Subissue(parent.clone()));
    }
    ranks
}

/// Ids from a task's highest *present* ancestor down to itself. Bounded by
/// [`MAX_PARENT_HOPS`] so a cyclic `parent_task_id` chain degrades to a
/// truncated chain instead of hanging the load.
fn ancestor_chain(task: &BacklogTask, by_id: &HashMap<String, BacklogTask>) -> Vec<String> {
    let mut chain = vec![task.id.clone()];
    let mut current = task;
    for _ in 0..MAX_PARENT_HOPS {
        match parent_id(current).and_then(|p| by_id.get(&p)) {
            Some(parent) => {
                chain.push(parent.id.clone());
                current = parent;
            }
            None => break,
        }
    }
    chain.reverse();
    chain
}

/// Lexicographic walk down two ancestor chains: at the first level where
/// they name different tasks, those two *are* siblings-or-cousins in the
/// flatten, so their sibling ranks (then today's comparator) decide; chains
/// where one is a prefix of the other put the ancestor first.
fn compare_chains(
    a: &[String],
    b: &[String],
    sibling: &HashMap<String, usize>,
    by_id: &HashMap<String, BacklogTask>,
) -> std::cmp::Ordering {
    for (a_id, b_id) in a.iter().zip(b.iter()) {
        if a_id == b_id {
            continue;
        }
        let rank = |id: &String| sibling.get(id).copied().unwrap_or(usize::MAX);
        return rank(a_id).cmp(&rank(b_id)).then_with(|| {
            match (by_id.get(a_id), by_id.get(b_id)) {
                (Some(ta), Some(tb)) => compare_tasks(ta, tb),
                _ => a_id.cmp(b_id),
            }
        });
    }
    a.len().cmp(&b.len())
}

/// The project a task sorts under: its own, or - for a sub-issue without one
/// (the common shape; children rarely repeat the parent's membership) - the
/// nearest ancestor's, so a ranked project's sub-issues ride its tier.
fn effective_project(task: &BacklogTask, by_id: &HashMap<String, BacklogTask>) -> Option<String> {
    let mut current = task;
    for _ in 0..MAX_PARENT_HOPS {
        if let Some(project) = &current.project {
            return Some(project.clone());
        }
        match parent_id(current).and_then(|p| by_id.get(&p)) {
            Some(parent) => current = parent,
            None => return None,
        }
    }
    None
}

// ---- writing ----
//
// The emitted shape, which the surgical edits below scan for:
//
//   expedite:
//     - TASK-91
//   projects:
//     - Stack Ranking
//   tasks:
//     Stack Ranking:
//       - TASK-82
//   root_tasks: []
//   subissues:
//     TASK-80:
//       - TASK-80.2
//
// An empty list collapses back to `key: []`, an empty map to `key: {}`.

const LIST_ITEM_INDENT: &str = "  ";
const SCOPE_KEY_INDENT: &str = "  ";
const SCOPE_ITEM_INDENT: &str = "    ";

fn ranking_path(root: &Path) -> PathBuf {
    root.join(RANKING_REL)
}

fn read_lines(path: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if text.contains('\r') {
        bail!("{} has CR line endings; refusing to edit", path.display());
    }
    Ok(text.lines().map(str::to_string).collect())
}

fn write_lines(path: &Path, lines: &[String]) -> Result<()> {
    let text = format!("{}\n", lines.join("\n"));
    if path.is_file() {
        return atomic_write(path, &text);
    }
    // First write: `atomic_write` preserves an existing file's permissions,
    // which a brand-new ranking.yml doesn't have - same tmp-then-rename
    // atomicity, default permissions.
    let tmp = path.with_extension("yml.tmp");
    fs::write(&tmp, &text).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("creating {}", path.display()))
}

fn skeleton() -> Vec<String> {
    vec![
        "# Stack ranking - written by switchbard (`rank` / `expedite` commands); records,"
            .to_string(),
        "# not documents - never hand-edit. See docs/product-trajectory.md (\"Stack ranking\")."
            .to_string(),
        "expedite: []".to_string(),
        "projects: []".to_string(),
        "tasks: {}".to_string(),
        "root_tasks: []".to_string(),
        "subissues: {}".to_string(),
    ]
}

/// Lines of the current file, or the fresh skeleton when it doesn't exist.
/// A file that exists but no longer parses fails closed - editing around
/// structure we cannot read risks compounding whatever broke it.
fn load_lines_for_edit(root: &Path) -> Result<Vec<String>> {
    let path = ranking_path(root);
    if !path.is_file() {
        fs::create_dir_all(path.parent().expect("ranking.yml has a parent"))
            .with_context(|| format!("creating {}", root.join("backlog").display()))?;
        return Ok(skeleton());
    }
    let mut warnings = Vec::new();
    load_ranking(root, &mut warnings);
    if !warnings.is_empty() {
        bail!(
            "{} does not parse cleanly - fix it (or remove it to start fresh): {}",
            path.display(),
            warnings.join("; ")
        );
    }
    read_lines(&path)
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// `[key_line, end)` of a top-level key's block: the key's own line through
/// the last line indented under it. Fails closed when the key is missing -
/// this module wrote the file, so a missing key means it was restyled.
fn top_level_span(lines: &[String], path: &Path, key: &str) -> Result<(usize, usize)> {
    let start = lines
        .iter()
        .position(|l| {
            indent_of(l) == 0 && (l.trim_end() == format!("{key}:") || l.starts_with(&format!("{key}: ")))
        })
        .with_context(|| {
            format!(
                "{} has no `{key}:` key - restore the emitted structure or remove the file to start fresh",
                path.display()
            )
        })?;
    let end = ((start + 1)..lines.len())
        .find(|&i| !lines[i].trim().is_empty() && indent_of(&lines[i]) == 0)
        .unwrap_or(lines.len());
    Ok((start, end))
}

/// Replace a top-level list key's block (`expedite`, `projects`,
/// `root_tasks`) with `items`, collapsing an empty list to `key: []`.
fn set_top_level_list(
    lines: &mut Vec<String>,
    path: &Path,
    key: &str,
    items: &[String],
) -> Result<()> {
    let (start, end) = top_level_span(lines, path, key)?;
    let mut block = Vec::with_capacity(items.len() + 1);
    if items.is_empty() {
        block.push(format!("{key}: []"));
    } else {
        block.push(format!("{key}:"));
        block.extend(
            items
                .iter()
                .map(|item| format!("{LIST_ITEM_INDENT}- {}", yaml_scalar(item))),
        );
    }
    lines.splice(start..end, block);
    Ok(())
}

/// Replace one scope's list under a top-level map key (`tasks`,
/// `subissues`). An emptied scope is removed; an emptied map collapses to
/// `key: {}`; a new scope inserts in key-sorted position so the file stays
/// deterministic whatever order ranks were assigned in.
fn set_scope_list(
    lines: &mut Vec<String>,
    path: &Path,
    map_key: &str,
    scope: &str,
    items: &[String],
) -> Result<()> {
    let (map_start, map_end) = top_level_span(lines, path, map_key)?;
    let scope_line = format!("{SCOPE_KEY_INDENT}{}:", yaml_scalar(scope));
    let scope_start = ((map_start + 1)..map_end).find(|&i| lines[i].trim_end() == scope_line);

    let mut block = Vec::with_capacity(items.len() + 1);
    if !items.is_empty() {
        block.push(scope_line.clone());
        block.extend(
            items
                .iter()
                .map(|item| format!("{SCOPE_ITEM_INDENT}- {}", yaml_scalar(item))),
        );
    }

    match scope_start {
        Some(start) => {
            let end = ((start + 1)..map_end)
                .find(|&i| indent_of(&lines[i]) <= SCOPE_KEY_INDENT.len())
                .unwrap_or(map_end);
            lines.splice(start..end, block);
        }
        None if items.is_empty() => return Ok(()),
        None => {
            // Insert before the first existing scope whose key sorts after
            // this one; scopes are at SCOPE_KEY_INDENT, items deeper.
            let insert_at = ((map_start + 1)..map_end)
                .find(|&i| {
                    indent_of(&lines[i]) == SCOPE_KEY_INDENT.len()
                        && lines[i].trim_end() > scope_line.as_str()
                })
                .unwrap_or(map_end);
            if lines[map_start].trim_end() == format!("{map_key}: {{}}") {
                lines.splice(map_start..map_start + 1, [format!("{map_key}:")]);
                lines.splice(map_start + 1..map_start + 1, block);
            } else {
                lines.splice(insert_at..insert_at, block);
            }
        }
    }

    // Collapse an emptied map back to `key: {}`.
    let (map_start, map_end) = top_level_span(lines, path, map_key)?;
    if map_end == map_start + 1 && lines[map_start].trim_end() == format!("{map_key}:") {
        lines[map_start] = format!("{map_key}: {{}}");
    }
    Ok(())
}

/// Retain only rankable ids that still live in `scope`, preserving order -
/// the prune half of "stale entries are ignored on read and pruned on the
/// next write to their scope".
fn pruned_scope_list(list: &[String], scope: &TaskScope, repo: &BacklogRepo) -> Vec<String> {
    list.iter()
        .filter(|id| {
            repo.tasks
                .iter()
                .find(|task| task.id == **id)
                .is_some_and(|task| rankable(task) && scope_of(task) == *scope)
        })
        .cloned()
        .collect()
}

fn insert_placed(
    list: &mut Vec<String>,
    id: &str,
    placement: &RankPlacement,
    scope_label: &str,
) -> Result<()> {
    let anchor_index = |anchor: &str| -> Result<usize> {
        if anchor.eq_ignore_ascii_case(id) {
            bail!("cannot rank {id} relative to itself");
        }
        list.iter()
            .position(|entry| entry.eq_ignore_ascii_case(anchor))
            .with_context(|| {
                format!("{anchor} is not ranked among {scope_label} - rank it first, or use --top")
            })
    };
    let at = match placement {
        RankPlacement::Top => 0,
        RankPlacement::Before(anchor) => anchor_index(anchor)?,
        RankPlacement::After(anchor) => anchor_index(anchor)? + 1,
    };
    list.insert(at, id.to_string());
    Ok(())
}

fn find_task<'r>(repo: &'r BacklogRepo, id: &str) -> Result<&'r BacklogTask> {
    repo.tasks
        .iter()
        .find(|task| task.id.eq_ignore_ascii_case(id))
        .with_context(|| format!("no task {id} - check `list --all` for the id"))
}

fn rankable_task<'r>(repo: &'r BacklogRepo, id: &str) -> Result<&'r BacklogTask> {
    let task = find_task(repo, id)?;
    if !rankable(task) {
        bail!(
            "{} is {} - only active, unfinished tasks can be ranked",
            task.id,
            if task.is_done() {
                "done"
            } else {
                task.source.label()
            }
        );
    }
    Ok(task)
}

/// The scope's stored list and the (map_key, scope_key) address to write it
/// back to; `None` map_key means a top-level list.
fn scope_address(scope: &TaskScope) -> (Option<&'static str>, String) {
    match scope {
        TaskScope::Project(name) => (Some("tasks"), name.clone()),
        TaskScope::Root => (None, "root_tasks".to_string()),
        TaskScope::Subissue(parent) => (Some("subissues"), parent.clone()),
    }
}

fn stored_scope_list<'r>(ranking: &'r RepoRanking, scope: &TaskScope) -> &'r [String] {
    match scope {
        TaskScope::Project(name) => ranking.tasks.get(name).map_or(&[], Vec::as_slice),
        TaskScope::Root => &ranking.root_tasks,
        TaskScope::Subissue(parent) => ranking.subissues.get(parent).map_or(&[], Vec::as_slice),
    }
}

fn write_scope_list(
    root: &Path,
    scope: &TaskScope,
    stored: &[String],
    updated: &[String],
) -> Result<WriteOutcome> {
    if stored == updated {
        return Ok(WriteOutcome::Unchanged);
    }
    let path = ranking_path(root);
    let mut lines = load_lines_for_edit(root)?;
    match scope_address(scope) {
        (Some(map_key), scope_key) => {
            set_scope_list(&mut lines, &path, map_key, &scope_key, updated)?
        }
        (None, key) => set_top_level_list(&mut lines, &path, &key, updated)?,
    }
    write_lines(&path, &lines)?;
    Ok(WriteOutcome::Changed)
}

/// Rank a task within its sibling scope (its project, the repo root, or its
/// parent's sub-issues), pruning stale ids from that scope as it writes.
pub fn rank_task(root: &Path, id: &str, placement: &RankPlacement) -> Result<WriteOutcome> {
    let repo = load_backlog_repo(root)?;
    let task = rankable_task(&repo, id)?;
    let scope = scope_of(task);
    let mut warnings = Vec::new();
    let ranking = load_ranking(root, &mut warnings);

    let stored = stored_scope_list(&ranking, &scope).to_vec();
    let mut updated = pruned_scope_list(&stored, &scope, &repo);
    updated.retain(|entry| entry != &task.id);
    let scope_label = match &scope {
        TaskScope::Project(name) => format!("project '{name}'s tasks"),
        TaskScope::Root => "the repo's root tasks".to_string(),
        TaskScope::Subissue(parent) => format!("{parent}'s sub-issues"),
    };
    insert_placed(&mut updated, &task.id, placement, &scope_label)?;
    write_scope_list(root, &scope, &stored, &updated)
}

/// Remove a task from its scope's ranked list. Unranking an id is valid
/// even when the task no longer exists (that is how a stray entry is
/// cleared by hand), so this searches every scope rather than resolving one.
pub fn unrank_task(root: &Path, id: &str) -> Result<WriteOutcome> {
    let mut warnings = Vec::new();
    let ranking = load_ranking(root, &mut warnings);
    let mut outcome = WriteOutcome::Unchanged;

    let mut scopes: Vec<(TaskScope, Vec<String>)> =
        vec![(TaskScope::Root, ranking.root_tasks.clone())];
    scopes.extend(
        ranking
            .tasks
            .iter()
            .map(|(name, list)| (TaskScope::Project(name.clone()), list.clone())),
    );
    scopes.extend(
        ranking
            .subissues
            .iter()
            .map(|(parent, list)| (TaskScope::Subissue(parent.clone()), list.clone())),
    );
    for (scope, stored) in scopes {
        let updated: Vec<String> = stored
            .iter()
            .filter(|entry| !entry.eq_ignore_ascii_case(id))
            .cloned()
            .collect();
        if write_scope_list(root, &scope, &stored, &updated)?.changed() {
            outcome = WriteOutcome::Changed;
        }
    }
    Ok(outcome)
}

/// Add a task to the expedite lane (at the end - the lane reads top-down
/// and stays short; reorder by unexpediting and re-expediting). Prunes
/// stale lane entries as it writes.
pub fn expedite_task(root: &Path, id: &str) -> Result<WriteOutcome> {
    let repo = load_backlog_repo(root)?;
    let task = rankable_task(&repo, id)?;
    let mut warnings = Vec::new();
    let ranking = load_ranking(root, &mut warnings);

    let mut updated: Vec<String> = ranking
        .expedite
        .iter()
        .filter(|entry| {
            repo.tasks
                .iter()
                .find(|t| t.id == **entry)
                .is_some_and(rankable)
        })
        .cloned()
        .collect();
    if !updated.iter().any(|entry| entry == &task.id) {
        updated.push(task.id.clone());
    }
    write_expedite(root, &ranking.expedite, &updated)
}

/// Remove a task from the expedite lane.
pub fn unexpedite_task(root: &Path, id: &str) -> Result<WriteOutcome> {
    let mut warnings = Vec::new();
    let ranking = load_ranking(root, &mut warnings);
    let updated: Vec<String> = ranking
        .expedite
        .iter()
        .filter(|entry| !entry.eq_ignore_ascii_case(id))
        .cloned()
        .collect();
    write_expedite(root, &ranking.expedite, &updated)
}

fn write_expedite(root: &Path, stored: &[String], updated: &[String]) -> Result<WriteOutcome> {
    if stored == updated {
        return Ok(WriteOutcome::Unchanged);
    }
    let path = ranking_path(root);
    let mut lines = load_lines_for_edit(root)?;
    set_top_level_list(&mut lines, &path, "expedite", updated)?;
    write_lines(&path, &lines)?;
    Ok(WriteOutcome::Changed)
}

/// One step of the GUI's reorder arrows. The semantics are sparse-rank
/// honest and live here so every surface shares one testable authority:
/// - **Up** on a ranked item swaps it with the ranked sibling above;
///   already-first is a no-op. Up on an *unranked* item enters the ranked
///   set at its bottom (rank is sparse - there is no "one above" inside
///   the unranked fallback tail to swap with).
/// - **Down** on a ranked item swaps it with the ranked sibling below;
///   down on the *last* ranked item removes its rank (it rejoins the
///   fallback tail). Down on an unranked item is a no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankMove {
    Up,
    Down,
}

fn moved_list(pruned: Vec<String>, id: &str, direction: RankMove) -> Vec<String> {
    let position = pruned.iter().position(|entry| entry == id);
    let mut updated = pruned;
    match (direction, position) {
        (RankMove::Up, Some(0)) | (RankMove::Down, None) => {}
        (RankMove::Up, Some(index)) => updated.swap(index, index - 1),
        (RankMove::Up, None) => updated.push(id.to_string()),
        (RankMove::Down, Some(index)) if index + 1 == updated.len() => {
            updated.remove(index);
        }
        (RankMove::Down, Some(index)) => updated.swap(index, index + 1),
    }
    updated
}

/// Move a task one step among its ranked scope siblings - see [`RankMove`]
/// for the exact arrow semantics. Prunes stale ids as it writes.
pub fn rank_task_move(root: &Path, id: &str, direction: RankMove) -> Result<WriteOutcome> {
    let repo = load_backlog_repo(root)?;
    let task = rankable_task(&repo, id)?;
    let scope = scope_of(task);
    let mut warnings = Vec::new();
    let ranking = load_ranking(root, &mut warnings);

    let stored = stored_scope_list(&ranking, &scope).to_vec();
    let updated = moved_list(
        pruned_scope_list(&stored, &scope, &repo),
        &task.id,
        direction,
    );
    write_scope_list(root, &scope, &stored, &updated)
}

/// [`rank_task_move`]'s project twin, over the repo's ranked project list.
pub fn rank_project_move(root: &Path, name: &str, direction: RankMove) -> Result<WriteOutcome> {
    let name = validated_single_line("project", name)?;
    let repo = load_backlog_repo(root)?;
    let live = live_project_names(&repo);
    let Some(canonical) = live.iter().find(|p| p.eq_ignore_ascii_case(name)).cloned() else {
        bail!("no project named '{name}' - check `project list` for the exact name");
    };
    let mut warnings = Vec::new();
    let ranking = load_ranking(root, &mut warnings);

    let pruned: Vec<String> = ranking
        .projects
        .iter()
        .filter(|entry| live.iter().any(|p| p == *entry))
        .cloned()
        .collect();
    let updated = moved_list(pruned, &canonical, direction);
    write_projects(root, &ranking.projects, &updated)
}

/// A project is a live rank target while any active, unfinished task names
/// it or a definition holds a non-terminal status - the same births-and-
/// deaths rule the hierarchy layer implies, applied to pruning.
fn live_project_names(repo: &BacklogRepo) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut push = |name: &str| {
        if !names.iter().any(|existing| existing == name) {
            names.push(name.to_string());
        }
    };
    for task in repo.tasks.iter().filter(|task| rankable(task)) {
        if let Some(project) = &task.project {
            push(project);
        }
    }
    for def in &repo.project_defs {
        if !matches!(def.status.as_str(), "Completed" | "Canceled") {
            push(&def.name);
        }
    }
    names
}

/// Rank a project against its repo siblings, pruning dead names as it
/// writes.
pub fn rank_project(root: &Path, name: &str, placement: &RankPlacement) -> Result<WriteOutcome> {
    let name = validated_single_line("project", name)?;
    let repo = load_backlog_repo(root)?;
    let live = live_project_names(&repo);
    let Some(canonical) = live.iter().find(|p| p.eq_ignore_ascii_case(name)).cloned() else {
        bail!("no project named '{name}' - check `project list` for the exact name");
    };
    let mut warnings = Vec::new();
    let ranking = load_ranking(root, &mut warnings);

    let mut updated: Vec<String> = ranking
        .projects
        .iter()
        .filter(|entry| live.iter().any(|p| p == *entry))
        .cloned()
        .collect();
    updated.retain(|entry| entry != &canonical);
    insert_placed(&mut updated, &canonical, placement, "the repo's projects")?;
    write_projects(root, &ranking.projects, &updated)
}

/// Remove a project from the ranked list.
pub fn unrank_project(root: &Path, name: &str) -> Result<WriteOutcome> {
    let name = validated_single_line("project", name)?;
    let mut warnings = Vec::new();
    let ranking = load_ranking(root, &mut warnings);
    let updated: Vec<String> = ranking
        .projects
        .iter()
        .filter(|entry| !entry.eq_ignore_ascii_case(name))
        .cloned()
        .collect();
    write_projects(root, &ranking.projects, &updated)
}

fn write_projects(root: &Path, stored: &[String], updated: &[String]) -> Result<WriteOutcome> {
    if stored == updated {
        return Ok(WriteOutcome::Unchanged);
    }
    let path = ranking_path(root);
    let mut lines = load_lines_for_edit(root)?;
    set_top_level_list(&mut lines, &path, "projects", updated)?;
    write_lines(&path, &lines)?;
    Ok(WriteOutcome::Changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(dir: &tempfile::TempDir) -> PathBuf {
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("backlog/tasks")).expect("layout");
        fs::write(root.join("backlog/config.yml"), "statuses: []\n").expect("config");
        root
    }

    fn write_task(root: &Path, id: &str, status: &str, project: Option<&str>) {
        let project_line = project.map_or(String::new(), |p| format!("project: {p}\n"));
        fs::write(
            root.join(format!("backlog/tasks/task-{} - fixture.md", id.trim_start_matches("TASK-"))),
            format!("---\nid: {id}\ntitle: Fixture {id}\nstatus: {status}\npriority: medium\n{project_line}---\n"),
        )
        .expect("task file");
    }

    fn ranking(root: &Path) -> (RepoRanking, Vec<String>) {
        let mut warnings = Vec::new();
        let loaded = load_ranking(root, &mut warnings);
        (loaded, warnings)
    }

    fn ids(tasks: &[BacklogTask]) -> Vec<&str> {
        tasks.iter().map(|task| task.id.as_str()).collect()
    }

    #[test]
    fn rank_task_round_trips_through_load_and_orders_siblings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", Some("Alpha"));
        write_task(&root, "TASK-2", "To Do", Some("Alpha"));
        write_task(&root, "TASK-3", "To Do", Some("Alpha"));

        assert!(rank_task(&root, "TASK-2", &RankPlacement::Top)
            .expect("rank")
            .changed());
        assert!(
            rank_task(&root, "TASK-3", &RankPlacement::After("TASK-2".to_string()))
                .expect("rank")
                .changed()
        );
        assert!(rank_task(
            &root,
            "TASK-1",
            &RankPlacement::Before("TASK-3".to_string())
        )
        .expect("rank")
        .changed());

        let (loaded, warnings) = ranking(&root);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(loaded.tasks["Alpha"], vec!["TASK-2", "TASK-1", "TASK-3"]);

        let repo = load_backlog_repo(&root).expect("load");
        assert_eq!(ids(&repo.tasks), vec!["TASK-2", "TASK-1", "TASK-3"]);
    }

    #[test]
    fn lowercase_and_bareish_ids_canonicalize_to_the_stored_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", None);

        assert!(rank_task(&root, "task-1", &RankPlacement::Top)
            .expect("rank")
            .changed());
        let (loaded, _) = ranking(&root);
        assert_eq!(loaded.root_tasks, vec!["TASK-1"]);
    }

    #[test]
    fn rank_refuses_missing_and_finished_tasks_and_bad_anchors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "Done", None);
        write_task(&root, "TASK-2", "To Do", None);

        let err = rank_task(&root, "TASK-9", &RankPlacement::Top).expect_err("missing");
        assert!(err.to_string().contains("list --all"), "{err}");
        let err = rank_task(&root, "TASK-1", &RankPlacement::Top).expect_err("done");
        assert!(err.to_string().contains("done"), "{err}");
        let err = rank_task(&root, "TASK-2", &RankPlacement::After("TASK-5".to_string()))
            .expect_err("unranked anchor");
        assert!(err.to_string().contains("--top"), "{err}");
        let err = rank_task(&root, "TASK-2", &RankPlacement::After("task-2".to_string()))
            .expect_err("self anchor");
        assert!(err.to_string().contains("itself"), "{err}");
    }

    #[test]
    fn expedite_jumps_the_whole_computed_order_within_the_status_tier() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", Some("Alpha"));
        write_task(&root, "TASK-2", "To Do", Some("Beta"));
        write_task(&root, "TASK-3", "In Progress", None);
        assert!(rank_project(&root, "Alpha", &RankPlacement::Top)
            .expect("rank project")
            .changed());
        assert!(expedite_task(&root, "TASK-2").expect("expedite").changed());

        let repo = load_backlog_repo(&root).expect("load");
        assert_eq!(
            ids(&repo.tasks),
            vec!["TASK-3", "TASK-2", "TASK-1"],
            "In Progress still leads; within To Do the expedited task beats the ranked project"
        );

        assert!(unexpedite_task(&root, "TASK-2")
            .expect("unexpedite")
            .changed());
        let repo = load_backlog_repo(&root).expect("load");
        assert_eq!(ids(&repo.tasks), vec!["TASK-3", "TASK-1", "TASK-2"]);
    }

    #[test]
    fn ranked_projects_float_their_tasks_above_unranked_projects() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", Some("Alpha"));
        write_task(&root, "TASK-2", "To Do", Some("Beta"));
        write_task(&root, "TASK-3", "To Do", None);
        assert!(rank_project(&root, "Beta", &RankPlacement::Top)
            .expect("rank")
            .changed());

        let repo = load_backlog_repo(&root).expect("load");
        assert_eq!(
            ids(&repo.tasks),
            vec!["TASK-2", "TASK-1", "TASK-3"],
            "Beta's task leads; Alpha's and the root task keep the fallback order"
        );
    }

    #[test]
    fn unranked_repo_keeps_todays_comparator_exactly() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-2", "To Do", None);
        write_task(&root, "TASK-1", "Done", None);
        write_task(&root, "TASK-3", "In Progress", None);

        let repo = load_backlog_repo(&root).expect("load");
        assert_eq!(ids(&repo.tasks), vec!["TASK-3", "TASK-2", "TASK-1"]);
    }

    #[test]
    fn stale_ids_are_ignored_on_read_and_pruned_on_the_next_scope_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", Some("Alpha"));
        write_task(&root, "TASK-2", "To Do", Some("Alpha"));
        assert!(rank_task(&root, "TASK-1", &RankPlacement::Top)
            .expect("rank")
            .changed());
        assert!(
            rank_task(&root, "TASK-2", &RankPlacement::After("TASK-1".to_string()))
                .expect("rank")
                .changed()
        );

        // TASK-1 finishes; its rank entry is now stale.
        fs::remove_file(root.join("backlog/tasks/task-1 - fixture.md")).expect("remove");
        let repo = load_backlog_repo(&root).expect("load");
        assert_eq!(ids(&repo.tasks), vec!["TASK-2"], "stale entry is inert");

        // The next write to the scope prunes it from disk.
        assert!(rank_task(&root, "TASK-2", &RankPlacement::Top)
            .expect("re-rank")
            .changed());
        let (loaded, _) = ranking(&root);
        assert_eq!(loaded.tasks["Alpha"], vec!["TASK-2"]);
    }

    #[test]
    fn subissues_rank_within_their_parent_and_inherit_its_project_tier() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", Some("Alpha"));
        write_task(&root, "TASK-1.1", "To Do", None);
        write_task(&root, "TASK-1.2", "To Do", None);
        write_task(&root, "TASK-2", "To Do", Some("Beta"));
        assert!(rank_project(&root, "Alpha", &RankPlacement::Top)
            .expect("rank project")
            .changed());
        assert!(rank_task(&root, "TASK-1.2", &RankPlacement::Top)
            .expect("rank subissue")
            .changed());

        let (loaded, _) = ranking(&root);
        assert_eq!(loaded.subissues["TASK-1"], vec!["TASK-1.2"]);

        let repo = load_backlog_repo(&root).expect("load");
        assert_eq!(
            ids(&repo.tasks),
            vec!["TASK-1", "TASK-1.2", "TASK-1.1", "TASK-2"],
            "sub-issues ride Alpha's project tier; the ranked one leads its sibling"
        );
    }

    #[test]
    fn project_rank_prunes_dead_names_and_refuses_unknown_ones() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", Some("Alpha"));
        write_task(&root, "TASK-2", "To Do", Some("Beta"));
        assert!(rank_project(&root, "Alpha", &RankPlacement::Top)
            .expect("rank")
            .changed());
        assert!(
            rank_project(&root, "Beta", &RankPlacement::After("Alpha".to_string()))
                .expect("rank")
                .changed()
        );

        let err = rank_project(&root, "Gamma", &RankPlacement::Top).expect_err("unknown");
        assert!(err.to_string().contains("project list"), "{err}");

        // Alpha's only task finishes; the next projects write prunes it.
        fs::remove_file(root.join("backlog/tasks/task-1 - fixture.md")).expect("remove");
        assert!(rank_project(&root, "Beta", &RankPlacement::Top)
            .expect("re-rank")
            .changed());
        let (loaded, _) = ranking(&root);
        assert_eq!(loaded.projects, vec!["Beta"]);
    }

    #[test]
    fn unrank_clears_every_scope_and_absent_ids_are_no_ops() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", Some("Alpha"));
        assert!(rank_task(&root, "TASK-1", &RankPlacement::Top)
            .expect("rank")
            .changed());

        assert!(unrank_task(&root, "task-1").expect("unrank").changed());
        assert!(!unrank_task(&root, "TASK-1").expect("again").changed());
        let (loaded, _) = ranking(&root);
        assert!(
            loaded.tasks.is_empty(),
            "emptied scope is removed: {loaded:?}"
        );

        assert!(!unrank_project(&root, "Alpha").expect("no-op").changed());
    }

    #[test]
    fn edits_are_surgical_around_untouched_scopes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", Some("Alpha"));
        write_task(&root, "TASK-2", "To Do", Some("Beta"));
        write_task(&root, "TASK-3", "To Do", None);
        assert!(rank_task(&root, "TASK-1", &RankPlacement::Top)
            .expect("rank")
            .changed());
        assert!(rank_task(&root, "TASK-3", &RankPlacement::Top)
            .expect("rank")
            .changed());
        assert!(rank_project(&root, "Alpha", &RankPlacement::Top)
            .expect("rank")
            .changed());
        let before = fs::read_to_string(root.join(RANKING_REL)).expect("read");

        assert!(rank_task(&root, "TASK-2", &RankPlacement::Top)
            .expect("rank")
            .changed());
        let after = fs::read_to_string(root.join(RANKING_REL)).expect("read");
        let changed: Vec<&str> = after.lines().filter(|l| !before.contains(*l)).collect();
        assert_eq!(
            changed,
            vec!["  Beta:", "    - TASK-2"],
            "exactly one scope block appears; every other byte survives"
        );
    }

    #[test]
    fn scopes_insert_in_key_sorted_order_for_deterministic_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", Some("Zeta"));
        write_task(&root, "TASK-2", "To Do", Some("Alpha"));
        assert!(rank_task(&root, "TASK-1", &RankPlacement::Top)
            .expect("rank")
            .changed());
        assert!(rank_task(&root, "TASK-2", &RankPlacement::Top)
            .expect("rank")
            .changed());

        let text = fs::read_to_string(root.join(RANKING_REL)).expect("read");
        let alpha = text.find("  Alpha:").expect("alpha scope");
        let zeta = text.find("  Zeta:").expect("zeta scope");
        assert!(
            alpha < zeta,
            "scopes are key-sorted regardless of rank order:\n{text}"
        );
    }

    #[test]
    fn malformed_files_warn_load_empty_and_fail_writes_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", None);
        fs::write(root.join(RANKING_REL), "expedite: [not: [valid\n").expect("write");

        let (loaded, warnings) = ranking(&root);
        assert!(loaded.is_empty());
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        let repo = load_backlog_repo(&root).expect("load survives");
        assert_eq!(repo.warnings.len(), 1, "{:?}", repo.warnings);

        let err = rank_task(&root, "TASK-1", &RankPlacement::Top).expect_err("fail closed");
        assert!(err.to_string().contains("does not parse cleanly"), "{err}");
    }

    #[test]
    fn a_restyled_file_missing_a_key_fails_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", None);
        fs::write(root.join(RANKING_REL), "projects: []\n").expect("write");

        let err = rank_task(&root, "TASK-1", &RankPlacement::Top).expect_err("no root_tasks key");
        assert!(err.to_string().contains("root_tasks"), "{err}");
    }

    #[test]
    fn missing_file_is_empty_and_first_write_creates_the_skeleton() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        let (loaded, warnings) = ranking(&root);
        assert!(
            loaded.is_empty() && warnings.is_empty(),
            "missing file is silent"
        );

        write_task(&root, "TASK-1", "To Do", None);
        assert!(rank_task(&root, "TASK-1", &RankPlacement::Top)
            .expect("rank")
            .changed());
        let text = fs::read_to_string(root.join(RANKING_REL)).expect("read");
        assert!(text.starts_with("# Stack ranking"), "{text}");
        for key in ["expedite: []", "projects: []", "tasks: {}", "subissues: {}"] {
            assert!(text.contains(key), "skeleton keeps `{key}`:\n{text}");
        }
        assert!(text.contains("root_tasks:\n  - TASK-1"), "{text}");
    }

    #[test]
    fn move_arrows_swap_enter_and_leave_the_ranked_set_honestly() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", Some("Alpha"));
        write_task(&root, "TASK-2", "To Do", Some("Alpha"));
        write_task(&root, "TASK-3", "To Do", Some("Alpha"));

        // Up on an unranked task with no ranked siblings ranks it first.
        assert!(rank_task_move(&root, "TASK-2", RankMove::Up)
            .expect("move")
            .changed());
        // Up on another unranked task enters the ranked set at the bottom.
        assert!(rank_task_move(&root, "TASK-3", RankMove::Up)
            .expect("move")
            .changed());
        let (loaded, _) = ranking(&root);
        assert_eq!(loaded.tasks["Alpha"], vec!["TASK-2", "TASK-3"]);

        // Up swaps ranked siblings; up at the top is a no-op.
        assert!(rank_task_move(&root, "TASK-3", RankMove::Up)
            .expect("move")
            .changed());
        assert!(!rank_task_move(&root, "TASK-3", RankMove::Up)
            .expect("no-op")
            .changed());
        let (loaded, _) = ranking(&root);
        assert_eq!(loaded.tasks["Alpha"], vec!["TASK-3", "TASK-2"]);

        // Down swaps; down on the LAST ranked task unranks it; down on an
        // unranked task is a no-op.
        assert!(rank_task_move(&root, "TASK-3", RankMove::Down)
            .expect("move")
            .changed());
        assert!(rank_task_move(&root, "TASK-3", RankMove::Down)
            .expect("unrank")
            .changed());
        let (loaded, _) = ranking(&root);
        assert_eq!(loaded.tasks["Alpha"], vec!["TASK-2"]);
        assert!(!rank_task_move(&root, "TASK-3", RankMove::Down)
            .expect("no-op")
            .changed());
        assert!(!rank_task_move(&root, "TASK-1", RankMove::Down)
            .expect("no-op")
            .changed());
    }

    #[test]
    fn project_moves_share_the_arrow_semantics_and_position_is_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "To Do", Some("Alpha"));
        write_task(&root, "TASK-2", "To Do", Some("Beta"));

        assert!(rank_project_move(&root, "Alpha", RankMove::Up)
            .expect("move")
            .changed());
        assert!(rank_project_move(&root, "Beta", RankMove::Up)
            .expect("move")
            .changed());
        assert!(rank_project_move(&root, "Beta", RankMove::Up)
            .expect("swap")
            .changed());
        let (loaded, _) = ranking(&root);
        assert_eq!(loaded.projects, vec!["Beta", "Alpha"]);

        let repo_loaded = load_backlog_repo(&root).expect("load");
        let alpha_task = repo_loaded
            .tasks
            .iter()
            .find(|t| t.id == "TASK-1")
            .expect("task");
        assert!(rank_task(&root, "TASK-1", &RankPlacement::Top)
            .expect("rank")
            .changed());
        let repo_loaded = load_backlog_repo(&root).expect("load");
        let alpha_task2 = repo_loaded
            .tasks
            .iter()
            .find(|t| t.id == "TASK-1")
            .expect("task");
        assert_eq!(repo_loaded.ranking.task_rank_position(alpha_task2), Some(0));
        assert_eq!(
            load_backlog_repo(&root)
                .expect("load")
                .ranking
                .task_rank_position(alpha_task),
            Some(0),
            "position reads from the scope the task actually lives in"
        );
    }

    #[test]
    fn expedite_validates_liveness_and_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task(&root, "TASK-1", "Done", None);
        write_task(&root, "TASK-2", "To Do", None);

        let err = expedite_task(&root, "TASK-1").expect_err("done");
        assert!(err.to_string().contains("done"), "{err}");
        assert!(expedite_task(&root, "TASK-2").expect("expedite").changed());
        assert!(!expedite_task(&root, "TASK-2").expect("again").changed());
        assert!(!unexpedite_task(&root, "TASK-9").expect("absent").changed());
    }
}
