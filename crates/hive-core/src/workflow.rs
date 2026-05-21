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
    DockerCompose,
}

pub fn detect_services(worktree_path: &Path) -> Vec<DetectedService> {
    let mut out = Vec::new();
    out.extend(detect_shell_scripts(worktree_path));
    out.extend(detect_makefile(worktree_path));
    out.extend(detect_node_scripts(worktree_path));
    out.extend(detect_procfile(worktree_path));
    out.extend(detect_docker_compose(worktree_path));
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
    // Surface a script when EITHER:
    //   1. its key matches a well-known server-y keyword (dev/start/serve/
    //      storybook/api/web/frontend/backend/proxy/mock/tunnel/preview),
    //      including namespaced variants like `app:dev` / `frontend:serve`,
    //   2. OR its body classifies as Server (catches non-obviously-named
    //      scripts like `playground: vite` or `e2e:server: webpack-dev-server`).
    // Then we run the body classifier — Server/Maybe pass through, NotServer
    // gets filtered (so `lint` / `build` / `test` never appear here).
    let keywords = [
        "dev",
        "start",
        "serve",
        "storybook",
        "api",
        "web",
        "frontend",
        "backend",
        "proxy",
        "mock",
        "tunnel",
        "preview",
    ];
    for (key, script_value) in &pkg.scripts {
        let key_lc = key.to_lowercase();
        let name_match = keywords.iter().any(|k| {
            key_lc == *k
                || key_lc.ends_with(&format!(":{k}"))
                || key_lc.starts_with(&format!("{k}:"))
        });
        let likelihood = classify_command(script_value);
        if !name_match && !matches!(likelihood, ServerLikelihood::Server) {
            continue;
        }
        if matches!(likelihood, ServerLikelihood::NotServer) {
            // Even if the key matches a keyword, refuse to call something a
            // server when its body is clearly a build/test/lint.
            continue;
        }
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

/// Detect services declared in a docker-compose / compose file. Each entry in
/// `services:` becomes its own row — postgres, redis, the API container, etc.
/// The Start action surfaces as `docker compose up <name>` so users can boot
/// individual services without spinning up the whole stack.
fn detect_docker_compose(root: &Path) -> Vec<DetectedService> {
    for name in [
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ] {
        let path = root.join(name);
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(compose) = serde_yaml::from_str::<ComposeFile>(&text) else {
            continue;
        };
        let mut out = Vec::new();
        for (svc_name, svc) in compose.services {
            // Pick the first parseable host port out of the ports list. We
            // intentionally prefer the host-side port — that's what users
            // will connect to and what blocker detection cares about.
            let expected_port = svc
                .ports
                .as_ref()
                .and_then(|ps| ps.iter().find_map(parse_compose_port));
            // Compose services are server-y by convention (you don't put
            // one-shots in compose); trust the file as a strong source.
            // If the inline `command:` looks like a builder, downgrade.
            let likelihood = match svc.command.as_deref().map(classify_command) {
                Some(ServerLikelihood::NotServer) => ServerLikelihood::NotServer,
                _ => ServerLikelihood::Server,
            };
            out.push(DetectedService {
                name: svc_name.clone(),
                command: format!("docker compose up {svc_name}"),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::DockerCompose,
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

#[derive(Deserialize)]
struct ComposeFile {
    #[serde(default)]
    services: HashMap<String, ComposeService>,
}

#[derive(Deserialize)]
struct ComposeService {
    #[serde(default)]
    ports: Option<Vec<ComposePort>>,
    /// Optional command override; used only for likelihood downgrade if it
    /// looks like a builder.
    #[serde(default)]
    command: Option<String>,
}

/// Compose ports can be a short string ("8000:80") or a long-form object
/// (`{ published: 8000, target: 80 }`). Accept both via untagged.
#[derive(Deserialize)]
#[serde(untagged)]
enum ComposePort {
    Short(String),
    Long(ComposePortLong),
}

#[derive(Deserialize)]
struct ComposePortLong {
    #[serde(default)]
    published: Option<serde_yaml::Value>,
    #[serde(default)]
    target: Option<u16>,
}

/// Pull the host-side port out of a compose ports entry. Short forms:
///   "8000"                    → 8000 (single port — both sides equal)
///   "8000:80"                 → 8000 (host:container)
///   "127.0.0.1:8000:80"       → 8000 (ip:host:container)
///   "8000-8005:80-85"         → 8000 (range — first only)
/// Long form: `published` if present, else `target`.
fn parse_compose_port(p: &ComposePort) -> Option<u16> {
    match p {
        ComposePort::Short(s) => {
            let parts: Vec<&str> = s.split(':').collect();
            let host = match parts.len() {
                1 => parts[0],
                2 => parts[0],
                3 => parts[1],
                _ => return None,
            };
            let first = host.split('-').next()?;
            first.parse().ok()
        }
        ComposePort::Long(long) => {
            // `published` may be u16 or string in compose 3+; handle both.
            if let Some(v) = &long.published {
                if let Some(n) = v.as_u64() {
                    return u16::try_from(n).ok();
                }
                if let Some(s) = v.as_str() {
                    return s.split('-').next()?.parse().ok();
                }
            }
            long.target
        }
    }
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
        // Real-world shape: a `scripts/start_lyon.sh` first builds a sidecar
        // Go binary on :8421, then execs the main uvicorn server on :8420.
        // The surface command Hive runs (`./scripts/start_lyon.sh`) reveals
        // neither port; we need to peek at the script body.
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

    #[test]
    fn detects_storybook_script() {
        let d = tmpdir();
        fs::write(
            d.path().join("package.json"),
            r#"{"name":"x","scripts":{
                "storybook":"storybook dev -p 6006",
                "build-storybook":"storybook build"
            }}"#,
        )
        .unwrap();
        let svcs = detect_services(d.path());
        let sb = svcs
            .iter()
            .find(|s| s.command == "npm run storybook")
            .expect("storybook not detected");
        assert_eq!(sb.likelihood, ServerLikelihood::Server);
        assert_eq!(sb.expected_port, Some(6006));
        // `build-storybook` is a build, must NOT appear
        assert!(
            !svcs.iter().any(|s| s.name.contains("build-storybook")),
            "build-storybook should not surface as a server"
        );
    }

    #[test]
    fn detects_namespaced_node_scripts() {
        let d = tmpdir();
        fs::write(
            d.path().join("package.json"),
            r#"{"name":"x","scripts":{
                "app:dev":"vite --port 5173",
                "api:serve":"uvicorn main:app --port 9000"
            }}"#,
        )
        .unwrap();
        let svcs = detect_services(d.path());
        assert!(svcs.iter().any(|s| s.command == "npm run app:dev"));
        assert!(svcs.iter().any(|s| s.command == "npm run api:serve"));
    }

    #[test]
    fn detect_node_scripts_skips_lints_and_builds_even_if_named_dev() {
        // Hypothetical: a `dev` script that's actually a lint
        let d = tmpdir();
        fs::write(
            d.path().join("package.json"),
            r#"{"name":"x","scripts":{"dev":"eslint . --watch"}}"#,
        )
        .unwrap();
        let svcs = detect_services(d.path());
        assert!(
            svcs.iter().all(|s| s.source != ServiceSource::NodeScript),
            "linting `dev` should be filtered out by classifier"
        );
    }

    #[test]
    fn detect_docker_compose_short_form_ports() {
        let d = tmpdir();
        fs::write(
            d.path().join("docker-compose.yml"),
            r#"
services:
  postgres:
    image: postgres:17-alpine
    ports:
      - "5432:5432"
  redis:
    image: redis:7
    ports:
      - "6379"
  api:
    image: myapi:latest
    ports:
      - "127.0.0.1:8000:80"
"#,
        )
        .unwrap();
        let svcs = detect_services(d.path());
        let pg = svcs
            .iter()
            .find(|s| s.name == "postgres")
            .expect("postgres not detected");
        assert_eq!(pg.source, ServiceSource::DockerCompose);
        assert_eq!(pg.expected_port, Some(5432));
        assert_eq!(pg.command, "docker compose up postgres");
        assert_eq!(pg.likelihood, ServerLikelihood::Server);

        let redis = svcs.iter().find(|s| s.name == "redis").unwrap();
        assert_eq!(redis.expected_port, Some(6379));

        let api = svcs.iter().find(|s| s.name == "api").unwrap();
        assert_eq!(api.expected_port, Some(8000));
    }

    #[test]
    fn detect_docker_compose_long_form_ports() {
        let d = tmpdir();
        fs::write(
            d.path().join("compose.yml"),
            r#"
services:
  postgres:
    image: postgres:17
    ports:
      - target: 5432
        published: 5432
        protocol: tcp
"#,
        )
        .unwrap();
        let svcs = detect_services(d.path());
        let pg = svcs.iter().find(|s| s.name == "postgres").unwrap();
        assert_eq!(pg.expected_port, Some(5432));
    }

    #[test]
    fn detect_docker_compose_command_override_downgrades_likelihood() {
        // A compose entry whose `command:` is clearly a build/test should
        // classify as NotServer, not Server.
        let d = tmpdir();
        fs::write(
            d.path().join("docker-compose.yml"),
            r#"
services:
  test-runner:
    image: node:20
    command: npm test
"#,
        )
        .unwrap();
        let svcs = detect_services(d.path());
        let svc = svcs.iter().find(|s| s.name == "test-runner").unwrap();
        assert_eq!(svc.likelihood, ServerLikelihood::NotServer);
    }
}
