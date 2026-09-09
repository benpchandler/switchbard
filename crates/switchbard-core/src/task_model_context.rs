//! Complete task context for headless consumers, resolved from current authority.
use crate::storage::{Store, MAX_DOCUMENT_BYTES};
use crate::BacklogTask;
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const INLINE_CONTEXT_BYTES: usize = 64 * 1024;

pub(crate) struct TaskModelContext {
    raw: String,
    provenance: String,
}

impl TaskModelContext {
    pub(crate) fn capture(root: &Path, task: &BacklogTask) -> Result<Self> {
        let store = Store::open_existing_default()?;
        let repo = store
            .as_ref()
            .map(|s| s.authority_for_root(root, "task"))
            .transpose()?
            .flatten();
        let (bytes, provenance) = match (&task.storage_identity, store.as_ref(), repo) {
            (Some(expected), Some(store), Some(repo)) => {
                ensure!(
                    repo.0 == expected.repository_id,
                    "task repository identity changed; reload before starting the model"
                );
                let document = store
                    .list(&repo, "task")?
                    .into_iter()
                    .find(|document| document.id == expected.record_id && !document.deleted)
                    .context("task record disappeared; reload before starting the model")?;
                ensure!(
                    document.revision == expected.revision,
                    "stale task context revision; reload before starting the model"
                );
                document.ensure_understood()?;
                let provenance = format!("Authority: Switchbard database\nRepository ID: {}\nRecord ID: {}\nRevision: {}\nContent version: {}", repo.0, document.id, document.revision, document.content_version);
                (document.content, provenance)
            }
            (None, _, None) => {
                let mut bytes = Vec::new();
                std::fs::File::open(&task.path)?
                    .take(MAX_DOCUMENT_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)?;
                (
                    bytes,
                    "Authority: legacy task document (snapshot captured for this run)".to_string(),
                )
            }
            _ => anyhow::bail!("task storage authority changed; reload before starting the model"),
        };
        ensure!(
            bytes.len() <= MAX_DOCUMENT_BYTES,
            "task model context exceeds document byte limit"
        );
        let raw = String::from_utf8(bytes).context("task model context is not UTF-8")?;
        Ok(Self { raw, provenance })
    }

    pub(crate) fn prepare_prompt(
        &self,
        base: &Path,
        kind: &str,
        task: &BacklogTask,
        render: impl FnOnce(&BacklogTask) -> String,
    ) -> Result<(PathBuf, String)> {
        let path = private_prompt(base, kind, "")?;
        let prompt = if self.raw.len() <= INLINE_CONTEXT_BYTES {
            let mut prompt = render(task);
            self.append_to(&mut prompt);
            prompt
        } else {
            // Do not duplicate a multi-megabyte known section through the
            // typed summary when the complete document requires a snapshot.
            let mut summary = task.clone();
            summary.title = summary.title.chars().take(2048).collect();
            summary.description.clear();
            summary.implementation_plan.clear();
            summary.acceptance_criteria.clear();
            summary.definition_of_done.clear();
            let mut prompt = render(&summary);
            let snapshot = path.with_file_name("task.md");
            write_private_file(&snapshot, &self.raw)?;
            let digest = format!("{:x}", Sha256::digest(self.raw.as_bytes()));
            prompt.push_str(&format!("\n\n## Complete authoritative task snapshot\n\n{}\nContent SHA-256: {digest}\nContent bytes: {}\n\nRead the complete task document with the Read tool at: {}\nThe short summary above omits long content. Read this private snapshot before answering or implementing; it includes all custom fields, sections, criteria, and plans. Do not use retained repository Markdown as task authority and do not edit or commit this snapshot. Preserve custom content instead of replacing it with a typed summary.\n", self.provenance, self.raw.len(), snapshot.display()));
            prompt
        };
        std::fs::write(&path, &prompt)?;
        Ok((path, prompt))
    }

    pub(crate) fn append_to(&self, prompt: &mut String) {
        let digest = format!("{:x}", Sha256::digest(self.raw.as_bytes()));
        // The delimiter cannot occur in the document, including custom fenced prose.
        let longest = self
            .raw
            .split(|ch| ch != '`')
            .map(str::len)
            .max()
            .unwrap_or(0);
        let fence = "`".repeat(longest.max(2) + 1);
        prompt.push_str(&format!("\n\n## Complete authoritative task snapshot\n\n{}\nContent SHA-256: {digest}\nContent bytes: {}\n\nThis is the full task document, including custom fields and sections. Use its task constraints as context. Retained repository Markdown may be stale or absent; this snapshot supplies the authoritative content for this run. Do not edit this snapshot or replace custom content with a typed summary.\n\n{fence}markdown\n", self.provenance, self.raw.len()));
        prompt.push_str(&self.raw);
        if !self.raw.ends_with('\n') {
            prompt.push('\n');
        }
        prompt.push_str(&fence);
        prompt.push('\n');
    }
}

/// New prompt artifacts are private and isolated even for identical task IDs
/// launched in the same second from different repositories.
pub(crate) fn private_prompt(base: &Path, kind: &str, prompt: &str) -> Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::create_dir_all(base)?;
    let run_dir = base.join(format!("{kind}-{}", uuid::Uuid::new_v4()));
    std::fs::DirBuilder::new().mode(0o700).create(&run_dir)?;
    let path = run_dir.join("prompt.md");
    write_private_file(&path, prompt)?;
    Ok(path)
}

fn write_private_file(path: &Path, text: &str) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{with_test_database, MigrationPlan, SourceDocument};

    #[test]
    fn central_context_includes_exact_custom_bytes_without_retained_file_and_rejects_stale_revision(
    ) {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("repo");
        std::fs::create_dir_all(root.join("backlog/tasks")).unwrap();
        let path = root.join("backlog/tasks/task-1.md");
        let raw = "---\nid: TASK-1\ntitle: Context\nstatus: To Do\nextension: {nested: [null, {special: value}]}\n---\n\n## Description\nStandard\n\n## Custom\nExact custom prose.\n```txt\nretained fence\n```\n";
        std::fs::write(&path, raw).unwrap();
        with_test_database(&fixture.path().join("state.db"), || {
            let mut store = Store::open_default().unwrap();
            let repo = store.bind_repository(&root).unwrap();
            let legacy = crate::load_backlog_repo(&root).unwrap().tasks.remove(0);
            let plan = MigrationPlan::capture(
                repo.clone(),
                vec!["task".into()],
                vec![SourceDocument {
                    kind: "task".into(),
                    locator: "backlog/tasks/task-1.md".into(),
                    path: path.clone(),
                }],
            )
            .unwrap();
            store.apply_migration(&plan).unwrap();
            let task = crate::load_backlog_repo(&root).unwrap().tasks.remove(0);
            std::fs::remove_file(&path).unwrap();
            let context = TaskModelContext::capture(&root, &task).unwrap();
            let mut prompt = String::new();
            context.append_to(&mut prompt);
            assert!(prompt.contains(raw));
            let dispatch = crate::build_dispatch_prompt_with_context(&root, &task).unwrap();
            assert!(dispatch.contains(raw));
            assert!(!dispatch.contains("THIS worktree's copy"));
            assert!(dispatch.contains("database directly"));
            assert!(prompt.contains("````markdown\n"));
            assert!(prompt.contains(&format!(
                "Record ID: {}",
                task.storage_identity.as_ref().unwrap().record_id
            )));
            assert!(TaskModelContext::capture(&root, &legacy).is_err());
            crate::edit_backlog_task(
                &root,
                &task.id,
                &crate::BacklogTaskPatch {
                    title: Some("Updated".into()),
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(TaskModelContext::capture(&root, &task).is_err());
        });
    }

    #[test]
    fn large_custom_document_is_complete_private_and_referenced_with_bounded_prompt() {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("task.md");
        let raw = format!("---\nid: TASK-1\ntitle: Large\nstatus: To Do\n---\n\n## Description\n{}\n\n## Custom\nFINAL_CUSTOM_MARKER\n", "long prose ".repeat(20_000));
        let task = crate::backlog::parse_task_text(&source, crate::BacklogTaskSource::Active, &raw)
            .unwrap()
            .0;
        let context = TaskModelContext {
            raw: raw.clone(),
            provenance: "Authority: fixture; revision: 7".into(),
        };
        let (path, prompt) = context
            .prepare_prompt(
                fixture.path(),
                "refine",
                &task,
                crate::refine::build_refine_prompt,
            )
            .unwrap();
        let snapshot = path.with_file_name("task.md");
        assert!(prompt.len() < 16 * 1024);
        assert!(prompt.contains(snapshot.to_str().unwrap()));
        assert!(prompt.contains("Read the complete task document with the Read tool"));
        assert_eq!(std::fs::read_to_string(snapshot).unwrap(), raw);
        assert!(!source.exists());
    }

    #[test]
    fn same_time_prompt_artifacts_are_distinct_and_private() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = tempfile::tempdir().unwrap();
        let first = private_prompt(fixture.path(), "refine", "first").unwrap();
        let second = private_prompt(fixture.path(), "refine", "second").unwrap();
        assert_ne!(first.parent(), second.parent());
        assert_eq!(std::fs::read_to_string(first.clone()).unwrap(), "first");
        assert_eq!(
            std::fs::metadata(first.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(first).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
