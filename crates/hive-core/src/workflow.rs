//! Workflow detection: scan a worktree for declared ways to start a dev server.
//!
//! v0 sources: `scripts/` and `bin/` shell scripts, `Makefile` dev-ish targets,
//! `package.json#scripts`, `Procfile`. Each detector returns Vec<DetectedService>
//! and they're merged. No source is authoritative — user picks per row in the UI.

use crate::classify::{classify_command, classify_script_body, ServerLikelihood};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DetectedService {
    pub name: String,     // human-readable label, e.g. "scripts/start_lyon.sh", "make dev", "pnpm dev"
    pub command: String,  // exact shell command to run
    pub cwd_rel: PathBuf, // relative to worktree root, usually "."
    pub source: ServiceSource,
    pub source_file: PathBuf, // which file in the worktree produced this
    pub likelihood: ServerLikelihood, // computed at detection time
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
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else { continue };
            let name_lc = name.to_lowercase();
            let prefix_match = ["start", "dev", "run", "serve"]
                .iter()
                .any(|p| name_lc.starts_with(p));
            if !prefix_match {
                continue;
            }
            let Ok(meta) = fs::metadata(&path) else { continue };
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
            // Read the script body and classify based on its actual content.
            let likelihood = match fs::read_to_string(&path) {
                Ok(body) => classify_script_body(&body),
                Err(_) => ServerLikelihood::Maybe,
            };
            out.push(DetectedService {
                name: rel.clone(),
                command: format!("./{rel}"),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::ShellScript,
                source_file: PathBuf::from(rel),
                likelihood,
            });
        }
    }
    out
}

fn detect_makefile(root: &Path) -> Vec<DetectedService> {
    let mk = root.join("Makefile");
    let Ok(text) = fs::read_to_string(&mk) else { return vec![] };
    let keywords: &[&str] = &["dev", "start", "run", "serve", "up", "web", "api", "watch", "frontend", "backend"];
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
        out.push(DetectedService {
            name: format!("make {target}"),
            command: format!("make {target}"),
            cwd_rel: PathBuf::from("."),
            source: ServiceSource::Makefile,
            source_file: PathBuf::from("Makefile"),
            likelihood,
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
            if target.is_empty() || target.contains(|c: char| c.is_whitespace()) || target.starts_with('.') {
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
    let Ok(text) = fs::read_to_string(&pj) else { return vec![] };
    let Ok(pkg) = serde_json::from_str::<NodePackage>(&text) else { return vec![] };
    let pm = detect_node_pm(root);
    let mut out = Vec::new();
    for key in ["dev", "start", "serve"] {
        if let Some(script_value) = pkg.scripts.get(key) {
            // Classify based on what the script actually runs, not the key.
            let likelihood = classify_command(script_value);
            out.push(DetectedService {
                name: format!("{pm} {key}"),
                command: format!("{pm} run {key}"),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::NodeScript,
                source_file: PathBuf::from("package.json"),
                likelihood,
            });
        }
    }
    out
}

fn detect_node_pm(root: &Path) -> &'static str {
    if root.join("pnpm-lock.yaml").exists() { return "pnpm"; }
    if root.join("yarn.lock").exists() { return "yarn"; }
    // Bun moved from binary (bun.lockb) to text (bun.lock) — accept both.
    if root.join("bun.lock").exists() || root.join("bun.lockb").exists() { return "bun"; }
    "npm"
}

fn detect_procfile(root: &Path) -> Vec<DetectedService> {
    for name in ["Procfile", "Procfile.dev"] {
        let path = root.join(name);
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some(colon) = line.find(':') else { continue };
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
            out.push(DetectedService {
                name: format!("{name}:{entry_name}"),
                command: command.to_string(),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::Procfile,
                source_file: PathBuf::from(name),
                likelihood,
            });
        }
        if !out.is_empty() {
            return out;
        }
    }
    vec![]
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
        ).unwrap();
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
        ).unwrap();
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
        ).unwrap();
        let svcs = detect_services(d.path());
        // 3 entries surface from the Procfile.
        let procfile_svcs: Vec<_> = svcs.iter().filter(|s| s.source == ServiceSource::Procfile).collect();
        assert_eq!(procfile_svcs.len(), 3);
        let api = svcs.iter().find(|s| s.name == "Procfile:api").unwrap();
        assert_eq!(api.likelihood, ServerLikelihood::Server);
        let lint = svcs.iter().find(|s| s.name == "Procfile:lint").unwrap();
        assert_eq!(lint.likelihood, ServerLikelihood::NotServer, "lint should be filtered out even from Procfile");
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
}
