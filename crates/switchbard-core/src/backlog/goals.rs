//! Weekly goals — `backlog/goals.yml`, one structured file per repo
//! (trajectory: *Weekly goals*, owner-approved 2026-08-31).
//!
//! Goals are **records, not documents** (the owner's storage decision):
//! a goal is a name, a unit, a measure, and a `weeks` map of
//! `{target, checkins: [{date, value}]}`. Cross-week history is one read;
//! `roll` adds a week key instead of cloning files. Values are integers —
//! a weekly goal is a count ("5 users", "8 tasks"), and integer values keep
//! every type in the snapshot `Eq`.
//!
//! Reads are **tolerant**: a missing file is an empty goal list, and a
//! malformed file warns and loads empty rather than failing the repo load —
//! the same posture as `parse_config_statuses`. Writes are **line-surgical**
//! over the file this module itself emits (precedent: `status_config.rs`
//! editing `config.yml`): check-ins append one line, `roll` inserts a week
//! block, and an edit that changes nothing writes nothing. A hand-restyled
//! file this module cannot confidently locate its edit point in fails
//! closed with an error naming the fix, never a rewrite.

use super::write::{atomic_write, validated_single_line, yaml_scalar, WriteOutcome};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const GOALS_REL: &str = "backlog/goals.yml";

/// How a goal's *actual* is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalMeasure {
    /// Reported via dated check-ins; current = the latest entry.
    Manual,
    /// Computed from tasks done within the goal week matching `scope`.
    Tasks,
}

impl GoalMeasure {
    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Tasks => "tasks",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalCheckIn {
    /// `YYYY-MM-DD`, stored as written.
    pub date: String,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GoalWeek {
    pub target: i64,
    /// Append-only observations; "current" derives from the latest entry.
    pub checkins: Vec<GoalCheckIn>,
}

/// Explicitly attached inputs a [`GoalMeasure::Tasks`] goal counts, in
/// addition to any `scope` match (owner requirement 2026-09-01: "attach
/// tasks / projects to goals as input goals").
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GoalInputs {
    /// Canonical task ids (`TASK-7`), as stored in task frontmatter.
    pub tasks: Vec<String>,
    /// Project names; every member task counts.
    pub projects: Vec<String>,
}

impl GoalInputs {
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty() && self.projects.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalDef {
    pub name: String,
    pub unit: String,
    pub measure: GoalMeasure,
    /// For [`GoalMeasure::Tasks`]: a project name or label the counted
    /// tasks must match.
    pub scope: Option<String>,
    /// Attached inputs, also counted for [`GoalMeasure::Tasks`]; empty when
    /// nothing is attached.
    pub inputs: GoalInputs,
    /// Keyed by the week's Monday (`YYYY-MM-DD`); `BTreeMap` keeps weeks
    /// chronological for free since the keys are ISO dates.
    pub weeks: BTreeMap<String, GoalWeek>,
}

impl GoalDef {
    /// Whether `task` is one of this goal's inputs: it matches `scope` (its
    /// project or one of its labels), is attached directly, or belongs to an
    /// attached project. One predicate so the Digest's actuals, `sb goal
    /// view`, and sbt's goal column can never disagree about membership;
    /// done-ness and the week window are the caller's concern.
    pub fn counts_task(&self, task: &super::types::BacklogTask) -> bool {
        let scoped = self.scope.as_deref().is_some_and(|s| {
            task.project.as_deref() == Some(s) || task.labels.iter().any(|l| l == s)
        });
        let attached = self
            .inputs
            .tasks
            .iter()
            .any(|t| t.eq_ignore_ascii_case(&task.id));
        let via_project = task
            .project
            .as_deref()
            .is_some_and(|p| self.inputs.projects.iter().any(|ip| ip == p));
        scoped || attached || via_project
    }
}

/// Names of every goal `task` feeds, in `goals` order.
pub fn goals_feeding<'a>(goals: &'a [GoalDef], task: &super::types::BacklogTask) -> Vec<&'a str> {
    goals
        .iter()
        .filter(|goal| goal.counts_task(task))
        .map(|goal| goal.name.as_str())
        .collect()
}

/// Input for [`create_goal`] — one goal with its first week.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewGoal {
    pub name: String,
    pub unit: String,
    pub measure: GoalMeasure,
    pub scope: Option<String>,
    /// The first week's Monday, `YYYY-MM-DD`.
    pub week: String,
    pub target: i64,
}

// ---- reading ----

#[derive(Deserialize)]
struct GoalsFileSer {
    #[serde(default)]
    goals: Vec<GoalSer>,
}

#[derive(Deserialize)]
struct GoalSer {
    name: String,
    #[serde(default)]
    unit: String,
    #[serde(default)]
    measure: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    inputs: Option<GoalInputsSer>,
    #[serde(default)]
    weeks: BTreeMap<String, GoalWeekSer>,
}

#[derive(Deserialize, Default)]
struct GoalInputsSer {
    #[serde(default)]
    tasks: Vec<String>,
    #[serde(default)]
    projects: Vec<String>,
}

#[derive(Deserialize)]
struct GoalWeekSer {
    target: i64,
    #[serde(default)]
    checkins: Vec<GoalCheckInSer>,
}

#[derive(Deserialize)]
struct GoalCheckInSer {
    date: String,
    value: i64,
}

/// Load `backlog/goals.yml`. Never fails the repo load: missing file is an
/// empty list; a malformed file (or an entry with an unknown `measure:`)
/// warns and is dropped.
pub(super) fn load_goals(root: &Path, warnings: &mut Vec<String>) -> Vec<GoalDef> {
    let path = root.join(GOALS_REL);
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let parsed: GoalsFileSer = match serde_yaml::from_str(&text) {
        Ok(parsed) => parsed,
        Err(err) => {
            warnings.push(format!("{}: {err}", path.display()));
            return Vec::new();
        }
    };
    let mut goals = Vec::with_capacity(parsed.goals.len());
    for goal in parsed.goals {
        let measure = match goal.measure.as_deref() {
            None | Some("manual") => GoalMeasure::Manual,
            Some("tasks") => GoalMeasure::Tasks,
            Some(other) => {
                warnings.push(format!(
                    "{}: goal '{}' has unknown measure `{other}` (expected manual or tasks) — skipped",
                    path.display(),
                    goal.name
                ));
                continue;
            }
        };
        goals.push(GoalDef {
            name: goal.name,
            unit: goal.unit,
            measure,
            scope: goal.scope,
            inputs: goal
                .inputs
                .map_or_else(GoalInputs::default, |i| GoalInputs {
                    tasks: i.tasks,
                    projects: i.projects,
                }),
            weeks: goal
                .weeks
                .into_iter()
                .map(|(week, w)| {
                    (
                        week,
                        GoalWeek {
                            target: w.target,
                            checkins: w
                                .checkins
                                .into_iter()
                                .map(|c| GoalCheckIn {
                                    date: c.date,
                                    value: c.value,
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
        });
    }
    goals
}

// ---- writing ----
//
// The emitted shape, which the surgical edits below scan for:
//
//   goals:
//     - name: Onboard users
//       unit: users
//       measure: manual
//       inputs:
//         tasks: ['TASK-61']
//         projects: ['Stack Ranking']
//       weeks:
//         2026-09-01:
//           target: 5
//           checkins:
//             - { date: 2026-09-02, value: 1 }
//
// The `inputs:` block only exists while something is attached; detaching the
// last input removes it.

const GOAL_ITEM_INDENT: &str = "  ";
const GOAL_FIELD_INDENT: &str = "    ";
const WEEK_KEY_INDENT: &str = "      ";
const WEEK_FIELD_INDENT: &str = "        ";
const CHECKIN_ITEM_INDENT: &str = "          ";

fn goals_path(root: &Path) -> PathBuf {
    root.join(GOALS_REL)
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
    // which a brand-new goals.yml doesn't have — same tmp-then-rename
    // atomicity, default permissions.
    let tmp = path.with_extension("yml.tmp");
    fs::write(&tmp, &text).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("creating {}", path.display()))
}

fn validated_week(week: &str) -> Result<&str> {
    let week = validated_single_line("week", week)?;
    if chrono::NaiveDate::parse_from_str(week, "%Y-%m-%d").is_err() {
        bail!("week must be a YYYY-MM-DD date (the week's Monday), got `{week}`");
    }
    Ok(week)
}

fn validated_date(date: &str) -> Result<&str> {
    let date = validated_single_line("date", date)?;
    if chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
        bail!("date must be YYYY-MM-DD, got `{date}`");
    }
    Ok(date)
}

fn goal_week_block(week: &str, target: i64) -> Vec<String> {
    vec![
        format!("{WEEK_KEY_INDENT}{week}:"),
        format!("{WEEK_FIELD_INDENT}target: {target}"),
        format!("{WEEK_FIELD_INDENT}checkins: []"),
    ]
}

/// Append a goal (with its first week) to `backlog/goals.yml`, creating the
/// file when absent. Refuses a duplicate name.
pub fn create_goal(root: &Path, goal: &NewGoal) -> Result<()> {
    let name = validated_single_line("name", &goal.name)?;
    let unit = validated_single_line("unit", &goal.unit)?;
    let week = validated_week(&goal.week)?;
    // A tasks-measured goal may start with neither scope nor inputs — its
    // actual is 0 until `attach_goal_inputs` (or a later scope) gives it
    // something to count. The CLI surfaces that hint.

    let mut warnings = Vec::new();
    if load_goals(root, &mut warnings)
        .iter()
        .any(|existing| existing.name == name)
    {
        bail!("goal '{name}' already exists — check in with `goal check-in`, or extend it with `goal roll`");
    }

    let path = goals_path(root);
    let mut lines = if path.is_file() {
        read_lines(&path)?
    } else {
        fs::create_dir_all(path.parent().expect("goals.yml has a parent"))
            .with_context(|| format!("creating {}", root.join("backlog").display()))?;
        vec![
            "# Weekly goals — written by switchbard (`goal` commands); one record per goal,"
                .to_string(),
            "# targets and dated check-ins per week. See docs/product-trajectory.md.".to_string(),
            "goals:".to_string(),
        ]
    };
    if !lines.iter().any(|l| l.trim_end() == "goals:") {
        bail!(
            "{} has no `goals:` key — fix the file (or remove it to start fresh)",
            path.display()
        );
    }

    lines.push(format!("{GOAL_ITEM_INDENT}- name: {}", yaml_scalar(name)));
    lines.push(format!("{GOAL_FIELD_INDENT}unit: {}", yaml_scalar(unit)));
    lines.push(format!(
        "{GOAL_FIELD_INDENT}measure: {}",
        goal.measure.label()
    ));
    if let Some(scope) = &goal.scope {
        lines.push(format!(
            "{GOAL_FIELD_INDENT}scope: {}",
            yaml_scalar(validated_single_line("scope", scope)?)
        ));
    }
    lines.push(format!("{GOAL_FIELD_INDENT}weeks:"));
    lines.extend(goal_week_block(week, goal.target));
    write_lines(&path, &lines)
}

/// Append one dated observation to a goal's week. Fails closed when the
/// file's structure isn't one this module emitted (it never rewrites what
/// it cannot confidently locate).
pub fn check_in_goal(root: &Path, name: &str, week: &str, date: &str, value: i64) -> Result<()> {
    let name = validated_single_line("name", name)?;
    let week = validated_week(week)?;
    let date = validated_date(date)?;

    let path = goals_path(root);
    if !path.is_file() {
        bail!("no goals defined yet — run `goal create` first");
    }
    let mut lines = read_lines(&path)?;
    let (goal_start, goal_end) = goal_span(&lines, name)?;
    let Some(week_line) = (goal_start..goal_end)
        .find(|&i| lines[i].trim_end() == format!("{WEEK_KEY_INDENT}{week}:"))
    else {
        bail!("goal '{name}' has no week {week} — add it with `goal roll --week {week}`");
    };
    // The week block ends at the next line at week-key indent or shallower.
    let week_end = ((week_line + 1)..goal_end)
        .find(|&i| indent_of(&lines[i]) <= WEEK_KEY_INDENT.len())
        .unwrap_or(goal_end);

    let item = format!("{CHECKIN_ITEM_INDENT}- {{ date: {date}, value: {value} }}");
    let empty_marker = format!("{WEEK_FIELD_INDENT}checkins: []");
    let header = format!("{WEEK_FIELD_INDENT}checkins:");
    if let Some(i) = ((week_line + 1)..week_end).find(|&i| lines[i].trim_end() == empty_marker) {
        lines.splice(i..=i, [header, item]);
    } else if let Some(i) = ((week_line + 1)..week_end).find(|&i| lines[i].trim_end() == header) {
        // Append after the last existing check-in line.
        let insert_at = ((i + 1)..week_end)
            .take_while(|&j| indent_of(&lines[j]) > WEEK_FIELD_INDENT.len())
            .last()
            .map_or(i + 1, |j| j + 1);
        lines.insert(insert_at, item);
    } else {
        bail!(
            "{}: week {week} of goal '{name}' has no recognizable `checkins:` — restore the emitted structure before checking in",
            path.display()
        );
    }
    write_lines(&path, &lines)
}

/// Change a goal's target for one week — the pencil "Edit target" affordance
/// (Goals index row, goal page). Line-surgical like every other edit here:
/// the week must already exist (`goal roll` adds a week key; this never
/// creates one), and a target line this module didn't emit fails closed
/// rather than guessing where to write.
pub fn edit_goal_target(root: &Path, name: &str, week: &str, new_target: i64) -> Result<()> {
    let name = validated_single_line("name", name)?;
    let week = validated_week(week)?;
    if new_target < 0 {
        bail!("target must be zero or greater, got {new_target}");
    }

    let path = goals_path(root);
    if !path.is_file() {
        bail!("no goals defined yet — run `goal create` first");
    }
    let mut lines = read_lines(&path)?;
    let (goal_start, goal_end) = goal_span(&lines, name)?;
    let Some(week_line) = (goal_start..goal_end)
        .find(|&i| lines[i].trim_end() == format!("{WEEK_KEY_INDENT}{week}:"))
    else {
        bail!("goal '{name}' has no week {week} — add it with `goal roll --week {week}`");
    };
    let target_line = week_line + 1;
    let expected_prefix = format!("{WEEK_FIELD_INDENT}target: ");
    if target_line >= goal_end || !lines[target_line].starts_with(&expected_prefix) {
        bail!(
            "{}: week {week} of goal '{name}' has no recognizable `target:` — restore the emitted structure before editing",
            path.display()
        );
    }
    let new_line = format!("{WEEK_FIELD_INDENT}target: {new_target}");
    if lines[target_line] == new_line {
        return Ok(()); // already this value — nothing to write
    }
    lines[target_line] = new_line;
    write_lines(&path, &lines)
}

fn inputs_block(inputs: &GoalInputs) -> Vec<String> {
    // Always single-quote flow items: `yaml_scalar` decides quoting for
    // block scalars, but inside `[...]` an unquoted comma would split the
    // item. Quoting everything is safe for both emit and reload.
    let flow = |items: &[String]| {
        let quoted: Vec<String> = items
            .iter()
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect();
        format!("[{}]", quoted.join(", "))
    };
    vec![
        format!("{GOAL_FIELD_INDENT}inputs:"),
        format!("{WEEK_KEY_INDENT}tasks: {}", flow(&inputs.tasks)),
        format!("{WEEK_KEY_INDENT}projects: {}", flow(&inputs.projects)),
    ]
}

/// Replace (or remove, when `new_inputs` is empty) the goal's `inputs:`
/// block, inserting before `weeks:` when absent. Line-surgical like every
/// other write here: fails closed on a structure this module didn't emit.
fn write_goal_inputs(root: &Path, name: &str, new_inputs: &GoalInputs) -> Result<()> {
    let path = goals_path(root);
    let mut lines = read_lines(&path)?;
    let (goal_start, goal_end) = goal_span(&lines, name)?;
    let block = if new_inputs.is_empty() {
        Vec::new()
    } else {
        inputs_block(new_inputs)
    };
    let header = format!("{GOAL_FIELD_INDENT}inputs:");
    if let Some(start) = (goal_start..goal_end).find(|&i| lines[i].trim_end() == header) {
        let end = ((start + 1)..goal_end)
            .find(|&i| indent_of(&lines[i]) <= GOAL_FIELD_INDENT.len())
            .unwrap_or(goal_end);
        lines.splice(start..end, block);
    } else if block.is_empty() {
        return Ok(()); // nothing attached, nothing stored — nothing to write
    } else {
        let Some(weeks_line) = (goal_start..goal_end)
            .find(|&i| lines[i].trim_end() == format!("{GOAL_FIELD_INDENT}weeks:"))
        else {
            bail!(
                "{}: goal '{name}' has no recognizable `weeks:` — restore the emitted structure before attaching inputs",
                path.display()
            );
        };
        lines.splice(weeks_line..weeks_line, block);
    }
    write_lines(&path, &lines)
}

/// Follow a task move into `goals.yml`: every `inputs.tasks` entry naming
/// `old` (ids compare case-insensitively, as `attach_goal_inputs` does) now
/// names `new`. A missing file, or one that never mentions `old`, is
/// `Unchanged`.
pub(super) fn rename_task_in_goals(root: &Path, old: &str, new: &str) -> Result<WriteOutcome> {
    let path = goals_path(root);
    if !path.is_file() {
        return Ok(WriteOutcome::Unchanged);
    }
    let mut warnings = Vec::new();
    let goals = load_goals(root, &mut warnings);
    if !warnings.is_empty() {
        bail!(
            "{} does not parse cleanly - fix it before moving tasks: {}",
            path.display(),
            warnings.join("; ")
        );
    }
    let mut changed = false;
    for goal in goals {
        if !goal
            .inputs
            .tasks
            .iter()
            .any(|t| t.eq_ignore_ascii_case(old))
        {
            continue;
        }
        let mut inputs = goal.inputs.clone();
        let mut renamed: Vec<String> = Vec::with_capacity(inputs.tasks.len());
        for task in inputs.tasks.drain(..) {
            let task = if task.eq_ignore_ascii_case(old) {
                new.to_string()
            } else {
                task
            };
            if !renamed.iter().any(|t| t.eq_ignore_ascii_case(&task)) {
                renamed.push(task);
            }
        }
        inputs.tasks = renamed;
        write_goal_inputs(root, &goal.name, &inputs)?;
        changed = true;
    }
    Ok(if changed {
        WriteOutcome::Changed
    } else {
        WriteOutcome::Unchanged
    })
}

/// Follow a project rename into `goals.yml`: a goal's `scope:` equal to
/// `old` (a scope is a project name or a label; a label spelled exactly like
/// the project is read as the project here) and any `inputs.projects` entry.
/// A missing file, or one that never mentions `old`, is `Unchanged`.
pub(super) fn rename_project_in_goals(root: &Path, old: &str, new: &str) -> Result<WriteOutcome> {
    let path = goals_path(root);
    if !path.is_file() {
        return Ok(WriteOutcome::Unchanged);
    }
    let mut warnings = Vec::new();
    let goals = load_goals(root, &mut warnings);
    if !warnings.is_empty() {
        bail!(
            "{} does not parse cleanly - fix it before renaming: {}",
            path.display(),
            warnings.join("; ")
        );
    }
    let mut changed = false;
    for goal in goals {
        if goal.scope.as_deref() == Some(old) {
            let mut lines = read_lines(&path)?;
            let (start, end) = goal_span(&lines, &goal.name)?;
            let needle = format!("{GOAL_FIELD_INDENT}scope: {}", yaml_scalar(old));
            let Some(at) = (start..end).find(|&i| lines[i].trim_end() == needle) else {
                bail!(
                    "{}: cannot locate goal '{}'s `scope:` line - restore the emitted structure first",
                    path.display(),
                    goal.name
                );
            };
            lines[at] = format!("{GOAL_FIELD_INDENT}scope: {}", yaml_scalar(new));
            write_lines(&path, &lines)?;
            changed = true;
        }
        if goal.inputs.projects.iter().any(|p| p == old) {
            let mut inputs = goal.inputs.clone();
            let mut renamed = Vec::with_capacity(inputs.projects.len());
            for project in inputs.projects.drain(..) {
                let project = if project == old {
                    new.to_string()
                } else {
                    project
                };
                if !renamed.contains(&project) {
                    renamed.push(project);
                }
            }
            inputs.projects = renamed;
            write_goal_inputs(root, &goal.name, &inputs)?;
            changed = true;
        }
    }
    Ok(if changed {
        WriteOutcome::Changed
    } else {
        WriteOutcome::Unchanged
    })
}

/// Look up a goal by name, insisting the file parses cleanly first (an
/// inputs edit rewrites the block, so a half-understood file is unsafe).
fn parsed_goal(root: &Path, name: &str) -> Result<GoalDef> {
    let path = goals_path(root);
    if !path.is_file() {
        bail!("no goals defined yet — run `goal create` first");
    }
    let mut warnings = Vec::new();
    let goals = load_goals(root, &mut warnings);
    if !warnings.is_empty() {
        bail!(
            "{} does not parse cleanly — fix it first: {}",
            path.display(),
            warnings.join("; ")
        );
    }
    goals
        .into_iter()
        .find(|g| g.name == name)
        .with_context(|| format!("no goal '{name}' — check `goal list` for the exact name"))
}

/// Attach tasks and/or projects to a tasks-measured goal as counted inputs.
/// Duplicates dedupe silently; returns how many inputs were actually added
/// (0 means everything was already attached and nothing was written).
pub fn attach_goal_inputs(
    root: &Path,
    name: &str,
    tasks: &[String],
    projects: &[String],
) -> Result<usize> {
    let name = validated_single_line("name", name)?;
    if tasks.is_empty() && projects.is_empty() {
        bail!("nothing to attach — pass --task <ID> and/or --in-project <NAME>");
    }
    let goal = parsed_goal(root, name)?;
    if goal.measure == GoalMeasure::Manual {
        bail!(
            "goal '{name}' is measured by manual check-ins — inputs only apply to `--measure tasks` goals"
        );
    }
    let mut inputs = goal.inputs.clone();
    let mut added = 0usize;
    for task in tasks {
        let task = validated_single_line("task id", task)?;
        if !inputs.tasks.iter().any(|t| t.eq_ignore_ascii_case(task)) {
            inputs.tasks.push(task.to_string());
            added += 1;
        }
    }
    for project in projects {
        let project = validated_single_line("project", project)?;
        if !inputs.projects.iter().any(|p| p == project) {
            inputs.projects.push(project.to_string());
            added += 1;
        }
    }
    if added > 0 {
        write_goal_inputs(root, name, &inputs)?;
    }
    Ok(added)
}

/// Detach previously attached inputs. Errors when none of the named inputs
/// were attached (a likely typo); otherwise removes what matches and returns
/// the count, dropping the whole `inputs:` block when it empties.
pub fn detach_goal_inputs(
    root: &Path,
    name: &str,
    tasks: &[String],
    projects: &[String],
) -> Result<usize> {
    let name = validated_single_line("name", name)?;
    if tasks.is_empty() && projects.is_empty() {
        bail!("nothing to detach — pass --task <ID> and/or --in-project <NAME>");
    }
    let goal = parsed_goal(root, name)?;
    let mut inputs = goal.inputs.clone();
    let before = inputs.tasks.len() + inputs.projects.len();
    inputs
        .tasks
        .retain(|t| !tasks.iter().any(|arg| arg.trim().eq_ignore_ascii_case(t)));
    inputs
        .projects
        .retain(|p| !projects.iter().any(|arg| arg.trim() == p));
    let removed = before - (inputs.tasks.len() + inputs.projects.len());
    if removed == 0 {
        bail!("none of those inputs are attached to '{name}' — see `goal view` for what is");
    }
    write_goal_inputs(root, name, &inputs)?;
    Ok(removed)
}

/// Give every goal that lacks `to_week` a new week block carrying its most
/// recent earlier target. Returns how many goals were rolled; rolling when
/// every goal already has the week is a no-op that writes nothing.
pub fn roll_goals(root: &Path, to_week: &str) -> Result<usize> {
    let to_week = validated_week(to_week)?;
    let path = goals_path(root);
    if !path.is_file() {
        bail!("no goals defined yet — run `goal create` first");
    }
    let mut warnings = Vec::new();
    let goals = load_goals(root, &mut warnings);
    if !warnings.is_empty() {
        bail!(
            "{} does not parse cleanly — fix it before rolling: {}",
            path.display(),
            warnings.join("; ")
        );
    }

    let mut lines = read_lines(&path)?;
    let mut rolled = 0usize;
    for goal in &goals {
        if goal.weeks.contains_key(to_week) {
            continue;
        }
        // The most recent week before `to_week` (ISO keys sort correctly).
        let Some((_, source)) = goal
            .weeks
            .range::<str, _>((
                std::ops::Bound::Unbounded,
                std::ops::Bound::Excluded(to_week),
            ))
            .next_back()
        else {
            continue; // only future weeks exist; nothing to carry forward
        };
        let (goal_start, goal_end) = goal_span(&lines, &goal.name)?;
        let Some(weeks_line) = (goal_start..goal_end)
            .find(|&i| lines[i].trim_end() == format!("{GOAL_FIELD_INDENT}weeks:"))
        else {
            bail!(
                "{}: goal '{}' has no recognizable `weeks:` — restore the emitted structure before rolling",
                path.display(),
                goal.name
            );
        };
        // Insert at the end of the weeks section (chronology holds because
        // rolls only ever add the newest week).
        let insert_at = ((weeks_line + 1)..goal_end)
            .take_while(|&i| indent_of(&lines[i]) > GOAL_FIELD_INDENT.len())
            .last()
            .map_or(weeks_line + 1, |i| i + 1);
        lines.splice(
            insert_at..insert_at,
            goal_week_block(to_week, source.target),
        );
        rolled += 1;
    }
    if rolled > 0 {
        write_lines(&path, &lines)?;
    }
    Ok(rolled)
}

/// `[start, end)` of the goal item whose `name:` matches. Matching is
/// against this module's own emitted line (`- name: <yaml_scalar>`), so a
/// hand-restyled entry fails closed rather than mislocating an edit.
fn goal_span(lines: &[String], name: &str) -> Result<(usize, usize)> {
    let needle = format!("{GOAL_ITEM_INDENT}- name: {}", yaml_scalar(name));
    let start = lines
        .iter()
        .position(|l| l.trim_end() == needle)
        .with_context(|| {
            format!("cannot locate goal '{name}' in backlog/goals.yml — check `goal list` for the exact name")
        })?;
    let end = ((start + 1)..lines.len())
        .find(|&i| {
            let indent = indent_of(&lines[i]);
            !lines[i].trim().is_empty() && indent <= GOAL_ITEM_INDENT.len()
        })
        .unwrap_or(lines.len());
    Ok((start, end))
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
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

    fn goal(name: &str, week: &str, target: i64) -> NewGoal {
        NewGoal {
            name: name.to_string(),
            unit: "users".to_string(),
            measure: GoalMeasure::Manual,
            scope: None,
            week: week.to_string(),
            target,
        }
    }

    fn load(root: &Path) -> (Vec<GoalDef>, Vec<String>) {
        let mut warnings = Vec::new();
        let goals = load_goals(root, &mut warnings);
        (goals, warnings)
    }

    #[test]
    fn create_check_in_and_reload_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        create_goal(&root, &goal("Onboard users", "2026-09-01", 5)).expect("create");
        check_in_goal(&root, "Onboard users", "2026-09-01", "2026-09-02", 1).expect("check in");
        check_in_goal(&root, "Onboard users", "2026-09-01", "2026-09-04", 4).expect("check in");

        let (goals, warnings) = load(&root);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(goals.len(), 1);
        let week = &goals[0].weeks["2026-09-01"];
        assert_eq!(week.target, 5);
        assert_eq!(
            week.checkins,
            vec![
                GoalCheckIn {
                    date: "2026-09-02".to_string(),
                    value: 1
                },
                GoalCheckIn {
                    date: "2026-09-04".to_string(),
                    value: 4
                },
            ],
            "check-ins append in order"
        );
    }

    #[test]
    fn create_refuses_duplicates_and_scopeless_tasks_goals_are_allowed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        create_goal(&root, &goal("Twice", "2026-09-01", 3)).expect("create");
        let err = create_goal(&root, &goal("Twice", "2026-09-08", 3)).expect_err("dup");
        assert!(err.to_string().contains("already exists"), "{err}");

        // A tasks goal may start scopeless: inputs get attached afterwards.
        let mut tasks_goal = goal("Scoped later", "2026-09-01", 8);
        tasks_goal.measure = GoalMeasure::Tasks;
        create_goal(&root, &tasks_goal).expect("scopeless tasks goal");
        let (goals, warnings) = load(&root);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(goals.iter().any(|g| g.name == "Scoped later"));
    }

    #[test]
    fn attach_detach_round_trip_is_surgical_and_dedupes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        let mut g = goal("Inputs", "2026-09-01", 4);
        g.measure = GoalMeasure::Tasks;
        create_goal(&root, &g).expect("create");
        create_goal(&root, &goal("Bystander", "2026-09-01", 2)).expect("create");
        let before = fs::read_to_string(root.join(GOALS_REL)).expect("read");

        let added = attach_goal_inputs(
            &root,
            "Inputs",
            &["TASK-61".to_string()],
            &["Stack Ranking".to_string()],
        )
        .expect("attach");
        assert_eq!(added, 2);
        let after = fs::read_to_string(root.join(GOALS_REL)).expect("read");
        let changed: Vec<&str> = after.lines().filter(|l| !before.contains(*l)).collect();
        assert_eq!(
            changed,
            vec![
                "    inputs:",
                "      tasks: ['TASK-61']",
                "      projects: ['Stack Ranking']",
            ],
            "only the inputs block appears"
        );

        // Re-attaching the same things adds nothing and writes nothing.
        let unchanged = fs::read_to_string(root.join(GOALS_REL)).expect("read");
        let added = attach_goal_inputs(
            &root,
            "Inputs",
            &["task-61".to_string()],
            &["Stack Ranking".to_string()],
        )
        .expect("re-attach");
        assert_eq!(added, 0);
        assert_eq!(
            unchanged,
            fs::read_to_string(root.join(GOALS_REL)).expect("read")
        );

        let (goals, _) = load(&root);
        let loaded = goals.iter().find(|g| g.name == "Inputs").expect("goal");
        assert_eq!(loaded.inputs.tasks, vec!["TASK-61"]);
        assert_eq!(loaded.inputs.projects, vec!["Stack Ranking"]);

        // Detaching everything drops the whole block.
        let removed = detach_goal_inputs(
            &root,
            "Inputs",
            &["task-61".to_string()],
            &["Stack Ranking".to_string()],
        )
        .expect("detach");
        assert_eq!(removed, 2);
        let final_text = fs::read_to_string(root.join(GOALS_REL)).expect("read");
        assert!(!final_text.contains("inputs:"), "{final_text}");
        let (goals, warnings) = load(&root);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(goals
            .iter()
            .find(|g| g.name == "Inputs")
            .expect("goal")
            .inputs
            .is_empty());
    }

    #[test]
    fn attach_guards_manual_goals_empty_args_and_bad_detaches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        create_goal(&root, &goal("Manual", "2026-09-01", 5)).expect("create");
        let mut g = goal("Tasks", "2026-09-01", 5);
        g.measure = GoalMeasure::Tasks;
        create_goal(&root, &g).expect("create");

        let err = attach_goal_inputs(&root, "Manual", &["TASK-1".to_string()], &[])
            .expect_err("manual refuses inputs");
        assert!(err.to_string().contains("check-ins"), "{err}");

        let err = attach_goal_inputs(&root, "Tasks", &[], &[]).expect_err("nothing to attach");
        assert!(err.to_string().contains("--task"), "{err}");

        let err = detach_goal_inputs(&root, "Tasks", &["TASK-9".to_string()], &[])
            .expect_err("nothing attached");
        assert!(err.to_string().contains("goal view"), "{err}");
    }

    #[test]
    fn roll_carries_the_latest_target_and_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        create_goal(&root, &goal("Onboard users", "2026-09-01", 5)).expect("create");
        create_goal(&root, &goal("Ship things", "2026-09-01", 2)).expect("create");

        assert_eq!(roll_goals(&root, "2026-09-08").expect("roll"), 2);
        let before = fs::read_to_string(root.join(GOALS_REL)).expect("read");
        assert_eq!(roll_goals(&root, "2026-09-08").expect("re-roll"), 0);
        let after = fs::read_to_string(root.join(GOALS_REL)).expect("read");
        assert_eq!(before, after, "an all-rolled roll writes nothing");

        let (goals, _) = load(&root);
        assert_eq!(goals[0].weeks["2026-09-08"].target, 5);
        assert!(goals[0].weeks["2026-09-08"].checkins.is_empty());
        // Check-ins still land in the right week after a roll.
        check_in_goal(&root, "Onboard users", "2026-09-08", "2026-09-09", 2).expect("check in");
        let (goals, _) = load(&root);
        assert_eq!(goals[0].weeks["2026-09-08"].checkins.len(), 1);
        assert!(goals[0].weeks["2026-09-01"].checkins.is_empty());
    }

    #[test]
    fn edits_are_surgical_around_untouched_goals() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        create_goal(&root, &goal("First", "2026-09-01", 5)).expect("create");
        create_goal(&root, &goal("Second", "2026-09-01", 3)).expect("create");
        let before = fs::read_to_string(root.join(GOALS_REL)).expect("read");

        check_in_goal(&root, "First", "2026-09-01", "2026-09-02", 1).expect("check in");
        let after = fs::read_to_string(root.join(GOALS_REL)).expect("read");
        let changed: Vec<&str> = after.lines().filter(|l| !before.contains(*l)).collect();
        assert_eq!(
            changed,
            vec!["          - { date: 2026-09-02, value: 1 }"],
            "exactly one line appears; every other byte survives"
        );
    }

    #[test]
    fn malformed_files_warn_and_load_empty_and_missing_files_are_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        let (goals, warnings) = load(&root);
        assert!(
            goals.is_empty() && warnings.is_empty(),
            "missing file is silent"
        );

        fs::write(root.join(GOALS_REL), "goals: [not: [valid\n").expect("write");
        let (goals, warnings) = load(&root);
        assert!(goals.is_empty());
        assert_eq!(warnings.len(), 1, "{warnings:?}");

        fs::write(
            root.join(GOALS_REL),
            "goals:\n  - name: Odd\n    measure: fortnightly\n    weeks: {}\n",
        )
        .expect("write");
        let (goals, warnings) = load(&root);
        assert!(goals.is_empty(), "unknown measure skips the goal");
        assert!(warnings[0].contains("fortnightly"), "{warnings:?}");
    }

    #[test]
    fn edit_target_is_surgical_idempotent_and_fails_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        create_goal(&root, &goal("Onboard users", "2026-09-01", 5)).expect("create");
        create_goal(&root, &goal("Bystander", "2026-09-01", 2)).expect("create");
        check_in_goal(&root, "Onboard users", "2026-09-01", "2026-09-02", 1).expect("check in");
        let before = fs::read_to_string(root.join(GOALS_REL)).expect("read");

        edit_goal_target(&root, "Onboard users", "2026-09-01", 8).expect("edit");
        let after = fs::read_to_string(root.join(GOALS_REL)).expect("read");
        let changed: Vec<&str> = after
            .lines()
            .zip(before.lines())
            .filter(|(a, b)| a != b)
            .map(|(a, _)| a)
            .collect();
        assert_eq!(
            changed,
            vec!["        target: 8"],
            "exactly the target line changes, check-ins and the other goal survive"
        );
        let (goals, warnings) = load(&root);
        assert!(warnings.is_empty(), "{warnings:?}");
        let g = goals
            .iter()
            .find(|g| g.name == "Onboard users")
            .expect("goal");
        assert_eq!(g.weeks["2026-09-01"].target, 8);
        assert_eq!(
            g.weeks["2026-09-01"].checkins.len(),
            1,
            "check-ins untouched"
        );

        // Re-applying the same target writes nothing.
        let unchanged = fs::read_to_string(root.join(GOALS_REL)).expect("read");
        edit_goal_target(&root, "Onboard users", "2026-09-01", 8).expect("re-edit");
        assert_eq!(
            unchanged,
            fs::read_to_string(root.join(GOALS_REL)).expect("read")
        );

        let err =
            edit_goal_target(&root, "Onboard users", "2026-09-08", 3).expect_err("missing week");
        assert!(err.to_string().contains("goal roll"), "{err}");
        let err = edit_goal_target(&root, "Ghost", "2026-09-01", 3).expect_err("unknown goal");
        assert!(err.to_string().contains("goal list"), "{err}");
        let err = edit_goal_target(&root, "Onboard users", "2026-09-01", -1)
            .expect_err("negative target");
        assert!(err.to_string().contains("zero or greater"), "{err}");
    }

    #[test]
    fn check_in_errors_name_the_next_step() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = repo(&dir);
        let err =
            check_in_goal(&root, "Ghost", "2026-09-01", "2026-09-02", 1).expect_err("no file yet");
        assert!(err.to_string().contains("goal create"), "{err}");

        create_goal(&root, &goal("Real", "2026-09-01", 5)).expect("create");
        let err =
            check_in_goal(&root, "Real", "2026-09-08", "2026-09-09", 1).expect_err("missing week");
        assert!(err.to_string().contains("goal roll"), "{err}");
        let err =
            check_in_goal(&root, "Ghost", "2026-09-01", "2026-09-02", 1).expect_err("unknown goal");
        assert!(err.to_string().contains("goal list"), "{err}");
    }
}
