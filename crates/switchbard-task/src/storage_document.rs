//! Lossless custom-content editing without changing the SQL schema or typed projections.
use anyhow::{ensure, Context, Result};
use clap::Args;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use switchbard_core::storage::{ExchangeContent, Store};

#[derive(Args)]
pub struct DocumentArgs {
    /// Stable record UUID (listed by storage export).
    #[arg(long)]
    id: String,
    /// Replace the full raw content from this file; omit to print current bytes.
    #[arg(long, requires = "expected_revision")]
    write_from: Option<PathBuf>,
    /// Exact local revision being edited, preventing stale document replacement.
    #[arg(long, requires = "write_from")]
    expected_revision: Option<u64>,
}

pub fn run(root: &Path, database: &Path, args: &DocumentArgs) -> Result<()> {
    let mut store = Store::open_existing(database)?.context("no central database")?;
    let repo = store.repository(root)?.context("unregistered repository")?;
    let sequence = store.change_sequence()?;
    let snapshot = store.export_snapshot(&repo)?;
    let record = snapshot
        .records
        .iter()
        .find(|r| r.id == args.id)
        .context("unknown record UUID in this repository")?;
    let current = store
        .read(&repo, &record.kind, &record.locator)?
        .context("missing record")?;
    let Some(path) = &args.write_from else {
        eprintln!("revision: {}", current.revision);
        std::io::stdout().write_all(&current.content)?;
        return Ok(());
    };
    current.ensure_understood()?;
    let mut content = Vec::new();
    std::fs::File::open(path)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut content)?;
    ensure!(
        content.len() <= 4 * 1024 * 1024,
        "document exceeds size limit"
    );
    let kinds: Vec<_> = snapshot.kinds.iter().map(String::as_str).collect();
    store.mutate_kinds(&repo, &kinds, Some(sequence), |documents, _| {
        let document = documents
            .iter_mut()
            .find(|d| d.id == args.id)
            .context("record disappeared")?;
        ensure!(
            Some(document.revision) == args.expected_revision,
            "stale document revision; reload and retry"
        );
        ensure!(
            !document.deleted,
            "record is deleted; resolve the tombstone explicitly"
        );
        document.content = content;
        let mut candidate = snapshot.clone();
        for record in &mut candidate.records {
            let doc = documents
                .iter()
                .find(|d| d.id == record.id)
                .context("candidate record missing")?;
            record.content = ExchangeContent::from_bytes(&doc.content);
        }
        candidate.refresh_digest()?;
        switchbard_core::backlog::validate_storage_snapshot(&candidate)
    })?;
    println!("Updated {}", args.id);
    Ok(())
}
