//! Stamps the git identity of the tree being compiled into the binary.
//!
//! Threat this exists for (TASK-172, and the recurrence that produced this
//! file): every Switchbard binary is installed with `cargo install --path`
//! from *some* worktree, and `sbt` re-execs itself whenever the file on disk
//! changes. An install from a worktree that predates a feature therefore
//! silently removes that feature from every running session, and the binary
//! reports the same workspace version either way, so nothing on screen or in
//! the event log says which tree it came from. The commit stamped here is what
//! makes that answerable — and it is what `scripts/install-switchbard.sh`
//! compares against to refuse a downgrade.
//!
//! Every probe degrades to `unknown` rather than failing the build: a build
//! from a source tarball, a vendored copy, or a checkout without `git` on PATH
//! must still compile.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let commit = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"])
        .filter(|b| b != "HEAD")
        .unwrap_or_else(|| "detached".to_string());
    // `--quiet` exits non-zero when the tree differs from HEAD; treat an
    // unavailable answer as dirty, because claiming clean is the lie that
    // costs someone their afternoon.
    let dirty = match Command::new("git")
        .args(["diff", "--quiet", "HEAD"])
        .status()
    {
        Ok(status) => !status.success(),
        Err(_) => commit != "unknown",
    };

    // Composed here rather than at runtime because clap's `version` wants a
    // `&'static str`, and a const is also what keeps `--version` free of any
    // formatting the event log does not share.
    let short = commit.get(..8).unwrap_or(&commit);
    let suffix = if dirty { " dirty" } else { "" };
    let version_line = format!(
        "{} ({short} {branch}{suffix})",
        std::env::var("CARGO_PKG_VERSION").unwrap_or_default()
    );

    println!("cargo:rustc-env=SWITCHBARD_BUILD_VERSION_LINE={version_line}");
    println!("cargo:rustc-env=SWITCHBARD_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=SWITCHBARD_BUILD_BRANCH={branch}");
    println!("cargo:rustc-env=SWITCHBARD_BUILD_DIRTY={dirty}");

    // Rebuild the stamp when the checked-out commit moves. `--git-path`
    // resolves both a plain `.git` directory and a worktree's `.git` file.
    for path in ["HEAD", "logs/HEAD"] {
        if let Some(resolved) = git(&["rev-parse", "--git-path", path]) {
            let resolved = PathBuf::from(resolved);
            if resolved.exists() {
                println!("cargo:rerun-if-changed={}", resolved.display());
            }
        }
    }
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}
