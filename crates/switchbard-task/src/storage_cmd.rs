//! Explicit, preview-first per-kind migration. Ordinary commands never export or migrate.
use anyhow::{ensure, Context, Result};
use clap::{Args, Subcommand};
use serde::Serialize;
use std::path::{Path, PathBuf};
use switchbard_core::backlog::migration::{
    content_digest, inventory_kind, inventory_kind_with_resolutions, KindInventory,
    MigrationResolutionFile,
};
use switchbard_core::storage::{default_database_path, Store};

#[derive(Args)]
pub struct StorageArgs {
    /// Alternate database for isolated migration/verification (normal commands use SWITCHBARD_DATABASE).
    #[arg(long)]
    database: Option<PathBuf>,
    #[command(subcommand)]
    command: StorageCommand,
}

#[derive(Subcommand)]
enum StorageCommand {
    /// Read, migrate, or replace the machine-local cross-repository ordering.
    Ordering(crate::storage_ordering::OrderingArgs),
    /// Create a consistent private backup of the whole database.
    Backup(crate::storage_recovery::BackupArgs),
    /// Restore a backup into a NEW database path and rotate its replica identity.
    Restore(crate::storage_recovery::RestoreArgs),
    /// Explicitly attach a moved repository to its stable database identity.
    Rebind(crate::storage_recovery::RebindArgs),
    /// Read or replace complete custom document content with revision checks.
    Document(crate::storage_document::DocumentArgs),
    /// Write the optional readable repository exchange file.
    Export(crate::storage_exchange::ExportArgs),
    /// Preview and apply a causal repository snapshot.
    Import(crate::storage_exchange::ImportArgs),
    /// Show per-kind authority and record counts. Never creates a database.
    Status,
    /// Preview one kind across linked worktrees; --apply switches authority, preserving originals.
    Migrate {
        #[arg(long, value_parser = ["initiative", "project", "config", "ranking", "goals", "task"])]
        kind: String,
        /// Commit the verified inventory. Without this flag, only print the preview.
        #[arg(long)]
        apply: bool,
        /// Exact digest printed by the reviewed preview; required with --apply.
        #[arg(long, requires = "apply")]
        preview_digest: Option<String>,
        /// Reviewed exhaustive source-selection JSON for otherwise ambiguous records.
        #[arg(long)]
        resolution_file: Option<PathBuf>,
    },
}

#[derive(Serialize)]
struct KindStatus<'a> {
    kind: &'a str,
    authority: &'a str,
    records: usize,
}

pub fn run(root: &Path, args: &StorageArgs) -> Result<()> {
    let database = match &args.database {
        Some(path) => path.clone(),
        None => default_database_path()?,
    };
    match &args.command {
        StorageCommand::Ordering(args) => crate::storage_ordering::run(root, &database, args),
        StorageCommand::Backup(args) => crate::storage_recovery::backup(root, &database, args),
        StorageCommand::Restore(args) => crate::storage_recovery::restore(root, &database, args),
        StorageCommand::Rebind(args) => crate::storage_recovery::rebind(root, &database, args),
        StorageCommand::Document(args) => crate::storage_document::run(root, &database, args),
        StorageCommand::Export(args) => crate::storage_exchange::export(root, &database, args),
        StorageCommand::Import(args) => crate::storage_exchange::import(root, &database, args),
        StorageCommand::Status => status(root, &database),
        StorageCommand::Migrate {
            kind,
            apply,
            preview_digest,
            resolution_file,
        } => migrate(
            root,
            &database,
            kind,
            *apply,
            preview_digest.as_deref(),
            resolution_file.as_deref(),
        ),
    }
}

fn status(root: &Path, database: &Path) -> Result<()> {
    let store = Store::open_existing(database)?;
    let repo = store
        .as_ref()
        .map(|s| s.repository(root))
        .transpose()?
        .flatten();
    let mut rows = Vec::new();
    for kind in [
        "initiative",
        "project",
        "config",
        "ranking",
        "goals",
        "task",
    ] {
        let central = match (&store, &repo) {
            (Some(s), Some(r)) => s.authority(r, kind)?,
            _ => false,
        };
        let records = match (&store, &repo) {
            (Some(s), Some(r)) if central => s.list(r, kind)?.iter().filter(|d| !d.deleted).count(),
            _ => 0,
        };
        rows.push(KindStatus {
            kind,
            authority: if central { "database" } else { "legacy" },
            records,
        });
    }
    let retained_sources = match (&store, &repo) {
        (Some(store), Some(repo)) => store.legacy_source_drift(repo)?,
        _ => Vec::new(),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"database":database,"repo_id":repo,"kinds":rows,"retained_sources":retained_sources})
        )?
    );
    Ok(())
}

fn migrate(
    root: &Path,
    database: &Path,
    kind: &str,
    apply: bool,
    preview_digest: Option<&str>,
    resolution_file: Option<&Path>,
) -> Result<()> {
    let inventory = migration_inventory(root, kind, resolution_file)?;
    inventory.validate_native()?;
    if !apply {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
            "kind": kind, "records": inventory.documents.len(), "worktrees": inventory.worktrees,
            "sources": inventory.documents.iter().flat_map(|document| document.sources.iter().map(|source| serde_json::json!({"path":source.path,"source_digest":source.digest,"selected_locator":document.locator,"selected_content_digest":content_digest(&document.bytes),"stale_proof":source.stale_proof,"addition_proof":source.addition_proof}))).collect::<Vec<_>>(),
            "repairs": inventory.documents.iter().filter_map(|d| d.repair.as_ref().map(|repair| serde_json::json!({"locator":d.locator,"repair":repair}))).collect::<Vec<_>>(),
            "resolution_file_digest": inventory.resolution_file_digest,
            "resolutions": inventory.resolutions,
            "status": "preview", "digest": inventory.digest(), "next_step": "rerun with --apply --preview-digest <digest> after checking the inventory" }))?
        );
        return Ok(());
    }
    ensure!(
        preview_digest == Some(inventory.digest().as_str()),
        "preview missing or changed; rerun storage migrate without --apply and review its digest"
    );
    let mut store = Store::open(database)?;
    let repo = store.bind_repository(root)?;
    for tree in &inventory.worktrees {
        ensure!(
            store.bind_repository(tree)? == repo,
            "worktree binding changed during migration"
        );
    }
    ensure!(
        !store.authority(&repo, kind)?,
        "{kind} already uses the database; use storage status"
    );
    let plan = inventory.capture_plan(repo.clone())?;
    let fresh = migration_inventory(root, kind, resolution_file)?;
    ensure!(
        inventory.digest() == fresh.digest(),
        "migration inventory changed; preview again"
    );
    let recovery = prepare_recovery(&store, &repo, &inventory)?;
    let count = store.apply_migration_checked(&plan, |candidate| {
        ensure!(
            migration_inventory(root, kind, resolution_file)?.digest() == inventory.digest(),
            "source or branch tips changed during recovery preparation; preview again"
        );
        switchbard_core::backlog::validate_storage_snapshot(candidate)
    })?;
    verify_import(&store, &repo, &inventory)?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"kind":kind,"records":count,"status":"central","originals":"preserved","recovery":recovery,"next_step":"verify ordinary reads and edits; continue with the next kind"})
        )?
    );
    Ok(())
}

fn migration_inventory(
    root: &Path,
    kind: &str,
    resolution_file: Option<&Path>,
) -> Result<KindInventory> {
    let resolutions = resolution_file
        .map(|path| MigrationResolutionFile::read(path, kind))
        .transpose()?;
    match resolutions.as_ref() {
        Some(resolutions) => inventory_kind_with_resolutions(root, kind, Some(resolutions)),
        None => inventory_kind(root, kind),
    }
}

fn verify_import(
    store: &Store,
    repo: &switchbard_core::storage::RepositoryId,
    inventory: &KindInventory,
) -> Result<()> {
    let documents = store.list(repo, &inventory.kind)?;
    ensure!(
        documents.len() == inventory.documents.len(),
        "post-migration count mismatch; preserve database and originals for reconciliation"
    );
    for expected in &inventory.documents {
        let actual = store
            .read(repo, &inventory.kind, &expected.locator)?
            .context("post-migration record missing")?;
        ensure!(
            actual.content == expected.bytes && !actual.deleted,
            "post-migration content mismatch at {}",
            expected.locator
        );
    }
    Ok(())
}

pub(super) fn prepare_recovery(
    store: &Store,
    repo: &switchbard_core::storage::RepositoryId,
    inventory: &KindInventory,
) -> Result<PathBuf> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let parent = store.path().parent().context("database parent")?;
    let dir = parent.join(format!("migration-{}-{stamp}", inventory.kind));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&dir)?;
    store.backup_to(&dir.join("before.sqlite3"))?;
    let restored = Store::open(dir.join("before.sqlite3"))?;
    ensure!(
        restored.recovery_fingerprint()? == store.recovery_fingerprint()?,
        "backup restore fingerprint mismatch; migration refused"
    );
    let manifest = serde_json::json!({"kind":inventory.kind,"repo_id":repo,"preview_digest":inventory.digest(),"resolution_file_digest":inventory.resolution_file_digest,"resolutions":inventory.resolutions,"worktrees":inventory.worktrees,"branch_refs":inventory.branch_refs,"sources":inventory.documents.iter().map(|d|serde_json::json!({"locator":d.locator,"selected_content_digest":content_digest(&d.bytes),"original_sources":d.sources,"selected_content_bytes":d.bytes,"repair":d.repair,"resolution":d.resolution})).collect::<Vec<_>>(),"restore_verified":true});
    write_private_new(
        &dir.join("sources.json"),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(dir)
}

fn write_private_new(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
