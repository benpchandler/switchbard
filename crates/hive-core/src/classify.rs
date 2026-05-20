//! Content-based "is this a real server?" classifier.
//!
//! Given a command string (what the entry will actually execute) and optional
//! script body, returns Server / Maybe / NotServer. Used to filter out one-off
//! scripts like ship-gate runners and smoke-test wrappers from the Servers view.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerLikelihood {
    /// Confident this starts a long-running server.
    Server,
    /// Ambiguous — could be a server, could be a one-shot. Show with a soft signal.
    Maybe,
    /// Confident this is NOT a server (test, build, lint, migration, etc.).
    NotServer,
}

/// Classify based on a command line (what `sh -c` would run).
///
/// Strategy:
///   1. A STRONG positive (uvicorn, vite serve, docker compose up, …) wins
///      over any negatives in the same string — strong tokens are unambiguous
///      "this is a long-running server" signals.
///   2. NEGATIVES (tests, builders, linters) override WEAK positives
///      (`--reload`, bare `vite`, `--watch`) — so `vite build --watch` lands
///      as NotServer / Maybe, not Server.
///   3. Without any signal → Maybe.
pub fn classify_command(cmd: &str) -> ServerLikelihood {
    let s = cmd.to_lowercase();
    let strong_positives = count_tokens(&s, STRONG_POSITIVE_TOKENS);
    let weak_positives = count_tokens(&s, WEAK_POSITIVE_TOKENS);
    let negatives = count_tokens(&s, NEGATIVE_TOKENS);

    if strong_positives >= 1 {
        return ServerLikelihood::Server;
    }
    if negatives >= 1 {
        // Any negative beats weak positives — `vite build --watch` is a builder,
        // not a server, even though `--watch` looks server-y.
        return ServerLikelihood::NotServer;
    }
    if weak_positives >= 1 {
        return ServerLikelihood::Server;
    }
    ServerLikelihood::Maybe
}

/// Classify a shell script body (full file content). Looks for any line that
/// classifies as `Server` — a build-then-serve script (compile setup + uvicorn)
/// is a server. If no Server line and any NotServer line, it's NotServer
/// (test/lint scripts). Mixed-but-no-strong is Maybe.
pub fn classify_script_body(body: &str) -> ServerLikelihood {
    let mut any_server_line = false;
    let mut any_notserver_line = false;
    let mut any_weak_signal = false;

    // Body-level shell signals (script structure, not in a single line).
    let s = body.to_lowercase();
    if s.contains("trap ") && (s.contains("exit") || s.contains("term") || s.contains("int")) {
        any_weak_signal = true;
    }
    if s.contains("--reload") || s.contains("--watch") {
        any_weak_signal = true;
    }

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !trimmed.contains(' ') && trimmed.contains('=') {
            continue; // variable assignment
        }
        match classify_command(trimmed) {
            ServerLikelihood::Server => any_server_line = true,
            ServerLikelihood::NotServer => any_notserver_line = true,
            ServerLikelihood::Maybe => {}
        }
    }

    if any_server_line {
        // A script that starts a server WINS even if it also builds/lints as setup.
        // start_lyon.sh runs `go build` (negative) before `uvicorn` (strong positive) —
        // it's a server-start script, full stop.
        ServerLikelihood::Server
    } else if any_notserver_line {
        ServerLikelihood::NotServer
    } else {
        // `any_weak_signal` is computed above for future use (e.g. a "soft Maybe"
        // tier) but today both with-and-without-weak-signal land at Maybe.
        let _ = any_weak_signal;
        ServerLikelihood::Maybe
    }
}

fn count_tokens(s: &str, tokens: &[&str]) -> usize {
    let mut n = 0;
    for needle in tokens {
        if contains_word(s, needle) {
            n += 1;
        }
    }
    n
}

/// Word-ish containment: must be preceded and followed by non-alphanumeric or
/// string boundary. Prevents 'air' matching 'pairing' or 'next' matching 'context'.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let needle_lc = needle.to_lowercase();
    let mut start = 0;
    while let Some(idx) = haystack[start..].find(&needle_lc) {
        let pos = start + idx;
        let before_ok = pos == 0
            || haystack.as_bytes()[pos - 1].is_ascii_punctuation()
            || haystack.as_bytes()[pos - 1].is_ascii_whitespace()
            || haystack.as_bytes()[pos - 1] == b'/'
            || haystack.as_bytes()[pos - 1] == b'$';
        let after_idx = pos + needle_lc.len();
        let after_ok = after_idx >= haystack.len()
            || haystack.as_bytes()[after_idx].is_ascii_punctuation()
            || haystack.as_bytes()[after_idx].is_ascii_whitespace()
            || haystack.as_bytes()[after_idx] == b'/'
            || haystack.as_bytes()[after_idx] == b':';
        if before_ok && after_ok {
            return true;
        }
        start = pos + needle_lc.len().max(1);
    }
    false
}

/// Unambiguous "this is a long-running server" — overrides any negatives in
/// the same string. A script that `go build`s then runs `uvicorn` is a server,
/// not a builder.
const STRONG_POSITIVE_TOKENS: &[&str] = &[
    // Python servers
    "uvicorn",
    "gunicorn",
    "hypercorn",
    "daphne",
    // Node / JS dev servers
    "nodemon",
    "next dev",
    "next start",
    "webpack-dev-server",
    "ng serve",
    "remix dev",
    "astro dev",
    "vue-cli-service serve",
    "concurrently",
    "vite serve",
    "vite preview",
    "vite dev",
    // Ruby
    "rails s",
    "rails server",
    "puma",
    "unicorn",
    // Go
    "air",
    // Rust
    "cargo-watch",
    // PHP
    "artisan serve",
    // .NET
    "dotnet watch",
    // Container orchestration
    "docker compose up",
    "docker-compose up",
    // Process orchestrators
    "foreman start",
    "overmind start",
    "honcho start",
    "hivemind",
    "goreman start",
];

/// Ambiguous "could be a server" — overridden by negatives. Bare `vite` (no
/// args) is usually `vite serve` by default but `vite build` is a builder; a
/// `--reload` flag is server-y but could appear in other contexts.
const WEAK_POSITIVE_TOKENS: &[&str] = &["vite", "--reload", "--watch", "--dev"];

/// Tokens whose presence implies "this is NOT a server" — overrides weak
/// positives, overridden by strong positives.
const NEGATIVE_TOKENS: &[&str] = &[
    // Test runners
    "pytest",
    "vitest",
    "jest",
    "mocha",
    "playwright test",
    "go test",
    "cargo test",
    "rspec",
    "phpunit",
    // Builders / type checkers
    "tsc",
    "cargo build",
    "cargo check",
    "cargo clippy",
    "go build",
    "go vet",
    "vite build",
    "next build",
    "webpack build",
    "rollup",
    "esbuild",
    // Linters / formatters
    "ruff check",
    "ruff format",
    "eslint",
    "mypy",
    "prettier",
    "rustfmt",
    "gofmt",
    "black",
    "flake8",
    // Schema / DB ops
    "alembic",
    "migrate",
    "db:migrate",
    "db:seed",
    "knex migrate",
    // Other one-shots
    "deploy",
    "release",
    "publish",
    "ship-gate",
    "smoke",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uvicorn_command_is_server() {
        assert_eq!(
            classify_command("uv run uvicorn lyon.server:app --reload --port 8420"),
            ServerLikelihood::Server
        );
    }

    #[test]
    fn vite_is_server() {
        assert_eq!(classify_command("bun run dev"), ServerLikelihood::Maybe);
        // Bare `vite` is a weak positive (defaults to `vite serve`) → Server.
        assert_eq!(classify_command("vite"), ServerLikelihood::Server);
        assert_eq!(
            classify_command("vite --port 5173"),
            ServerLikelihood::Server
        );
        assert_eq!(classify_command("vite serve"), ServerLikelihood::Server);
    }

    #[test]
    fn pytest_is_not_a_server_even_if_called_run() {
        assert_eq!(
            classify_command("pytest tests/ -v"),
            ServerLikelihood::NotServer
        );
    }

    #[test]
    fn builder_with_watch_is_not_a_server() {
        // `vite build --watch` rebuilds on change — still not a server.
        assert_eq!(
            classify_command("vite build --watch"),
            ServerLikelihood::NotServer
        );
    }

    #[test]
    fn plain_builder_is_not_server() {
        assert_eq!(classify_command("tsc"), ServerLikelihood::NotServer);
        assert_eq!(classify_command("vite build"), ServerLikelihood::NotServer);
    }

    #[test]
    fn lyon_ship_gate_is_not_server() {
        let body = "#!/bin/bash\nset -euo pipefail\nuv run pytest lyon/tests/test_ship_gate.py\nruff check lyon\n";
        assert_eq!(classify_script_body(body), ServerLikelihood::NotServer);
    }

    #[test]
    fn start_lyon_script_is_server() {
        // Shape similar to start_lyon.sh: bg process + wait, uvicorn with --reload.
        let body = r#"#!/bin/bash
set -euo pipefail
go build -o /tmp/lyon-bundle .
/tmp/lyon-bundle -port 8421 &
BUNDLE_PID=$!
trap "kill $BUNDLE_PID" EXIT
exec uv run uvicorn lyon.server:app --reload --port 8420
"#;
        assert_eq!(classify_script_body(body), ServerLikelihood::Server);
    }

    #[test]
    fn random_python_script_is_maybe() {
        let body = "#!/usr/bin/env python\nimport sys\nprint('hello')\nsys.exit(0)\n";
        assert_eq!(classify_script_body(body), ServerLikelihood::Maybe);
    }

    #[test]
    fn word_boundary_avoids_substring_false_positive() {
        // 'pairing' shouldn't match 'air'.
        assert_eq!(classify_command("pairing socket"), ServerLikelihood::Maybe);
        // 'context' shouldn't match 'next' tokens (we only have 'next dev'/'next start').
        assert_eq!(classify_command("context build"), ServerLikelihood::Maybe);
    }

    #[test]
    fn explicit_test_or_lint_in_command() {
        assert_eq!(
            classify_command("eslint . --max-warnings 0"),
            ServerLikelihood::NotServer
        );
        assert_eq!(
            classify_command("cargo test --workspace"),
            ServerLikelihood::NotServer
        );
    }
}
