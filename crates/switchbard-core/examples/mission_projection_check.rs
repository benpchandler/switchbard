//! Read one xplan Mission Command snapshot through Switchbard's real adapter.
//!
//! This is a bounded, read-only cross-repository verification seam. Successful
//! output is exactly one JSON object on stdout; diagnostics go to stderr.
//!
//! ```sh
//! cargo run -q -p switchbard-core --example mission_projection_check -- <snapshot.json>
//! ```

use serde_json::{json, Value};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use switchbard_core::{load_mission_projection, MissionProjectionLoad, ProjectionFreshness};

const USAGE: &str = "usage: mission_projection_check <snapshot.json>";

fn main() -> ExitCode {
    let path = match snapshot_path(std::env::args_os().skip(1)) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    check_projection(&path)
}

fn check_projection(path: &std::path::Path) -> ExitCode {
    match load_mission_projection(path) {
        MissionProjectionLoad::Ready {
            projection,
            freshness,
            ..
        } => {
            let summary = json!({
                "schema_version": projection.schema_version,
                "revision": projection.revision,
                "mission_count": projection.portfolio.missions.len(),
                "adopted_source_count": projection.portfolio.missions.iter()
                    .filter(|mission| mission.source_revision.is_some()).count(),
                "freshness": freshness_summary(freshness),
            });
            println!("{summary}");
            ExitCode::SUCCESS
        }
        state => {
            eprintln!("{}", load_failure(&state));
            ExitCode::FAILURE
        }
    }
}

fn snapshot_path(args: impl IntoIterator<Item = OsString>) -> Result<PathBuf, &'static str> {
    let mut args = args.into_iter().take(2);
    let Some(path) = args.next() else {
        return Err(USAGE);
    };
    if path.is_empty() || args.next().is_some() {
        return Err(USAGE);
    }
    Ok(PathBuf::from(path))
}

fn freshness_summary(freshness: ProjectionFreshness) -> Value {
    match freshness {
        ProjectionFreshness::Fresh { age_seconds } => json!({
            "status": "fresh",
            "age_seconds": age_seconds,
        }),
        ProjectionFreshness::Stale {
            age_seconds,
            limit_seconds,
        } => json!({
            "status": "stale",
            "age_seconds": age_seconds,
            "limit_seconds": limit_seconds,
        }),
    }
}

fn load_failure(state: &MissionProjectionLoad) -> String {
    match state {
        MissionProjectionLoad::Loading { path } => path_failure("still loading", path, None),
        MissionProjectionLoad::Missing { path } => path_failure("missing", path, None),
        MissionProjectionLoad::Unavailable { path, message } => {
            path_failure("unavailable", path, Some(message))
        }
        MissionProjectionLoad::Malformed { path, message } => {
            path_failure("malformed", path, Some(message))
        }
        MissionProjectionLoad::Unsupported { path, found } => format!(
            "mission projection schema is unsupported at {}: {found}",
            path.display()
        ),
        MissionProjectionLoad::Ready { .. } => {
            "mission projection unexpectedly reached a failure branch".to_string()
        }
    }
}

fn path_failure(status: &str, path: &std::path::Path, detail: Option<&str>) -> String {
    let summary = format!("mission projection is {status} at {}", path.display());
    match detail {
        Some(detail) => format!("{summary}: {detail}"),
        None => summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_path_requires_exactly_one_non_empty_argument() {
        assert_eq!(snapshot_path(Vec::<OsString>::new()), Err(USAGE));
        assert_eq!(snapshot_path([OsString::new()]), Err(USAGE));
        assert_eq!(
            snapshot_path([OsString::from("one.json"), OsString::from("two.json")]),
            Err(USAGE)
        );
        assert_eq!(
            snapshot_path([OsString::from("one.json")]),
            Ok(PathBuf::from("one.json"))
        );
    }
}
