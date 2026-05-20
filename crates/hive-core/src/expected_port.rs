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
///   - `--bind ...:N` / `--bind=...:N`  (gunicorn)
///   - `PORT=N` (Procfile/env style)
///
/// Returns `None` if no match. Bare numbers in the command string are
/// intentionally ignored — too noisy (think `--workers 4`, `--max 8000`).
pub fn expected_port(cmd: &str) -> Option<u16> {
    let lc = cmd.to_lowercase();

    for flag in &["--port", "-port"] {
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

fn scan_after_flag(haystack: &str, flag: &str) -> Option<u16> {
    let idx = haystack.find(flag)?;
    let after = &haystack[idx + flag.len()..];
    // Must be followed by whitespace, '=' or end — else this is a substring match
    // like `--portable` (no real-world tool uses that for a server port, but be
    // safe).
    let first = after.chars().next();
    match first {
        None => return None,
        Some(c) if c == ' ' || c == '=' => {}
        _ => return None,
    }
    let rest = after.trim_start_matches([' ', '=']);
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let p: u16 = digits.parse().ok()?;
    if p > 0 {
        Some(p)
    } else {
        None
    }
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
}
