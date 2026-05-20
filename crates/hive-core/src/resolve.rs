//! Cluster `DetectedService` entry-points into logical `ResolvedService`s.
//!
//! The detectors (`workflow::detect_services`) produce one record per *way
//! to start something*: a Procfile entry, a Makefile target, a Node script,
//! a docker-compose service, a shell script. From the user's point of view,
//! many of those are aliases for the same logical service — `make run` and
//! `Procfile:api` both launch the FastAPI backend, on the same port.
//!
//! `resolve` groups them. Within a single worktree, detections that share an
//! `expected_port` collapse into one service; detections without a port stay
//! as their own service. The canonical name is picked by a small priority
//! ladder (Procfile > docker-compose > Node script > Makefile > shell
//! script) so the user sees the most-recognizable name.
//!
//! Cross-worktree clustering is *not* attempted: the same `expected_port`
//! in two different worktrees is two different services (you can only
//! actually bind one at a time, and the user's mental model is per-worktree
//! anyway).

use crate::classify::ServerLikelihood;
use crate::workflow::{DetectedService, ServiceSource};

/// A logical service — one or more entry-points that all start the same
/// underlying process.
#[derive(Debug, Clone)]
pub struct ResolvedService {
    /// User-facing name: `api`, `web`, `postgres`, `storybook`, …
    pub canonical_name: String,
    /// Port the service binds to, if any of the entry points expose one.
    pub expected_port: Option<u16>,
    /// Most-confident likelihood among the entry points (Server > Maybe >
    /// NotServer — see `combine_likelihood`).
    pub likelihood: ServerLikelihood,
    /// All entry points that launch this service. Sorted by priority
    /// (preferred Start first). At least one entry; the first is what
    /// `Start` should run.
    pub entry_points: Vec<DetectedService>,
}

impl ResolvedService {
    /// The entry point Start should invoke by default — the highest-priority
    /// detector's command.
    pub fn primary_entry_point(&self) -> &DetectedService {
        &self.entry_points[0]
    }
}

/// Cluster raw detections into resolved services. Input order is preserved
/// for stability (canonical name selection ties broken by first-seen).
pub fn resolve(mut detected: Vec<DetectedService>) -> Vec<ResolvedService> {
    // Stable-sort by detector priority first so primary_entry_point lookups
    // are O(1) once a cluster is formed. Source priority mirrors the
    // canonical-name ladder.
    detected.sort_by_key(|d| source_priority(d.source));

    let mut by_port: Vec<(u16, Vec<DetectedService>)> = Vec::new();
    let mut no_port: Vec<DetectedService> = Vec::new();

    for d in detected {
        match d.expected_port {
            Some(port) => match by_port.iter_mut().find(|(p, _)| *p == port) {
                Some((_, bucket)) => bucket.push(d),
                None => by_port.push((port, vec![d])),
            },
            None => no_port.push(d),
        }
    }

    let mut out = Vec::new();
    for (port, mut entries) in by_port {
        entries.sort_by_key(|d| source_priority(d.source));
        out.push(ResolvedService {
            canonical_name: canonical_name(&entries),
            expected_port: Some(port),
            likelihood: combine_likelihood(&entries),
            entry_points: entries,
        });
    }
    for d in no_port {
        let name = canonical_name_single(&d);
        let likelihood = d.likelihood;
        out.push(ResolvedService {
            canonical_name: name,
            expected_port: None,
            likelihood,
            entry_points: vec![d],
        });
    }
    out
}

/// Lower is higher priority. Drives both the canonical-name pick and the
/// order entries appear in `entry_points`.
fn source_priority(s: ServiceSource) -> u8 {
    match s {
        ServiceSource::Procfile => 0,
        ServiceSource::DockerCompose => 1,
        ServiceSource::NodeScript => 2,
        ServiceSource::Makefile => 3,
        ServiceSource::ShellScript => 4,
    }
}

/// Pick the cluster's display name. Procfile and compose detections carry
/// the conventional name in `name` already (`Procfile:api` → `api`;
/// compose's name field is the bare service name).
fn canonical_name(entries: &[DetectedService]) -> String {
    let primary = &entries[0];
    canonical_name_single(primary)
}

fn canonical_name_single(d: &DetectedService) -> String {
    match d.source {
        ServiceSource::Procfile => d
            .name
            .strip_prefix("Procfile.dev:")
            .or_else(|| d.name.strip_prefix("Procfile:"))
            .unwrap_or(&d.name)
            .to_string(),
        ServiceSource::DockerCompose => d.name.clone(),
        ServiceSource::NodeScript => d
            .name
            .split_once(' ')
            .map(|(_, k)| k.to_string())
            .unwrap_or_else(|| d.name.clone()),
        ServiceSource::Makefile => d.name.strip_prefix("make ").unwrap_or(&d.name).to_string(),
        ServiceSource::ShellScript => d
            .name
            .rsplit('/')
            .next()
            .unwrap_or(&d.name)
            .trim_end_matches(".sh")
            .to_string(),
    }
}

/// Pick the most confident likelihood across all entries: any Server wins;
/// otherwise any NotServer beats Maybe; otherwise Maybe.
fn combine_likelihood(entries: &[DetectedService]) -> ServerLikelihood {
    let mut has_server = false;
    let mut has_not = false;
    for e in entries {
        match e.likelihood {
            ServerLikelihood::Server => has_server = true,
            ServerLikelihood::NotServer => has_not = true,
            ServerLikelihood::Maybe => {}
        }
    }
    if has_server {
        ServerLikelihood::Server
    } else if has_not {
        ServerLikelihood::NotServer
    } else {
        ServerLikelihood::Maybe
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn d(name: &str, source: ServiceSource, port: Option<u16>) -> DetectedService {
        DetectedService {
            name: name.to_string(),
            command: name.to_string(),
            cwd_rel: PathBuf::from("."),
            source,
            source_file: PathBuf::from(""),
            likelihood: ServerLikelihood::Server,
            expected_port: port,
        }
    }

    #[test]
    fn clusters_same_port_within_worktree() {
        // `make run` and `Procfile:api` both bind 8000 → one service.
        let detected = vec![
            d("make run", ServiceSource::Makefile, Some(8000)),
            d("Procfile:api", ServiceSource::Procfile, Some(8000)),
        ];
        let resolved = resolve(detected);
        assert_eq!(resolved.len(), 1);
        let svc = &resolved[0];
        assert_eq!(svc.canonical_name, "api"); // Procfile beats Makefile
        assert_eq!(svc.expected_port, Some(8000));
        assert_eq!(svc.entry_points.len(), 2);
        // Primary entry should be the Procfile one (highest priority).
        assert_eq!(svc.primary_entry_point().source, ServiceSource::Procfile);
    }

    #[test]
    fn different_ports_stay_separate() {
        let detected = vec![
            d("Procfile:api", ServiceSource::Procfile, Some(8000)),
            d("Procfile:web", ServiceSource::Procfile, Some(3000)),
        ];
        let resolved = resolve(detected);
        assert_eq!(resolved.len(), 2);
        let names: Vec<_> = resolved.iter().map(|r| r.canonical_name.as_str()).collect();
        assert!(names.contains(&"api"));
        assert!(names.contains(&"web"));
    }

    #[test]
    fn no_port_entries_stay_solo() {
        // `make dev` and `Procfile:web` both have no port — separate services.
        let detected = vec![
            d("make dev", ServiceSource::Makefile, None),
            d("Procfile:web", ServiceSource::Procfile, None),
        ];
        let resolved = resolve(detected);
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn canonical_name_priority_compose_over_makefile() {
        // No Procfile in the mix; compose wins over Makefile.
        let detected = vec![
            d("make run", ServiceSource::Makefile, Some(5432)),
            d("postgres", ServiceSource::DockerCompose, Some(5432)),
        ];
        let resolved = resolve(detected);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].canonical_name, "postgres");
    }

    #[test]
    fn canonical_name_strips_node_script_prefix() {
        let detected = vec![d("npm storybook", ServiceSource::NodeScript, Some(6006))];
        let resolved = resolve(detected);
        assert_eq!(resolved[0].canonical_name, "storybook");
    }

    #[test]
    fn canonical_name_strips_shell_script_path_and_extension() {
        let detected = vec![d(
            "scripts/start_lyon.sh",
            ServiceSource::ShellScript,
            Some(8420),
        )];
        let resolved = resolve(detected);
        assert_eq!(resolved[0].canonical_name, "start_lyon");
    }

    #[test]
    fn combine_likelihood_prefers_server() {
        let mut a = d("a", ServiceSource::Procfile, Some(7000));
        a.likelihood = ServerLikelihood::Server;
        let mut b = d("b", ServiceSource::Makefile, Some(7000));
        b.likelihood = ServerLikelihood::Maybe;
        let resolved = resolve(vec![a, b]);
        assert_eq!(resolved[0].likelihood, ServerLikelihood::Server);
    }

    #[test]
    fn combine_likelihood_falls_back_to_notserver_when_no_server() {
        let mut a = d("a", ServiceSource::Procfile, Some(7000));
        a.likelihood = ServerLikelihood::NotServer;
        let mut b = d("b", ServiceSource::Makefile, Some(7000));
        b.likelihood = ServerLikelihood::Maybe;
        let resolved = resolve(vec![a, b]);
        assert_eq!(resolved[0].likelihood, ServerLikelihood::NotServer);
    }
}
