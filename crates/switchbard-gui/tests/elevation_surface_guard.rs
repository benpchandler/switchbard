//! TASK-79: guards the "one elevation authority" invariant `theme.rs`
//! documents (`theme::elevation`/`theme::frame`) — no other file under
//! `src/ui/` may mint its own literal surface color. A hand-picked
//! `Color32::from_rgb(...)` / `Color32::from_rgba_*(...)` /
//! `Color32::from_black_alpha(...)` / `Color32::from_gray(...)` outside
//! `theme.rs` is exactly the "two sources for one fact" shape TASK-79 found
//! and fixed repeatedly (`ui.visuals().faint_bg_color` instead of
//! `theme::faint_bg()`, an inline black-alpha modal scrim instead of
//! `theme::modal_scrim()`, `ui.visuals().widgets.noninteractive.bg_stroke`
//! instead of `theme::surface_stroke()`). `legibility.rs` is the one
//! deliberate exception: its `Color32::from_rgb` is a color-math helper
//! (`composite_over`, alpha-compositing two already-themed colors for a
//! contrast measurement), not a painted surface.
//!
//! Deliberately textual, not a syntax-aware lint: greppable and
//! false-positive-free today beats exhaustive coverage of every way a
//! `Color32` could theoretically be built. If a legitimate new exception
//! appears, name it in `EXEMPT_FILES` with a doc comment explaining why,
//! rather than loosening the forbidden-pattern list.

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN_PATTERNS: &[&str] = &[
    "Color32::from_rgb",
    "Color32::from_rgba",
    "Color32::from_black_alpha",
    "Color32::from_white_alpha",
    "Color32::from_gray",
];

/// Files under `src/ui/` allowed to construct a literal `Color32` — each
/// entry needs a reason, not just a name.
const EXEMPT_FILES: &[&str] = &[
    // The single semantic-color authority every accessor in this test's
    // forbidden list ultimately funnels through.
    "theme.rs",
    // Color math (WCAG contrast, alpha compositing), not a painted surface —
    // see `composite_over`'s own doc.
    "legibility.rs",
];

/// Directory recursion here is a fixed, small, version-controlled source
/// tree (not untrusted/external input), but still capped per this repo's
/// Power-of-10 bounded-recursion convention — a stack, not a recursive
/// function, with a hard ceiling on how many files it will ever visit.
const MAX_FILES_VISITED: usize = 2_000;

#[test]
fn no_ui_surface_outside_theme_mints_its_own_literal_color() {
    let ui_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
    assert!(ui_dir.is_dir(), "expected {} to exist", ui_dir.display());

    let mut violations = Vec::new();
    let mut stack: Vec<PathBuf> = vec![ui_dir];
    let mut visited = 0usize;

    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            assert!(
                visited <= MAX_FILES_VISITED,
                "src/ui/ grew past this guard's {MAX_FILES_VISITED}-file bound; raise the \
                 constant deliberately rather than let the walk run unbounded"
            );
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            check_file(&path, &mut violations);
        }
    }

    assert!(
        violations.is_empty(),
        "literal Color32 construction outside theme.rs — route through a theme:: accessor \
         (e.g. `theme::card_bg()`) or `theme::frame(Elevation::_)` instead:\n{}",
        violations.join("\n")
    );
}

fn check_file(path: &Path, violations: &mut Vec<String>) {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    if !name.ends_with(".rs") || EXEMPT_FILES.contains(&name) {
        return;
    }
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };
    for (line_no, line) in contents.lines().enumerate() {
        if FORBIDDEN_PATTERNS.iter().any(|pat| line.contains(pat)) {
            violations.push(format!(
                "{}:{}: {}",
                path.display(),
                line_no + 1,
                line.trim()
            ));
        }
    }
}
