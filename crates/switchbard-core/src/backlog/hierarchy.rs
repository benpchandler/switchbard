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

use super::parse::{split_frontmatter, yaml_string};
use super::write::{
    atomic_write, filename_slug, join_raw, remove_key, set_scalar, split_raw,
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

/// Create `backlog/projects/<slug>.md`. Refuses a name any existing def in
/// the repo already claims (whatever its slug), and refuses a slug collision
/// via `create_new` — same posture as task creation's "id already taken".
pub fn create_project_def(root: &Path, def: &NewProjectDef) -> Result<PathBuf> {
    let mut fields: Vec<(&str, Option<&str>)> = vec![
        ("initiative", def.initiative.as_deref()),
        ("lead", def.lead.as_deref()),
    ];
    fields.retain(|(_, v)| v.is_some());
    create_def(
        root,
        PROJECTS_DIR,
        "project",
        &def.name,
        &def.status,
        def.target_date.as_deref(),
        &fields,
        &def.description,
    )
}

pub fn create_initiative_def(root: &Path, def: &NewInitiativeDef) -> Result<PathBuf> {
    create_def(
        root,
        INITIATIVES_DIR,
        "initiative",
        &def.name,
        &def.status,
        def.target_date.as_deref(),
        &[],
        &def.description,
    )
}

#[allow(clippy::too_many_arguments)]
fn create_def(
    root: &Path,
    rel: &str,
    kind: &str,
    name: &str,
    status: &str,
    target_date: Option<&str>,
    extra_fields: &[(&str, Option<&str>)],
    description: &str,
) -> Result<PathBuf> {
    let name = validated_single_line("name", name)?;
    let status = if status.trim().is_empty() {
        DEFAULT_PROJECT_STATUS
    } else {
        validated_def_status(status)?
    };
    if let Some((_, existing)) = find_def_file(root, rel, name)? {
        bail!(
            "{kind} '{name}' is already defined at {} — edit it instead",
            existing.display()
        );
    }

    let mut fm = vec![
        format!("name: {}", yaml_scalar(name)),
        format!("status: {}", yaml_scalar(status)),
    ];
    if let Some(date) = target_date {
        fm.push(format!(
            "target_date: {}",
            yaml_scalar(validated_single_line("target_date", date)?)
        ));
    }
    for (key, value) in extra_fields {
        if let Some(value) = value {
            fm.push(format!(
                "{key}: {}",
                yaml_scalar(validated_single_line(key, value)?)
            ));
        }
    }
    let body = if description.trim().is_empty() {
        String::new()
    } else {
        format!("\n{}\n", description.trim())
    };
    let text = format!("---\n{}\n---\n{body}", fm.join("\n"));

    let dir = root.join(rel);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(format!("{}.md", filename_slug(name)));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("creating {} (slug already taken?)", path.display()))?;
    file.write_all(text.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
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
            bail!("no {kind} definition named '{name}' — run `switchbard-task {kind} create` first")
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
}
