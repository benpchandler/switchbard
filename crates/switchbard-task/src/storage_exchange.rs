//! Explicit repository exchange. Preview tokens bind both input and database revision.
use anyhow::{bail, ensure, Context, Result};
use clap::Args;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use switchbard_core::storage::{ConflictResolution, ExchangeContent, ExchangeSnapshot, Store};

#[derive(Args)]
pub struct ExportArgs {
    /// Defaults to .switchbard/tasks.json in this repository.
    #[arg(long)]
    file: Option<PathBuf>,
    /// Explicitly replace independently changed exchange content after review.
    #[arg(long)]
    replace: bool,
}

#[derive(Args)]
pub struct ImportArgs {
    #[arg(long)]
    file: Option<PathBuf>,
    /// Explicitly bind this previously unbound local checkout to the file's repository.
    #[arg(long)]
    bind: bool,
    #[arg(long)]
    apply: bool,
    /// Digest printed by the preview; binds the exact incoming content.
    #[arg(long, requires = "apply")]
    snapshot_digest: Option<String>,
    /// Database sequence printed by preview; rejects stale local state.
    #[arg(long, requires = "apply")]
    expected_sequence: Option<u64>,
    /// Resolve one concurrent record as RECORD_UUID=local or RECORD_UUID=incoming.
    #[arg(long, requires = "apply")]
    resolve: Vec<String>,
    /// Reviewed JSON resolutions with version, incoming_digest, expected_sequence and resolutions.
    /// Choices: local, incoming, custom (content/deleted), relocate-incoming (locator/content).
    #[arg(long, requires = "apply")]
    resolution_file: Option<PathBuf>,
}

fn file_path(root: &Path, selected: &Option<PathBuf>) -> PathBuf {
    selected
        .clone()
        .unwrap_or_else(|| root.join(".switchbard/tasks.json"))
}

pub fn export(root: &Path, database: &Path, args: &ExportArgs) -> Result<()> {
    let store =
        Store::open_existing(database)?.context("no central database; migrate a kind first")?;
    let repo = store
        .repository(root)?
        .context("repository is not centrally registered")?;
    let path = file_path(root, &args.file);
    let _lock = crate::storage_exchange_file::ExportLock::acquire(&path)?;
    let snapshot = store.export_snapshot(&repo)?;
    let bytes = snapshot.to_bytes()?;
    let existing = crate::storage_exchange_file::existing_bytes(&path)?;
    if existing.as_deref() == Some(bytes.as_slice()) {
        println!("no changes");
        return Ok(());
    }
    if let Some(ref previous) = existing {
        if !args.replace {
            let current = ExchangeSnapshot::parse(previous)
                .context("existing exchange is invalid; reconcile it or explicitly --replace")?;
            let preview = store.preview_import_checked(
                &repo,
                &current,
                switchbard_core::backlog::validate_storage_snapshot,
            )?;
            ensure!(preview.conflicts.is_empty() && preview.validation_errors.is_empty() && preview.changed_records == 0,
                "exchange file has independent changes; import/reconcile it first or explicitly --replace");
        }
    }
    crate::storage_exchange_file::replace(&path, existing.as_deref(), &bytes)?;
    println!("{}", path.display());
    Ok(())
}

pub fn import(root: &Path, database: &Path, args: &ImportArgs) -> Result<()> {
    let incoming = read_snapshot(&file_path(root, &args.file))?;
    switchbard_core::backlog::validate_storage_snapshot(&incoming)?;
    let existing = Store::open_existing(database)?;
    let sequence = existing
        .as_ref()
        .map(Store::change_sequence)
        .transpose()?
        .unwrap_or(0);
    let repo = existing
        .as_ref()
        .map(|s| s.repository(root))
        .transpose()?
        .flatten();
    let mut legacy_kinds = Vec::new();
    for kind in &incoming.kinds {
        let active = match (&existing, &repo) {
            (Some(store), Some(repo)) => store.authority(repo, kind)?,
            _ => false,
        };
        if !active {
            legacy_kinds.push(kind.clone());
        }
    }
    verify_legacy_compatibility(root, &incoming, &legacy_kinds)?;
    if let Some(ref repo) = repo {
        let preview = existing
            .as_ref()
            .expect("bound store")
            .preview_import_checked(
                repo,
                &incoming,
                switchbard_core::backlog::validate_storage_snapshot,
            )?;
        if !args.apply {
            println!("{}", serde_json::to_string_pretty(&preview)?);
            return Ok(());
        }
    } else {
        ensure!(
            args.bind,
            "repository is unbound; preview with --bind to explicitly join this exchange scope"
        );

        if !args.apply {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"incoming_digest":incoming.digest,"expected_sequence":sequence,"records":incoming.records.len(),"action":"bind and import","next_step":"apply with this digest and sequence"})
                )?
            );
            return Ok(());
        }
    }
    ensure!(
        args.snapshot_digest.as_deref() == Some(incoming.digest.as_str()),
        "incoming snapshot changed or digest missing; preview again"
    );
    ensure!(
        args.expected_sequence == Some(sequence),
        "local state changed or sequence missing; preview again"
    );
    let resolutions = resolutions(
        &args.resolve,
        args.resolution_file.as_deref(),
        &incoming.digest,
        sequence,
    )?;
    let mut store = match existing {
        Some(store) => store,
        None => Store::open(database)?,
    };
    for kind in &legacy_kinds {
        if native_kind(kind) {
            let inventory = switchbard_core::backlog::migration::inventory_kind(root, kind)?;
            crate::storage_cmd::prepare_recovery(&store, &incoming.repo_id, &inventory)?;
        }
    }
    let validate = |candidate: &ExchangeSnapshot| {
        verify_legacy_compatibility(root, &incoming, &legacy_kinds)?;
        switchbard_core::backlog::validate_storage_snapshot(candidate)
    };
    if let Some(repo) = repo {
        let result =
            store.apply_import_checked(&repo, &incoming, sequence, &resolutions, validate)?;
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        ensure!(
            resolutions.is_empty(),
            "bootstrap cannot resolve concurrent records"
        );
        store.bind_exchange_checked_at(root, &incoming, sequence, validate)?;
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"status":"bound and imported","repo_id":incoming.repo_id,"records":incoming.records.len()})
            )?
        );
    }
    Ok(())
}

fn native_kind(kind: &str) -> bool {
    [
        "initiative",
        "project",
        "config",
        "goals",
        "ranking",
        "task",
    ]
    .contains(&kind)
}

fn verify_legacy_compatibility(
    root: &Path,
    snapshot: &ExchangeSnapshot,
    kinds: &[String],
) -> Result<()> {
    for kind in kinds {
        if !native_kind(kind) {
            continue;
        }
        let inventory = switchbard_core::backlog::migration::inventory_kind(root, kind)?;
        for legacy in inventory.documents {
            let incoming = snapshot
                .records
                .iter()
                .find(|r| r.kind == *kind && r.locator == legacy.locator && !r.deleted)
                .with_context(|| {
                    format!(
                        "legacy record {} is absent from exchange; reconcile before binding",
                        legacy.locator
                    )
                })?;
            ensure!(
                incoming.content.bytes()? == legacy.bytes,
                "legacy record {} differs from exchange; reconcile before binding",
                legacy.locator
            );
        }
    }
    Ok(())
}

fn resolutions(
    values: &[String],
    file: Option<&Path>,
    digest: &str,
    sequence: u64,
) -> Result<BTreeMap<String, ConflictResolution>> {
    ensure!(values.len() <= 100_000, "too many resolutions");
    let mut result = BTreeMap::new();
    for value in values {
        let (id, side) = value
            .split_once('=')
            .context("resolution must be UUID=local or UUID=incoming")?;
        let choice = match side {
            "local" => ConflictResolution::Local,
            "incoming" => ConflictResolution::Incoming,
            _ => bail!("resolution must choose local or incoming"),
        };
        ensure!(
            result.insert(id.to_string(), choice).is_none(),
            "duplicate resolution for {id}"
        );
    }
    if let Some(path) = file {
        let document = read_resolutions(path)?;
        ensure!(document.version == 1, "unsupported resolution file version");
        ensure!(
            document.incoming_digest == digest && document.expected_sequence == sequence,
            "resolution file belongs to a different or stale preview; review again"
        );
        ensure!(
            document.resolutions.len() <= 100_000,
            "too many resolutions"
        );
        for resolution in document.resolutions {
            let (id, choice) = resolution.into_choice()?;
            ensure!(
                result.insert(id.clone(), choice).is_none(),
                "duplicate resolution for {id}"
            );
        }
    }
    ensure!(result.len() <= 100_000, "too many combined resolutions");
    Ok(result)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolutionFile {
    version: u32,
    incoming_digest: String,
    expected_sequence: u64,
    resolutions: Vec<ResolutionEntry>,
}

#[derive(serde::Deserialize)]
#[serde(tag = "choice", rename_all = "kebab-case", deny_unknown_fields)]
enum ResolutionEntry {
    Local {
        record_id: String,
    },
    Incoming {
        record_id: String,
    },
    Custom {
        record_id: String,
        content: ExchangeContent,
        deleted: bool,
    },
    RelocateIncoming {
        record_id: String,
        locator: String,
        content: ExchangeContent,
    },
}

impl ResolutionEntry {
    fn into_choice(self) -> Result<(String, ConflictResolution)> {
        Ok(match self {
            Self::Local { record_id } => (record_id, ConflictResolution::Local),
            Self::Incoming { record_id } => (record_id, ConflictResolution::Incoming),
            Self::Custom {
                record_id,
                content,
                deleted,
            } => (
                record_id,
                ConflictResolution::Custom {
                    content: content.bytes()?,
                    deleted,
                },
            ),
            Self::RelocateIncoming {
                record_id,
                locator,
                content,
            } => (
                record_id,
                ConflictResolution::RelocateIncoming {
                    locator,
                    content: content.bytes()?,
                },
            ),
        })
    }
}

fn read_resolutions(path: &Path) -> Result<ResolutionFile> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 64 * 1024 * 1024,
        "resolution file exceeds 64 MiB"
    );
    serde_json::from_slice(&bytes).context("invalid resolution file")
}

fn read_snapshot(path: &Path) -> Result<ExchangeSnapshot> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ExchangeSnapshot::parse(&bytes)
}
