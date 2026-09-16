//! Every layout is checked against real `git rev-parse` output, so the
//! filesystem resolver cannot drift from the identity Git itself reports.
use super::resolve;
use crate::git_env::git_cmd;
use std::path::{Path, PathBuf};

fn git(dir: &Path, args: &[&str]) {
    let status = git_cmd()
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .expect("invariant: git runs");
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

fn git_answer(dir: &Path) -> Option<PathBuf> {
    let output = git_cmd()
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .expect("invariant: git runs");
    output.status.success().then(|| {
        PathBuf::from(String::from_utf8(output.stdout).expect("utf-8").trim())
            .canonicalize()
            .expect("git reports an existing directory")
    })
}

fn commit(repo: &Path) {
    git(
        repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "init",
        ],
    );
}

fn assert_matches_git(path: &Path) {
    assert_eq!(resolve(path), git_answer(path), "{}", path.display());
}

#[test]
fn plain_checkout_and_nested_paths_match_git() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(repo.join("a/b")).unwrap();
    std::fs::write(repo.join("a/file.md"), "x").unwrap();
    git(&repo, &["init", "-q"]);
    for path in [
        repo.clone(),
        repo.join("a/b"),
        repo.join(".git"),
        repo.join(".git/refs"),
    ] {
        assert_matches_git(&path);
    }
    // `git -C` cannot enter a file; the resolver answers for its directory.
    assert_eq!(resolve(&repo.join("a/file.md")), resolve(&repo.join("a")));
    assert_eq!(
        resolve(&repo),
        Some(repo.join(".git").canonicalize().unwrap())
    );
}

#[test]
fn linked_worktree_resolves_to_the_main_repository() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    commit(&repo);
    let linked = temp.path().join("linked");
    git(&repo, &["worktree", "add", "-q", linked.to_str().unwrap()]);
    let gitdir = std::fs::read_to_string(linked.join(".git")).unwrap();
    let private = PathBuf::from(gitdir.trim().strip_prefix("gitdir: ").unwrap());
    for path in [linked.clone(), private] {
        assert_matches_git(&path);
    }
    assert_eq!(resolve(&linked), resolve(&repo));
}

#[test]
fn bare_repository_and_relative_gitfile_match_git() {
    let temp = tempfile::tempdir().unwrap();
    let bare = temp.path().join("bare.git");
    std::fs::create_dir(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare"]);
    assert_matches_git(&bare);
    let pointed = temp.path().join("pointed");
    std::fs::create_dir(&pointed).unwrap();
    std::fs::write(pointed.join(".git"), "gitdir: ../bare.git\n").unwrap();
    assert_matches_git(&pointed);
    assert_eq!(resolve(&pointed), Some(bare.canonicalize().unwrap()));
}

#[test]
fn non_repositories_and_broken_gitfiles_resolve_to_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let plain = temp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert_matches_git(&plain);
    assert_eq!(resolve(&plain), None);
    let broken = temp.path().join("broken");
    std::fs::create_dir(&broken).unwrap();
    std::fs::write(broken.join(".git"), "gitdir: missing\n").unwrap();
    assert_eq!(resolve(&broken), None);
    assert_eq!(git_answer(&broken), None);
    let fake = temp.path().join("fake");
    std::fs::create_dir_all(fake.join(".git")).unwrap();
    assert_matches_git(&fake);
    assert_eq!(resolve(&temp.path().join("absent")), None);
}
