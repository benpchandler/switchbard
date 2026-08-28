//! Native task ID allocation — the format fork's replacement for the
//! `backlog` CLI's `check_active_branches` collision scan.
//!
//! # What counts toward "the next id"
//!
//! [`next_task_id`] returns `1 + max` over every task id visible from this
//! machine:
//!
//! 1. **Every worktree of the repo** (via `crate::worktree::
//!    enumerate_worktrees`), scanning the four backlog directories the loader
//!    reads (`tasks`, `completed`, `drafts`, `archive/tasks`) — so a task
//!    file sitting *uncommitted* in a sibling worktree still blocks its id.
//!    A plain non-git directory degrades to scanning the project root alone.
//! 2. **Local branches with commit activity in the last
//!    [`ACTIVE_BRANCH_DAYS`] days** (`git for-each-ref` + `git ls-tree`,
//!    both through [`crate::git_cmd`]) — so a task committed on an unmerged
//!    branch whose worktree was already removed still blocks its id. The
//!    30-day window and the "active branches" idea match the `backlog` CLI
//!    defaults this replaces (`check_active_branches` /
//!    `active_branch_days: 30` in `backlog/config.yml`).
//!
//! **Deliberately out of scope: remote branches.** The CLI's
//! `remote_operations` fetch-and-scan is not replicated. Task creation in
//! this fleet happens on one machine (GUI, `switchbard-dispatch`, the
//! terminal); an id minted on another machine collides, at worst, as a
//! visible conflict at PR review — the same place every other cross-machine
//! conflict is resolved. Fetching inside an allocator would make "create a
//! task" block on the network.
//!
//! # Concurrency: reservations in the git common dir
//!
//! Scanning alone leaves a window: two dispatchers can both read "max is 41"
//! and both mint 42 — and in *different worktrees* the two files never even
//! collide on the filesystem. The claim primitive that closes this is a
//! reservation file **named by the id alone**, `create_new`'d in the repo's
//! shared git common dir (`.git/switchbard/`), which every worktree of the
//! repo sees. Exactly one process can hold `task-42.reserve` at a time;
//! [`super::write::write_new_task_file`]'s own `create_new` remains the
//! in-directory backstop. A reservation is held only for the moments between
//! allocation and file creation; one left behind by a crash goes stale after
//! [`RESERVATION_STALE_SECS`] and is stolen (an unparseable reservation is
//! treated as stale too — refusing to would brick that id forever, and the
//! cost of a wrong steal is a reservation, not data).

use super::types::NewBacklogTask;
use super::write::write_new_task_file;
use crate::git_cmd;
use crate::worktree::enumerate_worktrees;
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A branch counts as "active" (and gets its task files scanned) when its
/// tip commit is at most this many days old — the `backlog` CLI's own
/// default window.
pub const ACTIVE_BRANCH_DAYS: u64 = 30;

/// Upper bound on branches scanned per allocation, newest first. A repo with
/// more simultaneously active branches than this is not a shape this fleet
/// has; the cap keeps one allocation from turning into hundreds of git
/// calls.
const MAX_BRANCHES_SCANNED: usize = 100;

/// Upper bound on reserve-and-create attempts before giving up.
const MAX_CREATE_ATTEMPTS: u32 = 20;

/// A reservation older than this is presumed abandoned by a crashed process
/// and may be stolen.
const RESERVATION_STALE_SECS: u64 = 60;

/// The four directories task files live in — the same set
/// `super::parse::load_backlog_project` reads.
const TASK_DIRS: [&str; 4] = ["tasks", "completed", "drafts", "archive/tasks"];

/// The next unclaimed task id for this repo. See the module doc for exactly
/// what "claimed" covers (and doesn't).
pub fn next_task_id(repo_root: &Path) -> Result<u32> {
    let from_worktrees = max_id_across_worktrees(repo_root);
    let from_branches = max_id_on_active_branches(repo_root, unix_now_secs())?;
    Ok(from_worktrees.max(from_branches) + 1)
}

/// Allocate an id, claim it, and create the task file in
/// `<repo_root>/backlog/tasks`. Returns the id and the created path.
///
/// On any conflict — the id already reserved by another process, or a file
/// already carrying it — the candidate id is bumped and the claim retried,
/// bounded by [`MAX_CREATE_ATTEMPTS`]. An existing file is never touched.
pub fn create_task_allocating_id(
    repo_root: &Path,
    task: &NewBacklogTask,
) -> Result<(u32, PathBuf)> {
    let tasks_dir = repo_root.join("backlog/tasks");
    fs::create_dir_all(&tasks_dir).with_context(|| format!("creating {}", tasks_dir.display()))?;
    let reservations = reservation_dir(repo_root);
    let mut id = next_task_id(repo_root)?;
    for _attempt in 0..MAX_CREATE_ATTEMPTS {
        let Some(_claim) = try_reserve(&reservations, id, unix_now_secs())? else {
            id += 1;
            continue;
        };
        if dir_has_task_id(&tasks_dir, id)? {
            id += 1;
            continue;
        }
        let path = write_new_task_file(&tasks_dir, id, task)?;
        return Ok((id, path));
    }
    bail!("could not claim a task id after {MAX_CREATE_ATTEMPTS} attempts")
}

// ---- scanning ----

fn max_id_across_worktrees(repo_root: &Path) -> u32 {
    let mut roots: BTreeSet<PathBuf> = BTreeSet::new();
    roots.insert(repo_root.to_path_buf());
    for worktree in enumerate_worktrees(repo_root).unwrap_or_default() {
        roots.insert(worktree.path);
    }
    roots
        .iter()
        .map(|root| max_id_in_project(root))
        .max()
        .unwrap_or(0)
}

fn max_id_in_project(root: &Path) -> u32 {
    let mut max = 0;
    for dir in TASK_DIRS {
        let Ok(entries) = fs::read_dir(root.join("backlog").join(dir)) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            if let Some(id) = filename_task_id(&name.to_string_lossy()) {
                max = max.max(id);
            }
        }
    }
    max
}

/// The integer task id a filename (or ls-tree basename) carries:
/// `task-42 - Title.md` → 42, `task-150.10 - Sub.md` → 150 (subtask ids
/// share their parent's integer namespace). Not a task filename → `None`.
fn filename_task_id(name: &str) -> Option<u32> {
    let rest = name
        .strip_prefix("task-")
        .or_else(|| name.strip_prefix("TASK-"))?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn max_id_on_active_branches(repo_root: &Path, now: u64) -> Result<u32> {
    let Some(root) = repo_root.to_str() else {
        return Ok(0);
    };
    let output = git_cmd()
        .args([
            "-C",
            root,
            "for-each-ref",
            "refs/heads",
            "--sort=-committerdate",
            "--format=%(refname:short)\t%(committerdate:unix)",
        ])
        .output()
        .context("running git for-each-ref")?;
    if !output.status.success() {
        // Not a git repo (or one with no refs yet): branches hold nothing.
        return Ok(0);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut max = 0;
    for branch in parse_active_branches(&text, now, ACTIVE_BRANCH_DAYS) {
        max = max.max(max_id_on_branch(root, &branch)?);
    }
    Ok(max)
}

/// Pure parse of `for-each-ref --format='%(refname:short)\t%(committerdate:
/// unix)'` output: branch names whose tip is within `days`, newest first,
/// capped at [`MAX_BRANCHES_SCANNED`]. Malformed lines are skipped.
fn parse_active_branches(text: &str, now: u64, days: u64) -> Vec<String> {
    let cutoff = now.saturating_sub(days * 24 * 60 * 60);
    text.lines()
        .filter_map(|line| {
            let (name, stamp) = line.split_once('\t')?;
            let stamp: u64 = stamp.trim().parse().ok()?;
            (stamp >= cutoff && !name.is_empty()).then(|| name.to_string())
        })
        .take(MAX_BRANCHES_SCANNED)
        .collect()
}

fn max_id_on_branch(root: &str, branch: &str) -> Result<u32> {
    let mut args = vec!["-C", root, "ls-tree", "-r", "--name-only", branch, "--"];
    let dirs: Vec<String> = TASK_DIRS.iter().map(|d| format!("backlog/{d}")).collect();
    args.extend(dirs.iter().map(String::as_str));
    let output = git_cmd()
        .args(&args)
        .output()
        .with_context(|| format!("running git ls-tree on {branch}"))?;
    if !output.status.success() {
        // A ref that vanished between the two git calls is a non-event.
        return Ok(0);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .lines()
        .filter_map(|path| filename_task_id(path.rsplit('/').next().unwrap_or(path)))
        .max()
        .unwrap_or(0))
}

fn dir_has_task_id(tasks_dir: &Path, id: u32) -> Result<bool> {
    let entries =
        fs::read_dir(tasks_dir).with_context(|| format!("reading {}", tasks_dir.display()))?;
    Ok(entries
        .filter_map(Result::ok)
        .any(|entry| filename_task_id(&entry.file_name().to_string_lossy()) == Some(id)))
}

// ---- reservations ----

/// Where id claims live: the repo's *common* git dir, which all worktrees
/// share — a claim taken in one worktree is visible to creates in every
/// other. A non-git project falls back to a dot-directory inside its own
/// `backlog/` (invisible to the task loader, which only reads `*.md` from
/// the four task dirs).
fn reservation_dir(repo_root: &Path) -> PathBuf {
    git_common_dir(repo_root)
        .map(|common| common.join("switchbard"))
        .unwrap_or_else(|| repo_root.join("backlog/.id-reservations"))
}

fn git_common_dir(repo_root: &Path) -> Option<PathBuf> {
    let root = repo_root.to_str()?;
    let output = git_cmd()
        .args(["-C", root, "rev-parse", "--git-common-dir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let path = PathBuf::from(text.trim());
    let absolute = if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    };
    Some(absolute)
}

/// An exclusive claim on one task id, released (best-effort) on drop.
struct IdReservation {
    path: PathBuf,
}

impl Drop for IdReservation {
    fn drop(&mut self) {
        // Best-effort: a leftover file goes stale and is stolen in 60s.
        let _ = fs::remove_file(&self.path);
    }
}

/// Try to claim `id`. `None` means another live process holds it. A stale
/// or unreadable existing claim is stolen (see the module doc for why
/// unreadable counts as stale here).
fn try_reserve(dir: &Path, id: u32, now: u64) -> Result<Option<IdReservation>> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(format!("task-{id}.reserve"));
    for _attempt in 0..2 {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(now.to_string().as_bytes())
                    .with_context(|| format!("writing {}", path.display()))?;
                return Ok(Some(IdReservation { path }));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if !reservation_is_stale(&path, now) {
                    return Ok(None);
                }
                // Stale: remove and take the second (and last) attempt. If
                // another stealer wins the re-create race, this claim fails
                // and the caller moves to the next id.
                let _ = fs::remove_file(&path);
            }
            Err(err) => {
                return Err(err).with_context(|| format!("claiming {}", path.display()));
            }
        }
    }
    Ok(None)
}

fn reservation_is_stale(path: &Path, now: u64) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        // Vanished between the create_new failure and this read: the holder
        // released it. Report stale so the caller's second create_new
        // attempt settles who owns it now.
        return true;
    };
    match content.trim().parse::<u64>() {
        Ok(stamp) => now.saturating_sub(stamp) > RESERVATION_STALE_SECS,
        // Garbage content: no way to age it; refusing to steal would brick
        // the id forever, and the price of a wrong steal is only a claim.
        Err(_) => true,
    }
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_project(dir: &Path, files: &[(&str, &str)]) {
        for (rel_dir, name) in files {
            let d = dir.join("backlog").join(rel_dir);
            fs::create_dir_all(&d).expect("fixture dirs");
            fs::write(d.join(name), "---\nid: TASK-0\ntitle: fixture\n---\n")
                .expect("fixture file");
        }
    }

    fn new_task(title: &str) -> NewBacklogTask {
        NewBacklogTask {
            title: title.to_string(),
            description: String::new(),
            status: String::new(),
            priority: String::new(),
            acceptance_criteria: vec![],
            parent: None,
            labels: vec![],
            assignees: vec![],
            milestone: None,
            dependencies: vec![],
        }
    }

    /// `git` in a test fixture, with identity flags so commits work on CI.
    fn git(dir: &Path, args: &[&str]) {
        let status = git_cmd()
            .args([
                "-C",
                dir.to_str().expect("utf-8 tempdir"),
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.com",
            ])
            .args(args)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed in {}", dir.display());
    }

    #[test]
    fn filename_task_id_reads_plain_and_subtask_ids_and_rejects_noise() {
        assert_eq!(filename_task_id("task-42 - Title.md"), Some(42));
        assert_eq!(filename_task_id("task-150.10 - Sub.md"), Some(150));
        assert_eq!(filename_task_id("TASK-7 - Caps.md"), Some(7));
        assert_eq!(filename_task_id("notes.md"), None);
        assert_eq!(filename_task_id("task- - broken.md"), None);
    }

    #[test]
    fn parse_active_branches_filters_by_age_and_caps_the_count() {
        let now = 100 * 24 * 60 * 60;
        let fresh = now - 24 * 60 * 60;
        let stale = now - 40 * 24 * 60 * 60;
        let text = format!("keep\t{fresh}\ndrop\t{stale}\nmalformed line\n");

        assert_eq!(parse_active_branches(&text, now, 30), vec!["keep"]);

        let many: String = (0..300).map(|i| format!("b{i}\t{fresh}\n")).collect();
        assert_eq!(
            parse_active_branches(&many, now, 30).len(),
            MAX_BRANCHES_SCANNED
        );
    }

    #[test]
    fn next_id_over_a_plain_project_spans_all_four_dirs() {
        let dir = tempfile::tempdir().expect("tempdir");
        plain_project(
            dir.path(),
            &[
                ("tasks", "task-3 - A.md"),
                ("completed", "task-11 - B.md"),
                ("drafts", "task-5 - C.md"),
                ("archive/tasks", "task-8 - D.md"),
            ],
        );

        assert_eq!(next_task_id(dir.path()).expect("allocates"), 12);
    }

    #[test]
    fn next_id_for_an_empty_project_is_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(next_task_id(dir.path()).expect("allocates"), 1);
    }

    #[test]
    fn a_task_committed_on_an_unmerged_branch_still_blocks_its_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-b", "main"]);
        plain_project(root, &[("tasks", "task-2 - Main.md")]);
        git(root, &["add", "."]);
        git(root, &["commit", "-m", "main task"]);
        git(root, &["checkout", "-b", "feat"]);
        plain_project(root, &[("tasks", "task-9 - Branch.md")]);
        git(root, &["add", "."]);
        git(root, &["commit", "-m", "branch task"]);
        git(root, &["checkout", "main"]);
        assert!(
            !root.join("backlog/tasks/task-9 - Branch.md").exists(),
            "the branch task must not be in the main worktree"
        );

        assert_eq!(next_task_id(root).expect("allocates"), 10);
    }

    #[test]
    fn an_uncommitted_task_in_a_sibling_worktree_still_blocks_its_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("main");
        fs::create_dir_all(&root).expect("fixture dirs");
        git(&root, &["init", "-b", "main"]);
        plain_project(&root, &[("tasks", "task-2 - Main.md")]);
        git(&root, &["add", "."]);
        git(&root, &["commit", "-m", "main task"]);
        let sibling = dir.path().join("wt");
        git(
            &root,
            &[
                "worktree",
                "add",
                sibling.to_str().expect("utf-8"),
                "-b",
                "side",
            ],
        );
        plain_project(&sibling, &[("tasks", "task-12 - Uncommitted.md")]);

        assert_eq!(next_task_id(&root).expect("allocates"), 13);
        assert_eq!(
            next_task_id(&sibling).expect("allocates"),
            13,
            "allocation agrees from either worktree"
        );
    }

    #[test]
    fn a_reservation_excludes_a_second_claimant_until_stale() {
        let dir = tempfile::tempdir().expect("tempdir");
        let now = 1_000_000;

        let claim = try_reserve(dir.path(), 42, now).expect("io ok");
        assert!(claim.is_some(), "first claim wins");
        let rival = try_reserve(dir.path(), 42, now).expect("io ok");
        assert!(rival.is_none(), "live claim excludes a rival");

        drop(claim);
        let after_release = try_reserve(dir.path(), 42, now).expect("io ok");
        assert!(after_release.is_some(), "released claim is claimable again");

        drop(after_release);
        fs::write(dir.path().join("task-42.reserve"), "999").expect("stale fixture");
        let stolen = try_reserve(dir.path(), 42, now).expect("io ok");
        assert!(stolen.is_some(), "a stale claim is stolen");
    }

    #[test]
    fn create_skips_an_id_reserved_by_someone_else() {
        let dir = tempfile::tempdir().expect("tempdir");
        plain_project(dir.path(), &[("tasks", "task-4 - Existing.md")]);
        let reservations = reservation_dir(dir.path());
        fs::create_dir_all(&reservations).expect("fixture dirs");
        fs::write(
            reservations.join("task-5.reserve"),
            unix_now_secs().to_string(),
        )
        .expect("rival claim");

        let (id, path) =
            create_task_allocating_id(dir.path(), &new_task("Skips over")).expect("creates");

        assert_eq!(id, 6, "id 5 is held by the rival, so 6 is minted");
        assert!(path.exists());
    }

    #[test]
    fn concurrent_creates_mint_distinct_ids_and_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        plain_project(dir.path(), &[("tasks", "task-1 - Seed.md")]);
        let root = dir.path().to_path_buf();

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let root = root.clone();
                std::thread::spawn(move || {
                    create_task_allocating_id(&root, &new_task(&format!("Racer {i}")))
                        .expect("create succeeds")
                })
            })
            .collect();
        let mut ids: Vec<u32> = handles
            .into_iter()
            .map(|h| h.join().expect("thread joins").0)
            .collect();
        ids.sort_unstable();

        let mut deduped = ids.clone();
        deduped.dedup();
        assert_eq!(ids, deduped, "no two racers may share an id: {ids:?}");
        let on_disk = fs::read_dir(root.join("backlog/tasks"))
            .expect("tasks dir reads")
            .count();
        assert_eq!(on_disk, 5, "seed + four racers, nothing overwritten");
    }

    /// The reservation dir for a git repo lives in the *common* dir, so a
    /// claim taken in one worktree excludes a claimant in another.
    #[test]
    fn reservations_are_shared_across_worktrees_of_one_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("main");
        fs::create_dir_all(&root).expect("fixture dirs");
        git(&root, &["init", "-b", "main"]);
        fs::write(root.join("seed.txt"), "x").expect("seed");
        git(&root, &["add", "."]);
        git(&root, &["commit", "-m", "seed"]);
        let sibling = dir.path().join("wt");
        git(
            &root,
            &[
                "worktree",
                "add",
                sibling.to_str().expect("utf-8"),
                "-b",
                "side",
            ],
        );

        let claim = try_reserve(&reservation_dir(&root), 9, unix_now_secs()).expect("io ok");
        assert!(claim.is_some());
        let rival = try_reserve(&reservation_dir(&sibling), 9, unix_now_secs()).expect("io ok");
        assert!(
            rival.is_none(),
            "a claim from the main worktree must be visible from the sibling"
        );
    }
}
