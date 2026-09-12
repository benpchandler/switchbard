//! Explicit machine-local ordering migration and revision-protected raw YAML editing.
use anyhow::{ensure, Context, Result};
use clap::Args;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use switchbard_core::storage::{
    preview_workspace_ordering, Store, WorkspaceOrdering, MAX_DOCUMENT_BYTES,
};

#[derive(Args)]
pub struct OrderingArgs {
    /// Preview this legacy ordering.yml. Add --apply and its exact digest to migrate.
    #[arg(long, conflicts_with = "write_from")]
    migrate_from: Option<PathBuf>,
    /// Activate the reviewed source, keeping its original bytes untouched.
    #[arg(long, requires = "migrate_from")]
    apply: bool,
    /// Exact digest from the reviewed preview, required with --apply.
    #[arg(long, requires = "apply")]
    source_digest: Option<String>,
    /// Replace central YAML from an edited file. Without a mode, print current YAML.
    #[arg(long, requires = "expected_sequence", conflicts_with = "migrate_from")]
    write_from: Option<PathBuf>,
    /// Sequence printed when reading the central ordering, protecting against stale edits.
    #[arg(long, requires = "write_from")]
    expected_sequence: Option<u64>,
    /// Alternate Switchbard config supplying tracked repository names and paths.
    #[arg(long)]
    config: Option<PathBuf>,
}

pub fn run(_root: &Path, database: &Path, args: &OrderingArgs) -> Result<()> {
    if let Some(source) = &args.migrate_from {
        return migrate(database, args, source);
    }
    let mut store = Store::open_existing(database)?
        .context("no central database; preview a source with --migrate-from")?;
    let Some(path) = &args.write_from else {
        let ordering = store
            .workspace_ordering()?
            .context("workspace ordering has not been migrated; use --migrate-from")?;
        eprintln!("sequence: {}", store.change_sequence()?);
        eprintln!("source: {}", ordering.source_path.display());
        std::io::stdout().write_all(&ordering.content)?;
        return Ok(());
    };
    let content = read_yaml(path)?;
    let expected = args
        .expected_sequence
        .context("--expected-sequence is required")?;
    ensure!(
        store.change_sequence()? == expected,
        "workspace state changed; read the central ordering again before editing"
    );
    let repositories = repositories(args.config.as_deref())?;
    let recovery = backup_before_change(&store)?;
    let result = store.replace_workspace_ordering(&content, &repositories, expected)?;
    ensure!(
        result.content == content,
        "central ordering verification mismatch"
    );
    print_result("updated", &store, &result, &recovery)
}

fn migrate(database: &Path, args: &OrderingArgs, source: &Path) -> Result<()> {
    let (content, digest) = preview_workspace_ordering(source)?;
    let repositories = repositories(args.config.as_deref())?;
    if !args.apply {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "status":"preview", "source":source, "digest":digest,
                "content":std::str::from_utf8(&content)?, "repositories":repositories,
                "next_step":"rerun with --apply --source-digest <digest> after reviewing the source"
            }))?
        );
        return Ok(());
    }
    ensure!(
        args.source_digest.as_deref() == Some(digest.as_str()),
        "source digest missing or changed; preview --migrate-from again before applying"
    );
    let mut store = Store::open(database)?;
    ensure!(store.workspace_ordering()?.is_none(), "workspace ordering already migrated; read it and use --write-from with --expected-sequence");
    let recovery = backup_before_change(&store)?;
    let result = store.migrate_workspace_ordering_checked(source, &repositories, &digest)?;
    ensure!(
        result.content == content,
        "central ordering differs from reviewed source"
    );
    print_result("central", &store, &result, &recovery)
}

fn repositories(config_path: Option<&Path>) -> Result<Vec<(String, PathBuf)>> {
    let path = config_path
        .map(Path::to_path_buf)
        .or_else(switchbard_core::config::default_path);
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let config = match switchbard_core::config::load_from(&path) {
        Ok(config) => config,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && config_path.is_none() => {
            Default::default()
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reading configured repository map {}", path.display()))
        }
    };
    Ok(config
        .repos
        .into_iter()
        .map(|repo| (repo.name, repo.path))
        .collect())
}

fn read_yaml(path: &Path) -> Result<Vec<u8>> {
    let mut content = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_end(&mut content)?;
    ensure!(
        content.len() <= MAX_DOCUMENT_BYTES,
        "ordering document exceeds size limit"
    );
    let text = std::str::from_utf8(&content).context("ordering document is not UTF-8")?;
    let (_, warning) = switchbard_core::OrderingOverlay::parse(text);
    ensure!(warning.is_none(), "{}", warning.unwrap_or_default());
    Ok(content)
}

fn backup_before_change(store: &Store) -> Result<PathBuf> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let directory = store
        .path()
        .parent()
        .context("database parent missing")?
        .join(format!("ordering-recovery-{stamp}"));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory)?;
    let file = directory.join("before.sqlite3");
    let before = store.recovery_fingerprint()?;
    store.backup_to(&file)?;
    let copy = Store::open_existing(&file)?.context("ordering recovery backup disappeared")?;
    ensure!(
        copy.recovery_fingerprint()? == before && store.recovery_fingerprint()? == before,
        "database changed while verifying ordering backup; retry during a quiet moment"
    );
    Ok(file)
}

fn print_result(
    status: &str,
    store: &Store,
    ordering: &WorkspaceOrdering,
    recovery: &Path,
) -> Result<()> {
    let resolved = ordering
        .entries
        .iter()
        .filter(|entry| entry.target.is_some())
        .count();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "status":status, "sequence":store.change_sequence()?, "entries":ordering.entries.len(),
            "resolved":resolved, "unresolved":ordering.entries.len()-resolved,
            "originals":"preserved", "recovery":recovery,
            "scope":"machine-local workspace ordering; excluded from repository exchange"
        }))?
    );
    Ok(())
}
