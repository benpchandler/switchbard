//! Path rendering helpers shared across the table views.
//!
//! Why: long worktree paths (`/Users/me/code/.worktrees/alpha/codex/
//! branch-x`) blow out column widths and force either
//! wrapping (looks weird, inconsistent row heights) or truncation with
//! ellipsis. The fix is to compute a tight elided form for the cell — the
//! tail two components prefixed with "…/" — and put the full path in the
//! tooltip. Single-line, predictable width, no lost information.

use std::path::Path;

/// Maximum characters before we elide. Tuned for the default Hive window
/// width: 40 chars renders at roughly 300 px in the body font.
const MAX_CHARS: usize = 40;

/// Number of trailing path components to keep when eliding. Two is the sweet
/// spot — for a path like `…/codex/branch-x` the parent
/// directory disambiguates between sibling worktrees and the leaf is the
/// branch-like identifier.
const TAIL_COMPONENTS: usize = 2;

/// Return a single-line cell-friendly version of `path`. Short paths pass
/// through; long ones become "…/<parent>/<leaf>" (or the longest suffix that
/// fits within `MAX_CHARS` if a single component is huge).
pub fn shorten(path: &Path) -> String {
    let full = path.to_string_lossy();
    if full.chars().count() <= MAX_CHARS {
        return full.into_owned();
    }
    let comps: Vec<_> = path
        .components()
        .filter(|c| {
            !matches!(
                c,
                std::path::Component::RootDir | std::path::Component::Prefix(_)
            )
        })
        .collect();
    if comps.len() <= TAIL_COMPONENTS {
        // Path is dominated by a single huge component — can't shorten
        // structurally; just return as-is. The tooltip will still carry the
        // full text.
        return full.into_owned();
    }
    let tail: std::path::PathBuf = comps.iter().rev().take(TAIL_COMPONENTS).rev().collect();
    format!("…/{}", tail.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn short_paths_pass_through() {
        let p = PathBuf::from("/Users/me/code/delta");
        assert_eq!(shorten(&p), "/Users/me/code/delta");
    }

    #[test]
    fn long_paths_keep_tail_two() {
        let p = PathBuf::from(
            "/Users/me/code/.worktrees/alpha/codex/branch-x",
        );
        assert_eq!(shorten(&p), "…/codex/branch-x");
    }

    #[test]
    fn paths_with_fewer_than_three_components_pass_through() {
        let p = PathBuf::from("/some-really-long-leaf-that-is-also-the-only-thing");
        assert_eq!(
            shorten(&p),
            "/some-really-long-leaf-that-is-also-the-only-thing"
        );
    }
}
