//! Narrow, projection-verified repairs to selected migration bytes only.
use super::{parse::parse_task_text, BacklogTaskSource};
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MigrationRepair {
    pub reason: &'static str,
    pub source_digest: String,
    pub target_digest: String,
}

/// Repair only the historical writer's fused first closing fence and heading.
/// No YAML, heading text, or other source bytes are rewritten.
pub(crate) fn repair_selected(
    kind: &str,
    locator: &str,
    bytes: &[u8],
) -> Result<Option<(Vec<u8>, MigrationRepair)>> {
    if kind != "task" {
        return Ok(None);
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Ok(None);
    };
    let Some(rest) = text.strip_prefix("---\n") else {
        return Ok(None);
    };
    let Some(fence) = rest.find("\n---") else {
        return Ok(None);
    };
    let boundary = 4 + fence + 4;
    if !text[boundary..].starts_with("## ") {
        return Ok(None);
    }
    let mut repaired = text.to_owned();
    repaired.insert(boundary, '\n');
    let source = if locator.starts_with("backlog/completed/") {
        BacklogTaskSource::Completed
    } else if locator.starts_with("backlog/drafts/") {
        BacklogTaskSource::Draft
    } else if locator.starts_with("backlog/archive/tasks/") {
        BacklogTaskSource::Archived
    } else {
        BacklogTaskSource::Active
    };
    let before = parse_task_text(Path::new(locator), source, text)?;
    let after = parse_task_text(Path::new(locator), source, &repaired)?;
    ensure!(
        before == after,
        "fused fence repair changes native task projection: {locator}"
    );
    let repair = MigrationRepair {
        reason: "insert newline between fused closing frontmatter fence and section heading; native task projection unchanged",
        source_digest: format!("{:x}", Sha256::digest(bytes)),
        target_digest: format!("{:x}", Sha256::digest(repaired.as_bytes())),
    };
    Ok(Some((repaired.into_bytes(), repair)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_capture_accepts_only_the_verified_repair_and_rechecks_original() {
        let fixture = tempfile::tempdir().expect("fixture");
        let path = fixture.path().join("backlog/tasks/task-1.md");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("tasks");
        let source = b"---\nid: TASK-1\ntitle: Example\nstatus: To Do\n---## Description\nExact.\n";
        std::fs::write(&path, source).expect("source");
        let mut inventory =
            crate::backlog::migration::inventory_kind(fixture.path(), "task").expect("inventory");
        inventory.validate_native().expect("repaired native format");
        let repo = crate::storage::RepositoryId(uuid::Uuid::new_v4().to_string());
        inventory
            .capture_plan(repo.clone())
            .expect("verified repair");
        inventory.documents[0].bytes.push(b'!');
        assert!(inventory.capture_plan(repo.clone()).is_err());
        inventory.documents[0].bytes.pop();
        std::fs::write(&path, b"changed").expect("source change");
        assert!(inventory.capture_plan(repo).is_err());
    }

    #[test]
    fn only_exact_first_closing_fence_is_repaired_without_projection_change() {
        let source = b"---\nid: TASK-1\ntitle: Example\nstatus: To Do\ncustom: {nested: [null, one]}\n---## Description\n\nKeep exact body.\n\n## Acceptance Criteria\n- [ ] #1 Keep\n";
        let (target, proof) = repair_selected("task", "backlog/tasks/task-1.md", source)
            .expect("verified")
            .expect("repair");
        assert_eq!(
            target,
            String::from_utf8_lossy(source)
                .replacen("---##", "---\n##", 1)
                .as_bytes()
        );
        assert_ne!(proof.source_digest, proof.target_digest);
        assert!(repair_selected("task", "backlog/tasks/task-1.md", &target)
            .expect("valid")
            .is_none());
        assert!(repair_selected("project", "backlog/projects/a.md", source)
            .expect("other kind")
            .is_none());
        for text in [
            "---\nid: TASK-1\n---bad\n---## Description\n",
            "---\nid: TASK-1\n---\nBody\n---## Description\n",
            "---\nid: TASK-1\n---# Heading\n",
        ] {
            assert!(
                repair_selected("task", "backlog/tasks/task-1.md", text.as_bytes())
                    .expect("unmatched")
                    .is_none()
            );
        }
    }
}
