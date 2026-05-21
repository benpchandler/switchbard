//! Auto-discover git repositories under common developer directories.
//!
//! Used by the GUI's first-launch onboarding flow to populate the
//! "Tracked repos" picker without forcing the user to navigate the file
//! tree. The scan is shallow on purpose: walking deep would be slow, hit
//! `node_modules`, and surface dependency repos the user didn't mean to
//! track.
//!
//! ### What counts as a repo
//! A directory whose entry `.git` is itself a *directory* (not a file).
//! A `.git` file means the directory is a worktree of another repo, in
//! which case `enumerate_worktrees` will surface it via its parent.
//!
//! ### Scan depth
//! Each search root is walked to depth 2 — `~/Dev/foo`, `~/Dev/work/bar`.
//! Depth 3+ rarely contains direct repos (it's nested workspace files,
//! dependencies, etc.) and the walk cost is real on slow disks.
//!
//! ### Ordering
//! Returns repos sorted by most-recently-modified first, so the GUI's
//! "auto-select recent" heuristic picks the right ones.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Roots we'll auto-scan if they exist. macOS conventions plus the
/// lowercase/uppercase variants people actually use.
pub const DEFAULT_SCAN_ROOT_NAMES: &[&str] = &[
    "Dev",
    "dev",
    "Code",
    "code",
    "Source",
    "src",
    "Projects",
    "projects",
    "repos",
    "Repos",
    "workspace",
    "Workspace",
    "work",
    "Work",
];

const MAX_DEPTH: usize = 2;
const MAX_CANDIDATES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredRepo {
    pub path: PathBuf,
    pub name: String,
    /// Most recent of: repo mtime, `.git/` mtime, `HEAD`-ish proxy. Used
    /// to sort and to default-select recent repos in the picker.
    pub modified: SystemTime,
}

/// Resolve scan roots under `$HOME`. Skips any name that doesn't exist
/// and collapses paths that resolve to the same inode — APFS is
/// case-insensitive by default, so on a typical Mac `~/Dev` and `~/dev`
/// are the same directory. Without canonicalization the walker would
/// visit the same tree once per spelling and surface every repo twice.
pub fn default_scan_roots(home: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for name in DEFAULT_SCAN_ROOT_NAMES {
        let candidate = home.join(name);
        if !candidate.is_dir() {
            continue;
        }
        let key = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
        if seen.insert(key) {
            out.push(candidate);
        }
    }
    out
}

/// Walk each root and return discovered repos, sorted newest-modified first.
///
/// The walk is depth-limited and bounded by `MAX_CANDIDATES` so a
/// pathological dev directory (someone with 500 hobby repos) can't
/// block the GUI's first paint.
pub fn discover_repos(roots: &[PathBuf]) -> Vec<DiscoveredRepo> {
    let mut found: Vec<DiscoveredRepo> = Vec::new();
    for root in roots {
        if found.len() >= MAX_CANDIDATES {
            break;
        }
        walk(root, 0, &mut found);
    }
    found.sort_by(|a, b| b.modified.cmp(&a.modified));
    // Dedup by *canonical* path. Without canonicalize, case-insensitive
    // APFS filesystems and symlinked roots both surface the same repo
    // under multiple spellings — and dedup'ing on the raw PathBuf
    // string misses them. Falling back to the raw path on canonicalize
    // failure keeps test fixtures (tempdirs with `.`s) working.
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    found.retain(|r| {
        let key = std::fs::canonicalize(&r.path).unwrap_or_else(|_| r.path.clone());
        seen.insert(key)
    });
    found
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<DiscoveredRepo>) {
    if out.len() >= MAX_CANDIDATES {
        return;
    }
    if let Some(repo) = repo_at(dir) {
        out.push(repo);
        // Don't recurse into a repo — we don't want `foo/vendored-submodule`
        // to show up as its own row.
        return;
    }
    if depth >= MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if should_skip_dir(&path) {
            continue;
        }
        walk(&path, depth + 1, out);
    }
}

/// Recognize a "real" repo root (not a worktree of one). A repo root
/// has `.git/` as a *directory*. A worktree of a repo has `.git` as a
/// *file* (pointing at `gitdir: …`).
fn repo_at(dir: &Path) -> Option<DiscoveredRepo> {
    let git_path = dir.join(".git");
    if !git_path.is_dir() {
        return None;
    }
    let name = dir.file_name()?.to_str()?.to_string();
    // Recency: take the latest of (git/, repo dir, git/HEAD if readable).
    let modified = latest_mtime(&[
        git_path.as_path(),
        dir,
        &git_path.join("HEAD"),
        &git_path.join("FETCH_HEAD"),
    ]);
    Some(DiscoveredRepo {
        path: dir.to_path_buf(),
        name,
        modified,
    })
}

fn latest_mtime(paths: &[&Path]) -> SystemTime {
    paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .filter_map(|m| m.modified().ok())
        .max()
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Directory names we never recurse into during discovery. These are
/// either chaff (caches, deps) or visually similar to repo roots but
/// aren't ones the user wants to "track" in Hive.
fn should_skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return true;
    };
    if name.starts_with('.') {
        // .archived, .Trash, .Trashes, .cache, etc. We never recurse
        // into dotted directories in discovery — but `.git` is handled
        // upstream by `repo_at` and never reached here.
        return true;
    }
    matches!(
        name,
        "node_modules"
            | "target"
            | "venv"
            | ".venv"
            | "__pycache__"
            | "dist"
            | "build"
            | "out"
            | "Pods"
            | "DerivedData"
    )
}

#[cfg(test)]
mod tests {
    //! Real-fs tests using `tempfile`. We create a synthetic dev tree and
    //! assert what comes back.

    use super::*;
    use std::fs;

    fn make_repo(dir: &Path) {
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    }

    fn make_worktree_of(dir: &Path) {
        // A real git worktree has a `.git` FILE, not a directory.
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(".git"), "gitdir: /elsewhere/.git/worktrees/foo\n").unwrap();
    }

    #[test]
    fn finds_repo_at_depth_1() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = tmp.path().join("Dev");
        make_repo(&dev.join("alpha"));
        make_repo(&dev.join("beta"));
        let found = discover_repos(&[dev]);
        let names: Vec<_> = found.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn finds_repo_at_depth_2() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = tmp.path().join("Dev");
        make_repo(&dev.join("work").join("gamma"));
        let found = discover_repos(&[dev]);
        let names: Vec<_> = found.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["gamma"]);
    }

    #[test]
    fn does_not_recurse_into_a_repo() {
        // A real repo with a vendored sub-repo inside it. We must only
        // surface the outer one — the inner is part of that repo.
        let tmp = tempfile::tempdir().unwrap();
        let dev = tmp.path().join("Dev");
        let outer = dev.join("outer");
        make_repo(&outer);
        make_repo(&outer.join("vendored"));
        let found = discover_repos(&[dev]);
        let names: Vec<_> = found.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["outer"]);
    }

    #[test]
    fn worktree_of_another_repo_is_not_listed() {
        // A worktree dir has `.git` as a *file*; it must not be picked up
        // as its own repo by discovery — the parent repo will surface it.
        let tmp = tempfile::tempdir().unwrap();
        let dev = tmp.path().join("Dev");
        make_repo(&dev.join("parent"));
        make_worktree_of(&dev.join("parent").join(".worktrees").join("feat"));
        let found = discover_repos(&[dev]);
        let names: Vec<_> = found.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["parent"]);
    }

    #[test]
    fn skips_node_modules() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = tmp.path().join("Dev");
        // node_modules at depth 1 isn't a repo, but if a transitive
        // dependency vendored a `.git` it might look like one. Verify
        // we never recurse into node_modules at all.
        make_repo(&dev.join("node_modules").join("evil-pkg"));
        make_repo(&dev.join("real-repo"));
        let found = discover_repos(&[dev]);
        let names: Vec<_> = found.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["real-repo"]);
    }

    #[test]
    fn skips_dotted_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = tmp.path().join("Dev");
        make_repo(&dev.join(".archived").join("old-repo"));
        make_repo(&dev.join("alive"));
        let found = discover_repos(&[dev]);
        let names: Vec<_> = found.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["alive"]);
    }

    #[test]
    fn sorts_by_modified_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = tmp.path().join("Dev");
        make_repo(&dev.join("older"));
        // Sleep briefly so mtimes differ. On macOS HFS+ mtime resolution
        // is 1s, but APFS is sub-second; 50ms is enough on either.
        std::thread::sleep(std::time::Duration::from_millis(50));
        make_repo(&dev.join("newer"));
        let found = discover_repos(&[dev]);
        let names: Vec<_> = found.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["newer", "older"]);
    }

    #[test]
    fn missing_root_is_silently_dropped_by_default_scan_roots() {
        // We don't pass missing dirs into `discover_repos`; the caller
        // filters first via `default_scan_roots`.
        let tmp = tempfile::tempdir().unwrap();
        let nonexistent = tmp.path().join("Nope");
        let roots = default_scan_roots(tmp.path());
        // The temp dir has no Dev / code / etc., so roots is empty.
        assert!(roots.is_empty());
        // And passing the nonexistent dir directly returns nothing,
        // doesn't panic.
        let found = discover_repos(&[nonexistent]);
        assert!(found.is_empty());
    }

    #[test]
    fn discover_dedupes_repos_reached_via_symlinked_roots() {
        // The original bug: macOS APFS is case-insensitive by default,
        // so `~/Dev` and `~/dev` are the same inode. Walking both
        // surfaces the same repo twice because the path strings differ.
        // We can't reliably create `Dev` and `dev` on the test
        // machine's actual filesystem (might be case-insensitive,
        // making the second mkdir EEXIST), so we simulate the
        // equivalent: two differently-named roots that resolve to the
        // same inode via a symlink. Canonicalize must collapse them.
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("Dev");
        make_repo(&real.join("alpha"));
        let alias = tmp.path().join("aliased");
        std::os::unix::fs::symlink(&real, &alias).unwrap();

        let found = discover_repos(&[real, alias]);
        assert_eq!(
            found.len(),
            1,
            "expected dedup across symlinked root spellings, got {found:?}"
        );
        assert_eq!(found[0].name, "alpha");
    }

    #[test]
    fn default_scan_roots_dedupes_inodes_reached_via_two_names() {
        // On real macOS APFS the case-insensitive collation means both
        // "Dev" and "dev" in DEFAULT_SCAN_ROOT_NAMES match the same
        // directory and would each get added without inode-level dedup.
        // Portable test: create `Dev`, symlink one of the OTHER
        // recognized names (`work`) to it, and assert
        // default_scan_roots returns exactly one entry.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("Dev")).unwrap();
        std::os::unix::fs::symlink(
            tmp.path().join("Dev"),
            tmp.path().join("work"),
        )
        .unwrap();
        let roots = default_scan_roots(tmp.path());
        assert_eq!(
            roots.len(),
            1,
            "expected one canonical entry, got {roots:?}"
        );
    }

    #[test]
    fn default_scan_roots_picks_up_existing_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("Dev")).unwrap();
        fs::create_dir(tmp.path().join("projects")).unwrap();
        let roots = default_scan_roots(tmp.path());
        // Both directories are distinct inodes so they both should
        // appear. Don't pin the exact filename spelling — the function
        // returns the first matching name from DEFAULT_SCAN_ROOT_NAMES,
        // which on a case-insensitive volume can collate to either
        // case. Inode count is the load-bearing assertion.
        let canonical: std::collections::HashSet<PathBuf> = roots
            .iter()
            .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
            .collect();
        assert_eq!(canonical.len(), 2, "got {roots:?}");
    }
}
