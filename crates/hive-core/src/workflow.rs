//! Workflow detection: scan a worktree for declared ways to start a dev server.
//!
//! v0 sources: `scripts/` and `bin/` shell scripts, `Makefile` dev-ish targets,
//! `package.json#scripts`, `Procfile`. Each detector returns Vec<DetectedService>
//! and they're merged. No source is authoritative — user picks per row in the UI.

use crate::classify::{classify_command, classify_script_body, ServerLikelihood};
use crate::expected_port::expected_port;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DetectedService {
    pub name: String, // human-readable label, e.g. "scripts/start_lyon.sh", "make dev", "pnpm dev"
    pub command: String, // exact shell command to run
    pub cwd_rel: PathBuf, // relative to worktree root, usually "."
    pub source: ServiceSource,
    pub source_file: PathBuf, // which file in the worktree produced this
    pub likelihood: ServerLikelihood, // computed at detection time
    /// Port we believe this service will bind to, when discoverable. For
    /// shell scripts and Makefile recipes that's extracted from the *body*
    /// (not just the surface command), so wrappers like `./scripts/start_lyon.sh`
    /// still get pre-warn blocker detection in the Servers view.
    pub expected_port: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceSource {
    ShellScript,
    Makefile,
    NodeScript,
    Procfile,
}

pub fn detect_services(worktree_path: &Path) -> Vec<DetectedService> {
    let mut out = Vec::new();
    out.extend(detect_shell_scripts(worktree_path));
    out.extend(detect_makefile(worktree_path));
    out.extend(detect_node_scripts(worktree_path));
    out.extend(detect_procfile(worktree_path));
    out
}

fn detect_shell_scripts(root: &Path) -> Vec<DetectedService> {
    let mut out = Vec::new();
    for subdir in ["scripts", "bin"] {
        let dir = root.join(subdir);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let name_lc = name.to_lowercase();
            let prefix_match = ["start", "dev", "run", "serve"]
                .iter()
                .any(|p| name_lc.starts_with(p));
            if !prefix_match {
                continue;
            }
            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o111 == 0 {
                    continue;
                }
            }
            let rel = format!("{subdir}/{name}");
            // Read the script body once and use it for BOTH classification and
            // port extraction. Without this, port-blocker pre-warning misses
            // any service whose surface command is just `./scripts/foo.sh` —
            // the port lives in the body, not the command we hand to sh.
            let (likelihood, port_from_body) = match fs::read_to_string(&path) {
                Ok(body) => (classify_script_body(&body), port_from_script_body(&body)),
                Err(_) => (ServerLikelihood::Maybe, None),
            };
            out.push(DetectedService {
                name: rel.clone(),
                command: format!("./{rel}"),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::ShellScript,
                source_file: PathBuf::from(rel),
                likelihood,
                expected_port: port_from_body,
            });
        }
    }
    out
}

fn detect_makefile(root: &Path) -> Vec<DetectedService> {
    let mk = root.join("Makefile");
    let Ok(text) = fs::read_to_string(&mk) else {
        return vec![];
    };
    let keywords: &[&str] = &[
        "dev", "start", "run", "serve", "up", "web", "api", "watch", "frontend", "backend",
    ];
    // Parse out a map of target -> recipe lines so we can classify each by its actual body.
    let recipes = parse_makefile_recipes(&text);
    let mut out = Vec::new();
    for (target, recipe_lines) in &recipes {
        let target_lc = target.to_lowercase();
        if !keywords.iter().any(|k| target_lc == *k) {
            continue;
        }
        let recipe_body = recipe_lines.join("\n");
        let likelihood = classify_script_body(&recipe_body);
        let expected_port = port_from_script_body(&recipe_body);
        out.push(DetectedService {
            name: format!("make {target}"),
            command: format!("make {target}"),
            cwd_rel: PathBuf::from("."),
            source: ServiceSource::Makefile,
            source_file: PathBuf::from("Makefile"),
            likelihood,
            expected_port,
        });
    }
    out
}

/// Parse a Makefile into target -> recipe lines. Skips dependencies, .PHONY,
/// comments. Recipe lines are those indented with tab or space immediately
/// after a target: line.
fn parse_makefile_recipes(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut current: Option<(String, Vec<String>)> = None;
    for line in text.lines() {
        if line.starts_with('\t') || (line.starts_with(' ') && !line.trim().is_empty()) {
            if let Some((_, recipe)) = current.as_mut() {
                recipe.push(line.trim_start().to_string());
            }
            continue;
        }
        // Non-recipe line: either a new target or anything else (blank, comment, variable).
        if let Some(colon) = line.find(':') {
            if colon == 0 || line.starts_with('#') {
                if let Some(prev) = current.take() {
                    out.push(prev);
                }
                continue;
            }
            let target = line[..colon].trim();
            if target.is_empty()
                || target.contains(|c: char| c.is_whitespace())
                || target.starts_with('.')
            {
                if let Some(prev) = current.take() {
                    out.push(prev);
                }
                continue;
            }
            if let Some(prev) = current.take() {
                out.push(prev);
            }
            current = Some((target.to_string(), Vec::new()));
        } else {
            // blank / variable / comment / etc.
            if let Some(prev) = current.take() {
                out.push(prev);
            }
        }
    }
    if let Some(prev) = current.take() {
        out.push(prev);
    }
    out
}

fn detect_node_scripts(root: &Path) -> Vec<DetectedService> {
    let pj = root.join("package.json");
    let Ok(text) = fs::read_to_string(&pj) else {
        return vec![];
    };
    let Ok(pkg) = serde_json::from_str::<NodePackage>(&text) else {
        return vec![];
    };
    let pm = detect_node_pm(root);
    let mut out = Vec::new();
    for key in ["dev", "start", "serve"] {
        if let Some(script_value) = pkg.scripts.get(key) {
            // Classify based on what the script actually runs, not the key.
            // Port also comes from the *script value*, not the surface
            // `pnpm run dev` we'll hand to the shell.
            let likelihood = classify_command(script_value);
            let expected_port = expected_port(script_value);
            out.push(DetectedService {
                name: format!("{pm} {key}"),
                command: format!("{pm} run {key}"),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::NodeScript,
                source_file: PathBuf::from("package.json"),
                likelihood,
                expected_port,
            });
        }
    }
    out
}

fn detect_node_pm(root: &Path) -> &'static str {
    if root.join("pnpm-lock.yaml").exists() {
        return "pnpm";
    }
    if root.join("yarn.lock").exists() {
        return "yarn";
    }
    // Bun moved from binary (bun.lockb) to text (bun.lock) — accept both.
    if root.join("bun.lock").exists() || root.join("bun.lockb").exists() {
        return "bun";
    }
    "npm"
}

fn detect_procfile(root: &Path) -> Vec<DetectedService> {
    for name in ["Procfile", "Procfile.dev"] {
        let path = root.join(name);
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some(colon) = line.find(':') else {
                continue;
            };
            let entry_name = line[..colon].trim();
            let command = line[colon + 1..].trim();
            if entry_name.is_empty() || command.is_empty() {
                continue;
            }
            // Procfile entries are by convention long-running processes. We
            // still run the command-content classifier so a misnamed entry
            // (e.g. `lint: eslint .`) gets correctly tagged as NotServer.
            // For ambiguous/Maybe results, promote to Server because of the
            // Procfile-by-convention signal.
            let cmd_class = classify_command(command);
            let likelihood = match cmd_class {
                ServerLikelihood::NotServer => ServerLikelihood::NotServer,
                _ => ServerLikelihood::Server,
            };
            let expected_port = expected_port(command);
            out.push(DetectedService {
                name: format!("{name}:{entry_name}"),
                command: command.to_string(),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::Procfile,
                source_file: PathBuf::from(name),
                likelihood,
                expected_port,
            });
        }
        if !out.is_empty() {
            return out;
        }
    }
    vec![]
}

/// Pull the most-likely listening port out of a multi-line script body
/// (shell script or Makefile recipe), ignoring comment lines.
///
/// Comments are stripped first, then the remaining lines are joined and
/// handed to `expected_port` as a single blob. That preserves the flag
/// priority `expected_port` cares about — `--port` beats `-port` beats
/// `--bind` beats `PORT=` — across the whole script rather than line by
/// line. For `start_lyon.sh` that means `uvicorn … --port 8420` wins over
/// `lyon-bundle -port 8421`, which is the right choice for blocker
/// detection: 8420 is the user-facing port.
fn port_from_script_body(body: &str) -> Option<u16> {
    let cleaned = body
        .lines()
        .filter_map(|raw| {
            let line = raw.trim_start_matches([' ', '\t']);
            if line.is_empty() || line.starts_with('#') {
                None
            } else {
                Some(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    expected_port(&cleaned)
}

#[derive(Deserialize)]
struct NodePackage {
    #[serde(default)]
    scripts: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmpdir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn detects_makefile_targets() {
        let d = tmpdir();
        let mk = d.path().join("Makefile");
        let mut f = fs::File::create(&mk).unwrap();
        writeln!(f, "dev: ## start dev server").unwrap();
        writeln!(f, "\tnpm run dev").unwrap();
        writeln!(f, "test:").unwrap();
        writeln!(f, "\tcargo test").unwrap();
        writeln!(f, ".PHONY: dev test").unwrap();
        let svcs = detect_services(d.path());
        let names: Vec<_> = svcs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"make dev"), "got {names:?}");
        assert!(!names.contains(&"make test"));
    }

    #[test]
    fn detects_node_scripts_with_pm() {
        let d = tmpdir();
        fs::write(
            d.path().join("package.json"),
            r#"{"name":"x","scripts":{"dev":"vite","build":"tsc"}}"#,
        )
        .unwrap();
        fs::write(d.path().join("pnpm-lock.yaml"), "").unwrap();
        let svcs = detect_services(d.path());
        assert!(svcs.iter().any(|s| s.command == "pnpm run dev"));
        assert!(!svcs.iter().any(|s| s.name.contains("build")));
    }

    #[test]
    fn bun_text_lockfile_is_recognized() {
        let d = tmpdir();
        fs::write(
            d.path().join("package.json"),
            r#"{"name":"x","scripts":{"dev":"vite"}}"#,
        )
        .unwrap();
        // Bun's new text-based lockfile.
        fs::write(d.path().join("bun.lock"), "").unwrap();
        let svcs = detect_services(d.path());
        assert!(svcs.iter().any(|s| s.command == "bun run dev"));
    }

    #[test]
    fn detects_procfile_entries_with_likelihood() {
        let d = tmpdir();
        fs::write(
            d.path().join("Procfile"),
            "api: uvicorn src.main:app --reload --port 8000\nweb: bun run dev\nlint: eslint .\n",
        )
        .unwrap();
        let svcs = detect_services(d.path());
        // 3 entries surface from the Procfile.
        let procfile_svcs: Vec<_> = svcs
            .iter()
            .filter(|s| s.source == ServiceSource::Procfile)
            .collect();
        assert_eq!(procfile_svcs.len(), 3);
        let api = svcs.iter().find(|s| s.name == "Procfile:api").unwrap();
        assert_eq!(api.likelihood, ServerLikelihood::Server);
        let lint = svcs.iter().find(|s| s.name == "Procfile:lint").unwrap();
        assert_eq!(
            lint.likelihood,
            ServerLikelihood::NotServer,
            "lint should be filtered out even from Procfile"
        );
    }

    #[test]
    fn detects_executable_shell_scripts() {
        let d = tmpdir();
        let scripts = d.path().join("scripts");
        fs::create_dir(&scripts).unwrap();
        let p = scripts.join("start_lyon.sh");
        fs::write(&p, "#!/bin/sh\necho hi\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&p, perms).unwrap();
        }
        let svcs = detect_services(d.path());
        assert!(svcs.iter().any(|s| s.command == "./scripts/start_lyon.sh"));
    }

    #[cfg(unix)]
    fn write_executable(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, body).unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn shell_script_port_pulled_from_body() {
        // Mirrors the shape of alpha/scripts/start_lyon.sh: build a
        // Go binary on :8421, then exec uvicorn on :8420. The surface command
        // Hive runs (`./scripts/start_lyon.sh`) reveals neither port; we need
        // to peek at the body.
        let d = tmpdir();
        let scripts = d.path().join("scripts");
        fs::create_dir(&scripts).unwrap();
        let body = r#"#!/bin/bash
set -euo pipefail
go build -o /tmp/lyon-bundle .
/tmp/lyon-bundle -port 8421 &
BUNDLE_PID=$!
trap "kill $BUNDLE_PID" EXIT
exec uv run uvicorn lyon.server:app --reload --port 8420
"#;
        write_executable(&scripts.join("start_lyon.sh"), body);
        let svcs = detect_services(d.path());
        let svc = svcs
            .iter()
            .find(|s| s.command == "./scripts/start_lyon.sh")
            .expect("shell script not detected");
        // expected_port searches for `--port` before `-port`, so it returns
        // the uvicorn port (the user-facing one) — which is the right pick
        // for blocker detection.
        assert_eq!(svc.expected_port, Some(8420));
    }

    #[cfg(unix)]
    #[test]
    fn shell_script_port_ignores_commented_lines() {
        let d = tmpdir();
        let scripts = d.path().join("scripts");
        fs::create_dir(&scripts).unwrap();
        // Body has a commented-out port directive on the first non-blank line —
        // a naive `expected_port(&body)` would still match through it; we
        // require a scan that strips comments.
        let body = "#!/bin/sh\n# uvicorn app --port 9999  # historical\nuvicorn app --port 7000\n";
        write_executable(&scripts.join("serve.sh"), body);
        let svcs = detect_services(d.path());
        let svc = svcs
            .iter()
            .find(|s| s.command == "./scripts/serve.sh")
            .expect("shell script not detected");
        assert_eq!(svc.expected_port, Some(7000));
    }

    #[test]
    fn makefile_recipe_port_extracted_from_body() {
        let d = tmpdir();
        let mk = d.path().join("Makefile");
        let mut f = fs::File::create(&mk).unwrap();
        // `make serve` itself reveals no port — but the recipe does.
        writeln!(f, "serve:").unwrap();
        writeln!(f, "\tuvicorn app:main --reload --port 5050").unwrap();
        let svcs = detect_services(d.path());
        let svc = svcs
            .iter()
            .find(|s| s.name == "make serve")
            .expect("make serve not detected");
        assert_eq!(svc.expected_port, Some(5050));
    }

    #[test]
    fn node_script_port_extracted_from_value() {
        let d = tmpdir();
        fs::write(
            d.path().join("package.json"),
            r#"{"name":"x","scripts":{"dev":"vite --port 5173"}}"#,
        )
        .unwrap();
        let svcs = detect_services(d.path());
        let dev = svcs
            .iter()
            .find(|s| s.command == "npm run dev")
            .expect("npm run dev not detected");
        assert_eq!(dev.expected_port, Some(5173));
    }

    #[test]
    fn procfile_port_extracted_from_command() {
        let d = tmpdir();
        fs::write(
            d.path().join("Procfile"),
            "api: uvicorn src.main:app --reload --port 8000\nweb: bun run dev\n",
        )
        .unwrap();
        let svcs = detect_services(d.path());
        let api = svcs
            .iter()
            .find(|s| s.name == "Procfile:api")
            .expect("api not detected");
        assert_eq!(api.expected_port, Some(8000));
        let web = svcs
            .iter()
            .find(|s| s.name == "Procfile:web")
            .expect("web not detected");
        assert_eq!(web.expected_port, None, "no port flag → None");
    }
}
