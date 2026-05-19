//! Workflow detection: scan a worktree for declared ways to start a dev server.
//!
//! v0 sources: `scripts/` and `bin/` shell scripts, `Makefile` dev-ish targets,
//! `package.json#scripts`, `Procfile`. Each detector returns Vec<DetectedService>
//! and they're merged. No source is authoritative — user picks per row in the UI.

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
            out.push(DetectedService {
                name: rel.clone(),
                command: format!("./{rel}"),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::ShellScript,
                source_file: PathBuf::from(rel),
            });
        }
    }
    out
}

fn detect_makefile(root: &Path) -> Vec<DetectedService> {
    let mk = root.join("Makefile");
    let Ok(text) = fs::read_to_string(&mk) else { return vec![] };
    let keywords: &[&str] = &["dev", "start", "run", "serve", "up", "web", "api", "watch", "frontend", "backend"];
    let mut out = Vec::new();
    for line in text.lines() {
        // Quick filter — a target line starts at column 0 (no tab/space) and has a colon.
        let Some(colon) = line.find(':') else { continue };
        if colon == 0 || line.starts_with(' ') || line.starts_with('\t') || line.starts_with('#') {
            continue;
        }
        let target = line[..colon].trim();
        if target.is_empty() || target.contains(|c: char| c.is_whitespace()) || target.starts_with('.') {
            continue;
        }
        let target_lc = target.to_lowercase();
        if !keywords.iter().any(|k| target_lc == *k) {
            continue;
        }
        out.push(DetectedService {
            name: format!("make {target}"),
            command: format!("make {target}"),
            cwd_rel: PathBuf::from("."),
            source: ServiceSource::Makefile,
            source_file: PathBuf::from("Makefile"),
        });
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
        if pkg.scripts.contains_key(key) {
            out.push(DetectedService {
                name: format!("{pm} {key}"),
                command: format!("{pm} run {key}"),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::NodeScript,
                source_file: PathBuf::from("package.json"),
            });
        }
    }
    out
}

fn detect_node_pm(root: &Path) -> &'static str {
    if root.join("pnpm-lock.yaml").exists() { return "pnpm"; }
    if root.join("yarn.lock").exists() { return "yarn"; }
    if root.join("bun.lockb").exists() { return "bun"; }
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
            out.push(DetectedService {
                name: format!("{name}:{entry_name}"),
                command: command.to_string(),
                cwd_rel: PathBuf::from("."),
                source: ServiceSource::Procfile,
                source_file: PathBuf::from(name),
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
