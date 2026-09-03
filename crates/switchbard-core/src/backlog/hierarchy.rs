//! Project and initiative **definition files** — the two Linear-hierarchy
//! tiers above tasks (trajectory: *Linear-vocabulary hierarchy*,
//! divergence 2).
//!
//! Membership is name-keyed: a task's `project:` frontmatter names a project,
//! and a project def's `initiative:` names an initiative. A tier's entry
//! *exists* the moment anything references it; the definition files here —
//! `backlog/projects/<slug>.md` and `backlog/initiatives/<slug>.md` — are
//! optional enrichment that give a name lifecycle (status, target date,
//! lead) and a prose description. Nothing validates a task's project against
//! the defined set: referencing an undefined project is how projects are
//! born, exactly as milestones were never validated before the divergence.
//!
//! Def files share the task files' skeleton (YAML frontmatter + markdown
//! body) and their write discipline (line-level frontmatter splices, byte
//! no-ops write nothing, atomic replace) via `super::write`'s primitives —
//! one frontmatter engine, not two. They deliberately do **not** carry
//! `created_date`/`updated_date`: a def is a description of intent, not an
//! activity log, and the roll-up derives freshness from member tasks.

use super::goals::rename_project_in_goals;
use super::parse::{load_backlog_repo, split_frontmatter, task_file_round_trips, yaml_string};
use super::ranking::rename_project_in_ranking;
use super::write::{
    atomic_write, filename_slug, join_raw, remove_key, set_scalar, set_task_project, split_raw,
    validated_single_line, yaml_scalar, WriteOutcome,
};
use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The lifecycle vocabulary for projects *and* initiatives — deliberately a
/// separate, fixed list from task statuses (divergence 3): a project is a
/// completable container, not a kanban card, and its states are validated
/// only when a definition is written.
pub const PROJECT_STATUSES: &[&str] = &["Planned", "In Progress", "Completed", "Canceled"];

/// The status a definition gets when none is given, and the one assumed for
/// a def file that omits the key.
pub const DEFAULT_PROJECT_STATUS: &str = "Planned";

/// A `backlog/projects/<slug>.md` definition. `name` is the join key to
/// `BacklogTask::project` — exact string match, same as the GUI's milestone
/// grouping always was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDef {
    pub name: String,
    /// One of [`PROJECT_STATUSES`] on every file this layer writes; a
    /// hand-written unknown value is kept verbatim and rendered honestly
    /// rather than coerced.
    pub status: String,
    /// `YYYY-MM-DD`, stored as written.
    pub target_date: Option<String>,
    /// Name of the initiative this project belongs to — the same
    /// name-keyed pattern one level up.
    pub initiative: Option<String>,
    pub lead: Option<String>,
    /// The markdown body below the frontmatter, trimmed.
    pub description: String,
    pub path: PathBuf,
}

/// A `backlog/initiatives/<slug>.md` definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitiativeDef {
    pub name: String,
    pub status: String,
    pub target_date: Option<String>,
    pub description: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewProjectDef {
    pub name: String,
    /// Empty means [`DEFAULT_PROJECT_STATUS`].
    pub status: String,
    pub target_date: Option<String>,
    pub initiative: Option<String>,
    pub lead: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewInitiativeDef {
    pub name: String,
    pub status: String,
    pub target_date: Option<String>,
    pub description: String,
}

/// Field-wise edit of a project def, mirroring `BacklogTaskPatch`'s
/// assign-or-clear pairs. `description: Some(..)` replaces the whole body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectDefPatch {
    pub status: Option<String>,
    pub target_date: Option<String>,
    pub clear_target_date: bool,
    pub initiative: Option<String>,
    pub clear_initiative: bool,
    pub lead: Option<String>,
    pub clear_lead: bool,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InitiativeDefPatch {
    pub status: Option<String>,
    pub target_date: Option<String>,
    pub clear_target_date: bool,
    pub description: Option<String>,
}

impl ProjectDefPatch {
    pub fn is_empty(&self) -> bool {
        self.status.is_none()
            && self.target_date.is_none()
            && !self.clear_target_date
            && self.initiative.is_none()
            && !self.clear_initiative
            && self.lead.is_none()
            && !self.clear_lead
            && self.description.is_none()
    }
}

impl InitiativeDefPatch {
    pub fn is_empty(&self) -> bool {
        self.status.is_none()
            && self.target_date.is_none()
            && !self.clear_target_date
            && self.description.is_none()
    }
}

// ---- loading ----

/// Load every `backlog/projects/*.md` def. Never fails the repo load:
/// unreadable or fence-less files become warnings, same posture as task
/// parsing.
pub(super) fn load_project_defs(root: &Path, warnings: &mut Vec<String>) -> Vec<ProjectDef> {
    load_defs(
        root,
        PROJECTS_DIR,
        warnings,
        |name, status, mapping, body, path| ProjectDef {
            name,
            status,
            target_date: yaml_string(mapping, "target_date"),
            initiative: yaml_string(mapping, "initiative"),
            lead: yaml_string(mapping, "lead"),
            description: body,
            path,
        },
    )
}

/// Load every `backlog/initiatives/*.md` def.
pub(super) fn load_initiative_defs(root: &Path, warnings: &mut Vec<String>) -> Vec<InitiativeDef> {
    load_defs(
        root,
        INITIATIVES_DIR,
        warnings,
        |name, status, mapping, body, path| InitiativeDef {
            name,
            status,
            target_date: yaml_string(mapping, "target_date"),
            description: body,
            path,
        },
    )
}

const PROJECTS_DIR: &str = "backlog/projects";
const INITIATIVES_DIR: &str = "backlog/initiatives";

fn load_defs<T>(
    root: &Path,
    rel: &str,
    warnings: &mut Vec<String>,
    build: impl Fn(String, String, &serde_yaml::Mapping, String, PathBuf) -> T,
) -> Vec<T> {
    let dir = root.join(rel);
    if !dir.is_dir() {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(&dir) else {
        warnings.push(format!("cannot read {}", dir.display()));
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(OsStr::to_str) == Some("md"))
        .collect();
    paths.sort();

    let mut defs = Vec::with_capacity(paths.len());
    for path in paths {
        let Ok(text) = fs::read_to_string(&path) else {
            warnings.push(format!("{}: cannot read definition", path.display()));
            continue;
        };
        let (mapping, body) = split_frontmatter(&text);
        let name = match yaml_string(&mapping, "name") {
            Some(name) => name,
            None => {
                let stem = path
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .unwrap_or_default()
                    .to_string();
                warnings.push(format!(
                    "{}: definition has no `name:`; using the file stem `{stem}`",
                    path.display()
                ));
                stem
            }
        };
        let status =
            yaml_string(&mapping, "status").unwrap_or_else(|| DEFAULT_PROJECT_STATUS.to_string());
        defs.push(build(name, status, &mapping, body.trim().to_string(), path));
    }
    defs
}

// ---- writing ----

/// Everything [`create_def`] needs beyond the repo root — one struct so the
/// per-kind wrappers pass a named shape instead of eight positional args.
struct DefSpec<'a> {
    rel: &'a str,
    kind: &'a str,
    name: &'a str,
    status: &'a str,
    target_date: Option<&'a str>,
    /// Kind-specific optional scalars (projects: `initiative`, `lead`).
    extra_fields: &'a [(&'a str, Option<&'a str>)],
    description: &'a str,
}

/// Create `backlog/projects/<slug>.md`. Refuses a name any existing def in
/// the repo already claims (whatever its slug), and refuses a slug collision
/// — including a case-variant one — before touching the filesystem.
pub fn create_project_def(root: &Path, def: &NewProjectDef) -> Result<PathBuf> {
    let mut fields: Vec<(&str, Option<&str>)> = vec![
        ("initiative", def.initiative.as_deref()),
        ("lead", def.lead.as_deref()),
    ];
    fields.retain(|(_, v)| v.is_some());
    create_def(
        root,
        DefSpec {
            rel: PROJECTS_DIR,
            kind: "project",
            name: &def.name,
            status: &def.status,
            target_date: def.target_date.as_deref(),
            extra_fields: &fields,
            description: &def.description,
        },
    )
}

pub fn create_initiative_def(root: &Path, def: &NewInitiativeDef) -> Result<PathBuf> {
    create_def(
        root,
        DefSpec {
            rel: INITIATIVES_DIR,
            kind: "initiative",
            name: &def.name,
            status: &def.status,
            target_date: def.target_date.as_deref(),
            extra_fields: &[],
            description: &def.description,
        },
    )
}

fn create_def(root: &Path, spec: DefSpec<'_>) -> Result<PathBuf> {
    let name = validated_single_line("name", spec.name)?;
    let kind = spec.kind;
    let status = if spec.status.trim().is_empty() {
        DEFAULT_PROJECT_STATUS
    } else {
        validated_def_status(spec.status)?
    };
    if let Some((_, existing)) = find_def_file(root, spec.rel, name)? {
        bail!(
            "{kind} '{name}' is already defined at {} — edit it instead",
            existing.display()
        );
    }

    let mut fm = vec![
        format!("name: {}", yaml_scalar(name)),
        format!("status: {}", yaml_scalar(status)),
    ];
    if let Some(date) = spec.target_date {
        fm.push(format!(
            "target_date: {}",
            yaml_scalar(validated_single_line("target_date", date)?)
        ));
    }
    for (key, value) in spec.extra_fields {
        if let Some(value) = value {
            fm.push(format!(
                "{key}: {}",
                yaml_scalar(validated_single_line(key, value)?)
            ));
        }
    }
    let body = if spec.description.trim().is_empty() {
        String::new()
    } else {
        format!("\n{}\n", spec.description.trim())
    };
    let text = format!("---\n{}\n---\n{body}", fm.join("\n"));

    let dir = root.join(spec.rel);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let slug = filename_slug(name);
    // Checked by scan, not left to `create_new`: names differing only in
    // case share a filename on case-insensitive filesystems (macOS APFS),
    // and the `create_new` failure would surface as a baffling generic
    // "slug already taken?" instead of naming the colliding definition.
    // Scanning also makes the behavior identical on case-sensitive Linux.
    if let Some(colliding) = slug_collision(&dir, &slug) {
        bail!(
            "a {kind} definition with the same filename slug already exists at {} \
             (names differing only in case share a slug) — pick a distinct name",
            colliding.display()
        );
    }
    let path = dir.join(format!("{slug}.md"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("creating {} (slug already taken?)", path.display()))?;
    file.write_all(text.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// An existing `.md` file in `dir` whose stem equals `slug` ignoring ASCII
/// case, if any — see the call site for why this is a scan.
fn slug_collision(dir: &Path, slug: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension().and_then(OsStr::to_str) == Some("md")
                && path
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .is_some_and(|stem| stem.eq_ignore_ascii_case(slug))
        })
}

pub fn edit_project_def(root: &Path, name: &str, patch: &ProjectDefPatch) -> Result<WriteOutcome> {
    let path = resolve_def_file(root, PROJECTS_DIR, "project", name)?;
    apply_def_edit(&path, |fm, body| {
        if let Some(status) = &patch.status {
            set_scalar(
                fm,
                "status",
                &yaml_scalar(validated_def_status(status)?),
                Some("name"),
            );
        }
        edit_optional_scalar(
            fm,
            "target_date",
            patch.target_date.as_deref(),
            patch.clear_target_date,
            "status",
        )?;
        edit_optional_scalar(
            fm,
            "initiative",
            patch.initiative.as_deref(),
            patch.clear_initiative,
            "target_date",
        )?;
        edit_optional_scalar(
            fm,
            "lead",
            patch.lead.as_deref(),
            patch.clear_lead,
            "initiative",
        )?;
        if let Some(description) = &patch.description {
            *body = if description.trim().is_empty() {
                String::new()
            } else {
                format!("\n{}\n", description.trim())
            };
        }
        Ok(())
    })
}

pub fn edit_initiative_def(
    root: &Path,
    name: &str,
    patch: &InitiativeDefPatch,
) -> Result<WriteOutcome> {
    let path = resolve_def_file(root, INITIATIVES_DIR, "initiative", name)?;
    apply_def_edit(&path, |fm, body| {
        if let Some(status) = &patch.status {
            set_scalar(
                fm,
                "status",
                &yaml_scalar(validated_def_status(status)?),
                Some("name"),
            );
        }
        edit_optional_scalar(
            fm,
            "target_date",
            patch.target_date.as_deref(),
            patch.clear_target_date,
            "status",
        )?;
        if let Some(description) = &patch.description {
            *body = if description.trim().is_empty() {
                String::new()
            } else {
                format!("\n{}\n", description.trim())
            };
        }
        Ok(())
    })
}

/// What [`rename_project`] touched, for the caller to report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectRename {
    /// The def file was renamed (`name:` and, when the slug changed, the
    /// filename). `false` when the project had no definition file.
    pub def_renamed: bool,
    /// Task files whose `project:` (or legacy `milestone:`) was rewritten.
    pub tasks_updated: usize,
    pub ranking_updated: bool,
    pub goals_updated: bool,
}

/// Rename a project everywhere its name is the key: its def file, every
/// member task (active, completed, draft, archived), `ranking.yml`, and
/// `goals.yml` — the bulk mutation the trajectory doc deferred until asked
/// for (owner request 2026-09-02, TASK-134).
///
/// Every refusal fires before the first write: `new` must not already be
/// defined or referenced (merging two projects is not a rename), `old` must
/// be defined or referenced (else there is nothing to rename), the new def
/// slug must be free, and every member task file must round-trip so the
/// pass cannot stop halfway on a file the write layer would reject. The
/// steps themselves are each atomic but not jointly transactional; an error
/// mid-way names the step, and re-running the same rename is safe (files
/// already renamed are simply no longer members of `old`).
pub fn rename_project(root: &Path, old: &str, new: &str) -> Result<ProjectRename> {
    let old = validated_single_line("project", old)?;
    let new = validated_single_line("project", new)?;
    if old == new {
        bail!("project '{old}' already has that name");
    }
    let repo = load_backlog_repo(root)?;
    let members: Vec<&super::types::BacklogTask> = repo
        .tasks
        .iter()
        .filter(|task| task.project.as_deref() == Some(old))
        .collect();
    let old_def = find_def_file(root, PROJECTS_DIR, old)?;
    if members.is_empty() && old_def.is_none() {
        bail!(
            "no project named '{old}' - nothing defines or references it (see `sb project list`)"
        );
    }
    if let Some((_, path)) = find_def_file(root, PROJECTS_DIR, new)? {
        bail!(
            "project '{new}' is already defined at {} - merging projects is not a rename",
            path.display()
        );
    }
    if let Some(task) = repo
        .tasks
        .iter()
        .find(|task| task.project.as_deref() == Some(new))
    {
        bail!(
            "project '{new}' is already referenced by {} - merging projects is not a rename",
            task.id
        );
    }
    let new_def_path = match &old_def {
        Some((_, old_path)) => Some(planned_def_path(root, old_path, new)?),
        None => None,
    };
    let stuck: Vec<String> = members
        .iter()
        .filter(|task| !task_file_round_trips(&task.path))
        .map(|task| task.path.display().to_string())
        .collect();
    if !stuck.is_empty() {
        bail!(
            "refusing to rename: {} member file(s) would not round-trip through the write layer: {}",
            stuck.len(),
            stuck.join(", ")
        );
    }

    let mut report = ProjectRename::default();
    for task in &members {
        if set_task_project(&task.path, Some(new))
            .with_context(|| format!("renaming project on {}", task.id))?
            .changed()
        {
            report.tasks_updated += 1;
        }
    }
    if let (Some((_, old_path)), Some(new_path)) = (old_def, new_def_path) {
        let renamed = apply_def_edit(&old_path, |fm, _| {
            set_scalar(fm, "name", &yaml_scalar(new), None);
            Ok(())
        })
        .with_context(|| format!("renaming {}", old_path.display()))?;
        if !renamed.changed() {
            bail!(
                "{} did not change when its name was rewritten - is its `name:` already '{new}'?",
                old_path.display()
            );
        }
        if new_path != old_path {
            fs::rename(&old_path, &new_path).with_context(|| {
                format!("moving {} to {}", old_path.display(), new_path.display())
            })?;
        }
        report.def_renamed = true;
    }
    report.ranking_updated = rename_project_in_ranking(root, old, new)
        .context("renaming project in backlog/ranking.yml")?
        .changed();
    report.goals_updated = rename_project_in_goals(root, old, new)
        .context("renaming project in backlog/goals.yml")?
        .changed();
    Ok(report)
}

/// Where the renamed def file will live: the new name's slug, unless the
/// only file already carrying that slug (case-insensitively) is the old
/// def itself, in which case the path is kept.
fn planned_def_path(root: &Path, old_path: &Path, new: &str) -> Result<PathBuf> {
    let dir = root.join(PROJECTS_DIR);
    let slug = filename_slug(new);
    if let Some(colliding) = slug_collision(&dir, &slug) {
        if colliding != old_path {
            bail!(
                "a project definition with the same filename slug already exists at {} \
                 (names differing only in case share a slug) - pick a distinct name",
                colliding.display()
            );
        }
        return Ok(colliding);
    }
    Ok(dir.join(format!("{slug}.md")))
}

fn edit_optional_scalar(
    fm: &mut Vec<String>,
    key: &str,
    value: Option<&str>,
    clear: bool,
    insert_after: &str,
) -> Result<()> {
    if let Some(value) = value {
        set_scalar(
            fm,
            key,
            &yaml_scalar(validated_single_line(key, value)?),
            Some(insert_after),
        );
    } else if clear {
        remove_key(fm, key);
    }
    Ok(())
}

fn validated_def_status(status: &str) -> Result<&str> {
    let status = validated_single_line("status", status)?;
    if !PROJECT_STATUSES
        .iter()
        .any(|s| s.eq_ignore_ascii_case(status))
    {
        bail!(
            "Invalid status: {status}. Valid statuses are: {}",
            PROJECT_STATUSES.join(", ")
        );
    }
    // Return the canonical casing so `planned` and `Planned` are one value.
    Ok(PROJECT_STATUSES
        .iter()
        .find(|s| s.eq_ignore_ascii_case(status))
        .expect("membership just checked"))
}

/// The def file whose `name:` matches, if exactly one does. Zero matches and
/// the caller decides (create refuses duplicates, edit needs a target);
/// multiple matches are always an error — two files claiming one name is a
/// conflict a write must not paper over.
fn find_def_file(root: &Path, rel: &str, name: &str) -> Result<Option<(String, PathBuf)>> {
    let mut warnings = Vec::new();
    let matches: Vec<(String, PathBuf)> = match rel {
        PROJECTS_DIR => load_project_defs(root, &mut warnings)
            .into_iter()
            .filter(|def| def.name == name)
            .map(|def| (def.name, def.path))
            .collect(),
        _ => load_initiative_defs(root, &mut warnings)
            .into_iter()
            .filter(|def| def.name == name)
            .map(|def| (def.name, def.path))
            .collect(),
    };
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.into_iter().next()),
        n => bail!("{n} definition files under {rel} claim the name '{name}' — remove the duplicates first"),
    }
}

fn resolve_def_file(root: &Path, rel: &str, kind: &str, name: &str) -> Result<PathBuf> {
    match find_def_file(root, rel, name)? {
        Some((_, path)) => Ok(path),
        None => {
            bail!("no {kind} definition named '{name}' — run `sb {kind} create` first")
        }
    }
}

/// `super::write::apply_edit`'s shape without the `updated_date` bump — defs
/// don't carry dates. Same guarantees otherwise: byte no-ops write nothing,
/// real changes replace atomically.
fn apply_def_edit(
    path: &Path,
    edit: impl FnOnce(&mut Vec<String>, &mut String) -> Result<()>,
) -> Result<WriteOutcome> {
    let original =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut raw = split_raw(&original)?;
    edit(&mut raw.fm, &mut raw.rest)?;
    let next = join_raw(&raw.fm, &raw.rest);
    if next == original {
        return Ok(WriteOutcome::Unchanged);
    }
    atomic_write(path, &next)?;
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

    #[test]
    fn create_and_reload_round_trips_every_field() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        let path = create_project_def(
            &root,
            &NewProjectDef {
                name: "Lucella cutover".to_string(),
                status: String::new(),
                target_date: Some("2026-10-01".to_string()),
                initiative: Some("Rebrand".to_string()),
                lead: Some("ben".to_string()),
                description: "Make lucella.app canonical.".to_string(),
            },
        )
        .expect("create succeeds");
        assert!(path.ends_with("backlog/projects/Lucella-cutover.md"));

        let mut warnings = Vec::new();
        let defs = load_project_defs(&root, &mut warnings);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(defs.len(), 1);
        let def = &defs[0];
        assert_eq!(def.name, "Lucella cutover");
        assert_eq!(def.status, DEFAULT_PROJECT_STATUS, "empty status defaults");
        assert_eq!(def.target_date.as_deref(), Some("2026-10-01"));
        assert_eq!(def.initiative.as_deref(), Some("Rebrand"));
        assert_eq!(def.lead.as_deref(), Some("ben"));
        assert_eq!(def.description, "Make lucella.app canonical.");
    }

    #[test]
    fn create_refuses_a_name_an_existing_def_already_claims() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        let def = NewProjectDef {
            name: "Twice".to_string(),
            ..NewProjectDef::default()
        };
        create_project_def(&root, &def).expect("first create succeeds");
        let err = create_project_def(&root, &def).expect_err("duplicate refused");
        assert!(err.to_string().contains("already defined"), "{err}");
    }

    #[test]
    fn create_validates_status_and_canonicalizes_case() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        let err = create_project_def(
            &root,
            &NewProjectDef {
                name: "Bad".to_string(),
                status: "Shipped".to_string(),
                ..NewProjectDef::default()
            },
        )
        .expect_err("unknown status refused");
        assert!(
            err.to_string()
                .contains("Invalid status: Shipped. Valid statuses are: Planned, In Progress"),
            "{err}"
        );

        create_project_def(
            &root,
            &NewProjectDef {
                name: "Cased".to_string(),
                status: "in progress".to_string(),
                ..NewProjectDef::default()
            },
        )
        .expect("case-insensitive input accepted");
        let mut warnings = Vec::new();
        let defs = load_project_defs(&root, &mut warnings);
        assert_eq!(defs[0].status, "In Progress", "canonical casing stored");
    }

    #[test]
    fn missing_name_key_falls_back_to_the_file_stem_with_a_warning() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        fs::create_dir_all(root.join("backlog/projects")).expect("dir");
        fs::write(
            root.join("backlog/projects/orphan.md"),
            "---\nstatus: Planned\n---\n",
        )
        .expect("fixture");

        let mut warnings = Vec::new();
        let defs = load_project_defs(&root, &mut warnings);
        assert_eq!(defs[0].name, "orphan");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("no `name:`"), "{warnings:?}");
    }

    #[test]
    fn edit_is_surgical_and_a_noop_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        create_project_def(
            &root,
            &NewProjectDef {
                name: "Edit me".to_string(),
                description: "Original body.".to_string(),
                ..NewProjectDef::default()
            },
        )
        .expect("create succeeds");

        let outcome = edit_project_def(
            &root,
            "Edit me",
            &ProjectDefPatch {
                status: Some("Completed".to_string()),
                ..ProjectDefPatch::default()
            },
        )
        .expect("edit succeeds");
        assert_eq!(outcome, WriteOutcome::Changed);

        let mut warnings = Vec::new();
        let defs = load_project_defs(&root, &mut warnings);
        assert_eq!(defs[0].status, "Completed");
        assert_eq!(
            defs[0].description, "Original body.",
            "a status edit leaves the body untouched"
        );

        let outcome = edit_project_def(
            &root,
            "Edit me",
            &ProjectDefPatch {
                status: Some("Completed".to_string()),
                ..ProjectDefPatch::default()
            },
        )
        .expect("edit succeeds");
        assert_eq!(outcome, WriteOutcome::Unchanged, "no-op writes nothing");
    }

    #[test]
    fn edit_assigns_and_clears_optional_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        create_project_def(
            &root,
            &NewProjectDef {
                name: "Fields".to_string(),
                ..NewProjectDef::default()
            },
        )
        .expect("create succeeds");

        let outcome = edit_project_def(
            &root,
            "Fields",
            &ProjectDefPatch {
                target_date: Some("2026-12-01".to_string()),
                initiative: Some("Rebrand".to_string()),
                ..ProjectDefPatch::default()
            },
        )
        .expect("assign succeeds");
        assert_eq!(outcome, WriteOutcome::Changed);
        let mut warnings = Vec::new();
        let defs = load_project_defs(&root, &mut warnings);
        assert_eq!(defs[0].target_date.as_deref(), Some("2026-12-01"));
        assert_eq!(defs[0].initiative.as_deref(), Some("Rebrand"));

        let outcome = edit_project_def(
            &root,
            "Fields",
            &ProjectDefPatch {
                clear_target_date: true,
                clear_initiative: true,
                ..ProjectDefPatch::default()
            },
        )
        .expect("clear succeeds");
        assert_eq!(outcome, WriteOutcome::Changed);
        let defs = load_project_defs(&root, &mut warnings);
        assert_eq!(defs[0].target_date, None);
        assert_eq!(defs[0].initiative, None);
    }

    #[test]
    fn editing_an_undefined_name_names_the_next_step() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        let err = edit_project_def(&root, "Ghost", &ProjectDefPatch::default())
            .expect_err("undefined refused");
        assert!(
            err.to_string().contains("project create"),
            "the error names the next step: {err}"
        );
    }

    #[test]
    fn initiative_defs_round_trip_too() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        create_initiative_def(
            &root,
            &NewInitiativeDef {
                name: "Rebrand".to_string(),
                status: "In Progress".to_string(),
                target_date: None,
                description: "Ledger becomes Lucella.".to_string(),
            },
        )
        .expect("create succeeds");

        let mut warnings = Vec::new();
        let defs = load_initiative_defs(&root, &mut warnings);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "Rebrand");
        assert_eq!(defs[0].status, "In Progress");
        assert_eq!(defs[0].description, "Ledger becomes Lucella.");

        let outcome = edit_initiative_def(
            &root,
            "Rebrand",
            &InitiativeDefPatch {
                status: Some("Completed".to_string()),
                ..InitiativeDefPatch::default()
            },
        )
        .expect("edit succeeds");
        assert_eq!(outcome, WriteOutcome::Changed);
        let defs = load_initiative_defs(&root, &mut warnings);
        assert_eq!(defs[0].status, "Completed");
    }

    #[test]
    fn names_with_tabs_are_refused_and_case_variant_slugs_get_a_clear_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);

        let err = create_project_def(
            &root,
            &NewProjectDef {
                name: "Alpha\tBeta".to_string(),
                ..NewProjectDef::default()
            },
        )
        .expect_err("tab in a name would corrupt the TSV list contract");
        assert!(err.to_string().contains("tab"), "{err}");

        create_project_def(
            &root,
            &NewProjectDef {
                name: "Fresh Start".to_string(),
                ..NewProjectDef::default()
            },
        )
        .expect("first create succeeds");
        let err = create_project_def(
            &root,
            &NewProjectDef {
                name: "fresh start".to_string(),
                ..NewProjectDef::default()
            },
        )
        .expect_err("case-variant slug collides");
        assert!(
            err.to_string().contains("differing only in case"),
            "the diagnostic names the real cause: {err}"
        );
    }

    // ---- rename_project ----

    fn write_task_file(root: &Path, id: &str, membership_line: &str) -> PathBuf {
        let path = root.join(format!("backlog/tasks/task-{id} - fixture.md"));
        fs::write(
            &path,
            format!("---\nid: TASK-{id}\ntitle: Fixture {id}\nstatus: To Do\npriority: medium\n{membership_line}---\n"),
        )
        .expect("task file");
        path
    }

    fn projects_of(root: &Path) -> Vec<(String, Option<String>)> {
        let repo = load_backlog_repo(root).expect("load");
        repo.tasks
            .iter()
            .map(|t| (t.id.clone(), t.project.clone()))
            .collect()
    }

    #[test]
    fn rename_follows_the_name_into_def_tasks_ranking_and_goals() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        create_project_def(
            &root,
            &NewProjectDef {
                name: "Getting Financing".to_string(),
                initiative: Some("Acquisition".to_string()),
                description: "Term sheets.".to_string(),
                ..NewProjectDef::default()
            },
        )
        .expect("def");
        write_task_file(&root, "1", "project: Getting Financing\n");
        write_task_file(&root, "2", "milestone: Getting Financing\n");
        write_task_file(&root, "3", "project: Other\n");
        assert!(super::super::ranking::rank_project(
            &root,
            "Getting Financing",
            &super::super::ranking::RankPlacement::Top,
        )
        .expect("rank project")
        .changed());
        assert!(super::super::ranking::rank_task(
            &root,
            "TASK-1",
            &super::super::ranking::RankPlacement::Top,
        )
        .expect("rank task")
        .changed());
        super::super::goals::create_goal(
            &root,
            &super::super::goals::NewGoal {
                name: "Term sheets".to_string(),
                unit: "sheets".to_string(),
                measure: super::super::goals::GoalMeasure::Tasks,
                scope: Some("Getting Financing".to_string()),
                week: "2026-08-31".to_string(),
                target: 3,
            },
        )
        .expect("goal");
        super::super::goals::attach_goal_inputs(
            &root,
            "Term sheets",
            &[],
            &["Getting Financing".to_string()],
        )
        .expect("attach");

        let report = rename_project(&root, "Getting Financing", "Lender package").expect("rename");
        assert_eq!(
            report,
            ProjectRename {
                def_renamed: true,
                tasks_updated: 2,
                ranking_updated: true,
                goals_updated: true,
            }
        );

        let mut warnings = Vec::new();
        let defs = load_project_defs(&root, &mut warnings);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "Lender package");
        assert_eq!(defs[0].initiative.as_deref(), Some("Acquisition"));
        assert_eq!(defs[0].description, "Term sheets.");
        assert!(
            defs[0].path.ends_with("Lender-package.md"),
            "{:?}",
            defs[0].path
        );

        let mut members = projects_of(&root);
        members.sort();
        assert_eq!(
            members,
            vec![
                ("TASK-1".to_string(), Some("Lender package".to_string())),
                ("TASK-2".to_string(), Some("Lender package".to_string())),
                ("TASK-3".to_string(), Some("Other".to_string())),
            ]
        );
        let legacy =
            fs::read_to_string(root.join("backlog/tasks/task-2 - fixture.md")).expect("read");
        assert!(legacy.contains("project: Lender package") && !legacy.contains("milestone:"));

        let repo = load_backlog_repo(&root).expect("reload");
        assert_eq!(repo.ranking.projects, vec!["Lender package".to_string()]);
        assert_eq!(
            repo.ranking.tasks.get("Lender package").cloned(),
            Some(vec!["TASK-1".to_string()])
        );
        assert!(!repo.ranking.tasks.contains_key("Getting Financing"));
        assert_eq!(repo.goals[0].scope.as_deref(), Some("Lender package"));
        assert_eq!(
            repo.goals[0].inputs.projects,
            vec!["Lender package".to_string()]
        );
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn rename_without_a_def_file_touches_only_tasks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        write_task_file(&root, "1", "project: Loose\n");
        let report = rename_project(&root, "Loose", "Tight").expect("rename");
        assert_eq!(
            report,
            ProjectRename {
                def_renamed: false,
                tasks_updated: 1,
                ranking_updated: false,
                goals_updated: false,
            }
        );
        assert_eq!(
            projects_of(&root),
            vec![("TASK-1".to_string(), Some("Tight".to_string()))]
        );
    }

    #[test]
    fn rename_refuses_before_writing_anything() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        create_project_def(
            &root,
            &NewProjectDef {
                name: "Alpha".to_string(),
                ..NewProjectDef::default()
            },
        )
        .expect("def");
        write_task_file(&root, "1", "project: Alpha\n");
        write_task_file(&root, "2", "project: Beta\n");
        let before =
            fs::read_to_string(root.join("backlog/tasks/task-1 - fixture.md")).expect("read");

        let same = rename_project(&root, "Alpha", "Alpha").expect_err("same name");
        assert!(same.to_string().contains("already has that name"), "{same}");
        let unknown = rename_project(&root, "Gamma", "Delta").expect_err("unknown old");
        assert!(
            unknown.to_string().contains("no project named 'Gamma'"),
            "{unknown}"
        );
        let referenced = rename_project(&root, "Alpha", "Beta").expect_err("new referenced");
        assert!(
            referenced
                .to_string()
                .contains("already referenced by TASK-2"),
            "{referenced}"
        );
        create_project_def(
            &root,
            &NewProjectDef {
                name: "Omega".to_string(),
                ..NewProjectDef::default()
            },
        )
        .expect("second def");
        let defined = rename_project(&root, "Alpha", "Omega").expect_err("new defined");
        assert!(defined.to_string().contains("already defined"), "{defined}");
        let slug = rename_project(&root, "Alpha", "omega").expect_err("slug collision");
        assert!(slug.to_string().contains("same filename slug"), "{slug}");

        let after =
            fs::read_to_string(root.join("backlog/tasks/task-1 - fixture.md")).expect("read");
        assert_eq!(before, after, "a refusal must not touch member files");
    }
}
