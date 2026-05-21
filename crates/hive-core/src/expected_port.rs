//! Best-effort "what port will this command bind to?" extractor.
//!
//! Used by the Servers view to flag blockers — if `expected_port` returns Some
//! and the scanner sees a listener on that port from a different worktree, the
//! row gets a "BLOCKED · port held by …" tag before the user clicks Start.
//!
//! False negatives are fine (we just don't pre-warn). False positives would
//! cause spurious blocker tags, so the parser is conservative: it only
//! recognizes a small set of flag spellings that are unambiguously a port.

/// Returns the first port-looking integer found in a recognized position.
///
/// Recognized:
///   - `--port N` / `--port=N`
///   - `-port N` / `-port=N`   (Go-style single dash)
///   - `-p N` / `-p=N`         (short alias — Storybook, ng serve, next)
///   - `--bind ...:N` / `--bind=...:N`  (gunicorn)
///   - `PORT=N` (Procfile/env style)
///
/// Returns `None` if no match. Bare numbers in the command string are
/// intentionally ignored — too noisy (think `--workers 4`, `--max 8000`).
pub fn expected_port(cmd: &str) -> Option<u16> {
    let lc = cmd.to_lowercase();

    for flag in &["--port", "-port", "-p"] {
        if let Some(p) = scan_after_flag(&lc, flag) {
            return Some(p);
        }
    }

    // gunicorn / hypercorn `--bind 0.0.0.0:8000` or `--bind=:8000`
    if let Some(idx) = lc.find("--bind") {
        let after = &lc[idx + "--bind".len()..];
        let after = after.trim_start_matches([' ', '=']);
        // Find first colon, then scan digits after it.
        if let Some(colon_pos) = after.find(':') {
            let digits: String = after[colon_pos + 1..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(p) = digits.parse::<u16>() {
                if p > 0 {
                    return Some(p);
                }
            }
        }
    }

    // PORT=N (Procfile or env-prefix). Must be at a word boundary so we
    // don't match `EXPORT=...` or `--port=...` (already handled above).
    if let Some(p) = scan_after_keyword_at_word_boundary(&lc, "port=") {
        return Some(p);
    }

    None
}

/// Conventional default port for a service, looked up by its canonical name
/// (the `canonical_name` produced by `resolve::resolve`). Used as a last-tier
/// hint for the Open action when nothing on the command line declares a port
/// and no listener could be attributed back to the run.
///
/// Conservative: only well-known dev-tool defaults that the tool itself uses
/// when no `--port` flag is passed. We do NOT guess generic stacks ("python
/// http.server" → 8000) because the user's command might be doing anything.
pub fn default_port_for_service(canonical_name: &str) -> Option<u16> {
    match canonical_name.to_ascii_lowercase().as_str() {
        // JS frontends
        "storybook" => Some(6006),
        "vite" | "sveltekit" => Some(5173),
        "next" | "nuxt" | "react" | "cra" => Some(3000),
        "astro" => Some(4321),
        "gatsby" => Some(8000),
        // Static-site generators
        "jekyll" => Some(4000),
        "hugo" => Some(1313),
        "mkdocs" | "mkdocs-serve" => Some(8000),
        _ => None,
    }
}

fn scan_after_flag(haystack: &str, flag: &str) -> Option<u16> {
    let mut start = 0;
    while let Some(idx) = haystack[start..].find(flag) {
        let pos = start + idx;
        // Boundary BEFORE the flag must be start-of-string or whitespace, so
        // we don't match `-p` inside `--port` or `something-p` (no real flag
        // looks like that, but be safe given `-p` is so short).
        let before_ok = pos == 0 || matches!(haystack.as_bytes()[pos - 1], b' ' | b'\t' | b'\n');
        // Boundary AFTER the flag must be whitespace or '=' — else this is a
        // substring match like `--portable` or `-pidfile`.
        let after = &haystack[pos + flag.len()..];
        let after_ok = matches!(after.chars().next(), Some(' ') | Some('='));
        if before_ok && after_ok {
            let rest = after.trim_start_matches([' ', '=']);
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(p) = digits.parse::<u16>() {
                if p > 0 {
                    return Some(p);
                }
            }
        }
        start = pos + flag.len();
    }
    None
}

fn scan_after_keyword_at_word_boundary(haystack: &str, keyword: &str) -> Option<u16> {
    let mut start = 0;
    while let Some(idx) = haystack[start..].find(keyword) {
        let pos = start + idx;
        // Require a word boundary in front (start or whitespace/punct), so we
        // don't match `export=...` or things ending in `port=`.
        let before_ok = pos == 0 || {
            let b = haystack.as_bytes()[pos - 1];
            b == b' ' || b == b'\t' || b == b'\n'
        };
        if before_ok {
            let after = &haystack[pos + keyword.len()..];
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(p) = digits.parse::<u16>() {
                if p > 0 {
                    return Some(p);
                }
            }
        }
        start = pos + keyword.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uvicorn_explicit_port() {
        assert_eq!(
            expected_port("uv run uvicorn lyon.server:app --reload --port 8420"),
            Some(8420)
        );
    }

    #[test]
    fn long_port_with_equals() {
        assert_eq!(expected_port("vite --port=5173"), Some(5173));
    }

    #[test]
    fn go_style_single_dash_port() {
        assert_eq!(expected_port("./lyon-bundle -port 8421"), Some(8421));
    }

    #[test]
    fn gunicorn_bind() {
        assert_eq!(
            expected_port("gunicorn -w 4 --bind 0.0.0.0:8000 app:app"),
            Some(8000)
        );
        assert_eq!(expected_port("gunicorn --bind=:8000 app:app"), Some(8000));
    }

    #[test]
    fn procfile_env_style() {
        assert_eq!(expected_port("PORT=8000 uvicorn app:app"), Some(8000));
    }

    #[test]
    fn no_port_returns_none() {
        assert_eq!(expected_port("bun run dev"), None);
        assert_eq!(expected_port("./scripts/start_lyon.sh"), None);
        assert_eq!(expected_port("make dev"), None);
    }

    #[test]
    fn ignores_bare_numbers() {
        // `--workers 4` shouldn't yield a port. Neither should `--max 8000`.
        assert_eq!(expected_port("celery --concurrency 4 worker"), None);
        assert_eq!(expected_port("ruff check --max-line-length 100"), None);
    }

    #[test]
    fn ignores_portable_substring() {
        // `--portable` shouldn't match `--port`.
        assert_eq!(expected_port("./mybin --portable mode"), None);
    }

    #[test]
    fn word_boundary_on_keyword() {
        // `export=foo PORT=8000` should still parse the PORT=.
        assert_eq!(
            expected_port("export=foo PORT=8000 uvicorn app:app"),
            Some(8000)
        );
        // Bare `xport=8000` (no word boundary) should not.
        assert_eq!(expected_port("xport=8000"), None);
    }

    #[test]
    fn short_p_flag_for_storybook() {
        assert_eq!(expected_port("storybook dev -p 6006"), Some(6006));
        assert_eq!(expected_port("storybook dev -p=6006"), Some(6006));
    }

    #[test]
    fn default_port_for_known_dev_tools() {
        assert_eq!(default_port_for_service("storybook"), Some(6006));
        assert_eq!(default_port_for_service("Storybook"), Some(6006));
        assert_eq!(default_port_for_service("vite"), Some(5173));
        assert_eq!(default_port_for_service("next"), Some(3000));
        assert_eq!(default_port_for_service("cra"), Some(3000));
        assert_eq!(default_port_for_service("astro"), Some(4321));
        assert_eq!(default_port_for_service("hugo"), Some(1313));
    }

    #[test]
    fn default_port_unknown_service_returns_none() {
        assert_eq!(default_port_for_service("custom"), None);
        assert_eq!(default_port_for_service("api"), None);
        assert_eq!(default_port_for_service("worker"), None);
    }

    #[test]
    fn short_p_does_not_match_inside_longer_flag() {
        // `--port 8420` should NOT trigger the `-p` matcher (which would
        // otherwise see the `-p` inside `--port` and bail on bad boundary).
        // We assert it returns the correct port via the `--port` matcher.
        assert_eq!(expected_port("uvicorn --port 8420"), Some(8420));
        // And a substring like `--public 6000` doesn't fool `-p`.
        assert_eq!(expected_port("./myapp --public 6000"), None);
    }
}
