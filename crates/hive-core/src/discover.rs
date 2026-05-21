//! Auto-discover git repositories under the user's home directory.
//!
//! Used by the GUI's first-launch onboarding flow to populate the
//! "Tracked repos" picker without forcing the user to navigate the file
//! tree.
//!
//! ### Detection model
//! Rather than hardcode well-known folder names (`Dev`, `code`, `src`, …),
//! we identify dev-like folders by their *content*: any directory under
//! `~/` (or `~/Documents/`, since some devs nest there) that is itself a
//! git repo, or that contains at least one direct-child git repo, is
//! treated as a scan root. The hardcoded-name approach worked for the
//! ~50% of macOS devs who use one of the conventional names; this works
//! for everyone whose code lives anywhere not deeply buried.
//!
//! Skipped at the home level: macOS special folders (`Library`, `Music`,
//! `Pictures`, `Movies`, `Public`, `Downloads`, `Desktop`,
//! `Applications`) and dotted directories. `Documents` is skipped from
//! the main pass and processed separately so we don't bring along
//! receipt PDFs and saved screenshots.
//!
//! ### What counts as a repo
//! A directory whose entry `.git` is itself a *directory* (not a file).
//! A `.git` file means the directory is a worktree of another repo, in
//! which case `enumerate_worktrees` will surface it via its parent.
//!
//! ### Scan depth
//! Each chosen scan root is walked to depth 2 — `<root>/foo`,
//! `<root>/sub/bar`. Depth 3+ rarely contains direct repos and the walk
//! cost is real on slow disks / network mounts.
//!
//! ### Ordering
//! Returns repos sorted by most-recently-modified first, so the GUI's
//! "auto-select recent" heuristic picks the right ones.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// macOS home-level folders that never contain user code. Skipped during
/// auto-discovery so we don't waste readdirs on receipts and photos.
const HOME_SPECIAL_FOLDERS: &[&str] = &[
    "Library",
    "Music",
    "Pictures",
    "Movies",
    "Public",
    "Downloads",
    "Desktop",
    "Applications",
    "Documents", // handled separately
];

/// When sniffing a directory for "looks like a dev folder", we read at
/// most this many entries before giving up. A folder with hundreds of
/// non-repo children isn't worth a longer probe.
const DEV_LIKE_PROBE_BUDGET: usize = 100;

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

/// Auto-detect scan roots under `$HOME`. Looks at depth-1 children of
/// `~/` and of `~/Documents/`, returning any that either *are* a git
/// repo or *contain* at least one direct-child git repo. macOS special
/// folders and dotted directories are skipped at the home level.
///
/// Canonicalization-based dedup collapses APFS case-equivalent spellings
/// and symlinks across roots.
pub fn auto_scan_roots(home: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    add_dev_folders_under(home, /*at_home_level=*/ true, &mut out, &mut seen);

    // Documents is a macOS special folder so we skipped it above, but
    // some devs use ~/Documents/code or ~/Documents/Projects. Recurse
    // one level explicitly.
    let docs = home.join("Documents");
    if docs.is_dir() {
        add_dev_folders_under(&docs, /*at_home_level=*/ false, &mut out, &mut seen);
    }

    out
}

/// Returns true iff `dir` is itself a repo OR contains at least one
/// direct-child repo. Bounded by `DEV_LIKE_PROBE_BUDGET` so a folder
/// with hundreds of non-repo children doesn't slow us down.
fn dir_is_or_contains_repo(dir: &Path) -> bool {
    if dir.join(".git").is_dir() {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for (i, entry) in entries.flatten().enumerate() {
        if i >= DEV_LIKE_PROBE_BUDGET {
            return false;
        }
        let p = entry.path();
        if p.is_dir() && p.join(".git").is_dir() {
            return true;
        }
    }
    false
}

fn add_dev_folders_under(
    parent: &Path,
    at_home_level: bool,
    out: &mut Vec<PathBuf>,
    seen: &mut std::collections::HashSet<PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if at_home_level && HOME_SPECIAL_FOLDERS.contains(&name) {
            continue;
        }
        if !dir_is_or_contains_repo(&path) {
            continue;
        }
        let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.insert(key) {
            out.push(path);
        }
    }
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
    found.sort_by_key(|r| std::cmp::Reverse(r.modified));
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
    fn auto_scan_roots_returns_empty_for_empty_home() {
        // A pristine home dir with no folders should produce no roots —
        // not a panic, not a "Library/" entry, not a fallback list.
        let tmp = tempfile::tempdir().unwrap();
        let roots = auto_scan_roots(tmp.path());
        assert!(roots.is_empty(), "got {roots:?}");
        // discover_repos on a nonexistent path also no-ops cleanly.
        let nonexistent = tmp.path().join("Nope");
        let found = discover_repos(&[nonexistent]);
        assert!(found.is_empty());
    }

    #[test]
    fn discover_dedupes_repos_reached_via_symlinked_roots() {
        // Two roots pointing at the same inode via a symlink should
        // surface each repo exactly once. Mirrors the macOS APFS case-
        // insensitive collation behavior in a portable way.
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
    fn auto_scan_roots_finds_any_named_dev_folder() {
        // The whole point of content-based detection: a folder named
        // anything at all is included if it contains repos. We test
        // with a name that isn't in the old hardcoded list (`brainery`)
        // to prove name-agnosticism.
        let tmp = tempfile::tempdir().unwrap();
        let weird = tmp.path().join("brainery");
        make_repo(&weird.join("project-1"));
        let roots = auto_scan_roots(tmp.path());
        let names: Vec<_> = roots
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"brainery"), "got {roots:?}");
    }

    #[test]
    fn auto_scan_roots_finds_repo_directly_at_home() {
        // A direct child of home that IS a repo (e.g. `~/dotfiles`)
        // should be returned as a scan root, so discover_repos picks
        // it up via repo_at().
        let tmp = tempfile::tempdir().unwrap();
        make_repo(&tmp.path().join("dotfiles"));
        let roots = auto_scan_roots(tmp.path());
        let names: Vec<_> = roots
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert_eq!(names, vec!["dotfiles"]);
    }

    #[test]
    fn auto_scan_roots_skips_folders_with_no_repos() {
        // A non-dev folder under home (e.g. a notes archive) should be
        // ignored even if it has subdirectories.
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("notes").join("2024")).unwrap();
        fs::create_dir_all(tmp.path().join("notes").join("2025")).unwrap();
        make_repo(&tmp.path().join("actual-code").join("project"));
        let roots = auto_scan_roots(tmp.path());
        let names: Vec<_> = roots
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert_eq!(names, vec!["actual-code"], "got {roots:?}");
    }

    #[test]
    fn auto_scan_roots_skips_macos_special_folders() {
        // Even if Library happened to contain a `.git`-shaped subdir
        // (e.g. some dependency's vendored repo), we never include
        // Library as a scan root.
        let tmp = tempfile::tempdir().unwrap();
        make_repo(&tmp.path().join("Library").join("evil-vendor-repo"));
        make_repo(&tmp.path().join("Music").join("hidden-repo"));
        make_repo(&tmp.path().join("realdev").join("project"));
        let roots = auto_scan_roots(tmp.path());
        let names: Vec<_> = roots
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert_eq!(names, vec!["realdev"]);
    }

    #[test]
    fn auto_scan_roots_recurses_into_documents() {
        // ~/Documents is a macOS special folder but some devs use
        // ~/Documents/code. We process Documents children explicitly.
        let tmp = tempfile::tempdir().unwrap();
        make_repo(&tmp.path().join("Documents").join("code").join("project"));
        let roots = auto_scan_roots(tmp.path());
        let names: Vec<_> = roots
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert_eq!(names, vec!["code"]);
    }

    #[test]
    fn auto_scan_roots_skips_dotted_dirs_at_home() {
        // `~/.config`, `~/.cache`, `~/.cargo` can technically contain
        // git checkouts (rustup, cargo registry index, etc.). Never
        // include them.
        let tmp = tempfile::tempdir().unwrap();
        make_repo(&tmp.path().join(".cargo").join("registry-mirror"));
        make_repo(&tmp.path().join("real").join("project"));
        let roots = auto_scan_roots(tmp.path());
        let names: Vec<_> = roots
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert_eq!(names, vec!["real"]);
    }

    #[test]
    fn auto_scan_roots_dedupes_canonicalized() {
        // Same content reached via two names (via symlink) should
        // collapse to one entry.
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("RealDev");
        make_repo(&real.join("project"));
        std::os::unix::fs::symlink(&real, tmp.path().join("aliased")).unwrap();
        let roots = auto_scan_roots(tmp.path());
        assert_eq!(roots.len(), 1, "got {roots:?}");
    }
}
