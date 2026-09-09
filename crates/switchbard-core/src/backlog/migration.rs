//! Read-only inventory for per-kind strangler cutover. Original files stay in place.
use crate::git_env::git_cmd;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;

const MAX_WORKTREES: usize = 256;
const MAX_DOCUMENTS: usize = 100_000;
const MAX_DOCUMENT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_BRANCHES: usize = 256;
const MAX_GIT_LIST_BYTES: usize = 16 * 1024 * 1024;
const MAX_RESOLUTION_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationResolutionFile {
    pub version: u8,
    pub kind: String,
    pub resolutions: Vec<MigrationResolution>,
    #[serde(skip)]
    digest: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationResolution {
    pub source_locators: Vec<String>,
    pub target_locator: String,
    pub sources: Vec<ResolutionSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_task_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionSource {
    pub path: PathBuf,
    pub digest: String,
    pub decision: ResolutionDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionDecision {
    Select,
    Reject,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AppliedResolution {
    pub source_locators: Vec<String>,
    pub target_locator: String,
    pub selected_source: PathBuf,
    pub selected_source_digest: String,
    pub rejected_sources: Vec<ResolutionSource>,
    pub target_digest: String,
    pub repair_task_id: Option<String>,
}

impl MigrationResolutionFile {
    pub fn read(path: &Path, expected_kind: &str) -> Result<Self> {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .with_context(|| format!("reading migration resolution file {}", path.display()))?
            .take(MAX_RESOLUTION_BYTES + 1)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_RESOLUTION_BYTES,
            "migration resolution file exceeds byte limit"
        );
        let digest = sha256(&bytes);
        let mut plan: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing migration resolution file {}", path.display()))?;
        anyhow::ensure!(
            plan.version == 1,
            "unsupported migration resolution version"
        );
        anyhow::ensure!(
            plan.kind == expected_kind,
            "migration resolution kind mismatch"
        );
        anyhow::ensure!(
            !plan.resolutions.is_empty() && plan.resolutions.len() <= MAX_DOCUMENTS,
            "invalid migration resolution count"
        );
        plan.digest = digest;
        Ok(plan)
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[derive(Default)]
struct InventoryBudget {
    bytes: usize,
    sources: usize,
}

impl InventoryBudget {
    fn account(&mut self, bytes: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .context("migration byte count overflow")?;
        self.sources += 1;
        if self.bytes > MAX_TOTAL_BYTES || self.sources > MAX_DOCUMENTS {
            bail!("migration inventory exceeds bounded total bytes or source count");
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct InventoryDocument {
    pub locator: String,
    /// Selected authoritative bytes, independently validated as native content.
    pub bytes: Vec<u8>,
    /// Every original source, including stale variants, retained byte-for-byte.
    pub sources: Vec<InventorySource>,
    pub repair: Option<super::migration_repairs::MigrationRepair>,
    pub resolution: Option<AppliedResolution>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InventorySource {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub digest: String,
    pub stale_proof: Option<StaleSourceProof>,
    pub addition_proof: Option<NewSourceProof>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StaleSourceProof {
    pub primary_head: String,
    pub secondary_head: String,
    pub merge_base: String,
    pub reason: &'static str,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NewSourceProof {
    pub primary_head: String,
    pub secondary_head: String,
    pub merge_base: String,
    pub reason: &'static str,
}

#[derive(Debug)]
pub struct KindInventory {
    pub kind: String,
    pub documents: Vec<InventoryDocument>,
    pub worktrees: Vec<PathBuf>,
    /// Sorted local ref/tip pairs, including current and primary HEAD anchors.
    pub branch_refs: Vec<(String, String)>,
    pub resolution_file_digest: Option<String>,
    pub resolutions: Vec<AppliedResolution>,
}

impl KindInventory {
    /// Exact inventory token, including branch movements and preserved source bytes.
    /// Reject invalid native content before creating any live database or binding.
    pub fn validate_native(&self) -> Result<()> {
        use crate::storage::{ExchangeContent, ExchangeRecord, ExchangeSnapshot, RepositoryId};
        let namespace = uuid::Uuid::nil().to_string();
        let mut snapshot = ExchangeSnapshot {
            version: 2,
            repo_id: RepositoryId(namespace.clone()),
            epoch_id: namespace.clone(),
            kinds: vec![self.kind.clone()],
            digest: String::new(),
            records: self
                .documents
                .iter()
                .enumerate()
                .map(|(index, document)| ExchangeRecord {
                    id: uuid::Uuid::from_u128(index as u128 + 1).to_string(),
                    kind: self.kind.clone(),
                    content_version: 1,
                    locator: document.locator.clone(),
                    content: ExchangeContent::from_bytes(&document.bytes),
                    deleted: false,
                    clock: BTreeMap::from([(namespace.clone(), 1)]),
                })
                .collect(),
        };
        snapshot.refresh_digest()?;
        super::validate_storage_snapshot(&snapshot)
    }

    pub fn capture_plan(
        &self,
        repo: crate::storage::RepositoryId,
    ) -> Result<crate::storage::MigrationPlan> {
        use crate::storage::{AllowedTransform, MigrationPlan, SelectedDocument, SourceDocument};
        let sources = self
            .documents
            .iter()
            .flat_map(|document| {
                document.sources.iter().map(|source| {
                    (
                        SourceDocument {
                            kind: self.kind.clone(),
                            locator: document.locator.clone(),
                            path: source.path.clone(),
                        },
                        source.bytes.clone(),
                    )
                })
            })
            .collect();
        let selected = self
            .documents
            .iter()
            .map(|document| SelectedDocument {
                kind: self.kind.clone(),
                locator: document.locator.clone(),
                bytes: document.bytes.clone(),
                allowed_transform: document.resolution.as_ref().and_then(|resolution| {
                    resolution
                        .repair_task_id
                        .as_ref()
                        .map(|task_id| AllowedTransform {
                            task_id: task_id.clone(),
                            source_digest: resolution.selected_source_digest.clone(),
                        })
                }),
            })
            .collect();
        MigrationPlan::capture_selected(repo, vec![self.kind.clone()], sources, selected)
            .map(|plan| plan.with_lock_root(self.worktrees.first().cloned().unwrap_or_default()))
    }

    pub fn digest(&self) -> String {
        fn field(hash: &mut Sha256, value: &[u8]) {
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value);
        }
        let mut hash = Sha256::new();
        field(&mut hash, b"switchbard-kind-inventory-v2");
        field(&mut hash, self.kind.as_bytes());
        field(&mut hash, &(self.worktrees.len() as u64).to_be_bytes());
        for path in &self.worktrees {
            field(&mut hash, path.as_os_str().as_encoded_bytes());
        }
        field(&mut hash, &(self.branch_refs.len() as u64).to_be_bytes());
        for (name, oid) in &self.branch_refs {
            field(&mut hash, name.as_bytes());
            field(&mut hash, oid.as_bytes());
        }
        field(&mut hash, &(self.documents.len() as u64).to_be_bytes());
        for document in &self.documents {
            field(&mut hash, document.locator.as_bytes());
            field(&mut hash, &document.bytes);
            field(&mut hash, &(document.sources.len() as u64).to_be_bytes());
            for source in &document.sources {
                field(&mut hash, source.path.as_os_str().as_encoded_bytes());
                field(&mut hash, &source.bytes);
                if let Some(proof) = &source.stale_proof {
                    field(&mut hash, proof.primary_head.as_bytes());
                    field(&mut hash, proof.secondary_head.as_bytes());
                    field(&mut hash, proof.merge_base.as_bytes());
                    field(&mut hash, proof.reason.as_bytes());
                } else {
                    field(&mut hash, b"identical source");
                }
                if let Some(proof) = &source.addition_proof {
                    field(&mut hash, b"proven secondary addition");
                    field(&mut hash, proof.primary_head.as_bytes());
                    field(&mut hash, proof.secondary_head.as_bytes());
                    field(&mut hash, proof.merge_base.as_bytes());
                    field(&mut hash, proof.reason.as_bytes());
                } else {
                    field(&mut hash, b"not a secondary addition");
                }
            }
        }
        if let Some(digest) = &self.resolution_file_digest {
            field(&mut hash, b"explicit resolution file");
            field(&mut hash, digest.as_bytes());
        } else {
            field(&mut hash, b"no explicit resolution file");
        }
        format!("{hash:x}", hash = hash.finalize())
    }
}

/// Inventory every checked-out copy. Only a clean, committed common-base
/// secondary can yield to the primary source; all other divergence refuses cutover.
pub fn inventory_kind(root: &Path, kind: &str) -> Result<KindInventory> {
    inventory_kind_with_resolutions(root, kind, None)
}

pub fn inventory_kind_with_resolutions(
    root: &Path,
    kind: &str,
    resolutions: Option<&MigrationResolutionFile>,
) -> Result<KindInventory> {
    let relatives = source_paths(kind)?;
    let worktrees = sibling_worktrees(root)?;
    let mut documents = BTreeMap::new();
    let mut budget = InventoryBudget::default();
    for tree in &worktrees {
        for relative in relatives {
            if relative.ends_with(".yml") {
                let locator = if kind == "config" {
                    "backlog/config.yml"
                } else {
                    relative
                };
                inventory_file(
                    tree,
                    tree.join(relative),
                    locator.to_string(),
                    &mut documents,
                    &mut budget,
                )?;
            } else {
                inventory_directory(tree, relative, &mut documents, &mut budget)?;
            }
        }
    }
    if let Some(resolutions) = resolutions {
        apply_explicit_resolutions(kind, &mut documents, resolutions)?;
    }
    reconcile_task_locators(kind, &worktrees, &mut documents)?;
    reconcile_sources(&worktrees, &mut documents)?;
    prove_secondary_additions(kind, &worktrees, &mut documents)?;
    verify_missing_copies(&worktrees, &documents)?;
    let mut branch_refs = inventory_branches(root, kind, relatives, &documents, &mut budget)?;
    if !branch_refs.is_empty() {
        for tree in &worktrees {
            let head =
                String::from_utf8(git_bytes(tree, &["rev-parse", "--verify", "HEAD"], 128)?)?
                    .trim()
                    .to_owned();
            branch_refs.push((format!("WORKTREE_HEAD:{}", tree.display()), head));
        }
        branch_refs.sort();
    }
    for document in documents.values_mut() {
        document.sources.sort_by(|a, b| a.path.cmp(&b.path));
        document.sources.dedup_by(|a, b| a.path == b.path);
        if let Some((bytes, repair)) =
            super::migration_repairs::repair_selected(kind, &document.locator, &document.bytes)?
        {
            document.bytes = bytes;
            document.repair = Some(repair);
        }
    }
    let applied_resolutions = documents
        .values()
        .filter_map(|document| document.resolution.clone())
        .collect();
    Ok(KindInventory {
        kind: kind.into(),
        documents: documents.into_values().collect(),
        worktrees,
        branch_refs,
        resolution_file_digest: resolutions.map(|plan| plan.digest().to_owned()),
        resolutions: applied_resolutions,
    })
}

fn apply_explicit_resolutions(
    kind: &str,
    documents: &mut BTreeMap<String, InventoryDocument>,
    file: &MigrationResolutionFile,
) -> Result<()> {
    let mut claimed_locators = BTreeSet::new();
    let mut claimed_targets = BTreeSet::new();
    for resolution in &file.resolutions {
        anyhow::ensure!(
            !resolution.source_locators.is_empty()
                && resolution.source_locators.len() <= MAX_DOCUMENTS
                && !resolution.sources.is_empty()
                && resolution.sources.len() <= MAX_DOCUMENTS,
            "invalid explicit resolution size"
        );
        anyhow::ensure!(
            claimed_targets.insert(resolution.target_locator.clone()),
            "duplicate explicit resolution target {}",
            resolution.target_locator
        );
        let mut originals = Vec::new();
        for locator in &resolution.source_locators {
            anyhow::ensure!(
                claimed_locators.insert(locator.clone()),
                "source locator appears in more than one resolution: {locator}"
            );
            let document = documents.get(locator).with_context(|| {
                format!("resolution source locator is not inventoried: {locator}")
            })?;
            originals.extend(document.sources.iter().cloned());
        }
        anyhow::ensure!(
            resolution
                .source_locators
                .contains(&resolution.target_locator)
                || !documents.contains_key(&resolution.target_locator),
            "resolution target collides with an unlisted inventoried locator: {}",
            resolution.target_locator
        );
        let actual = originals
            .iter()
            .map(|source| (source.path.clone(), source.digest.clone()))
            .collect::<BTreeSet<_>>();
        let declared = resolution
            .sources
            .iter()
            .map(|source| {
                Ok((
                    source.path.canonicalize().with_context(|| {
                        format!(
                            "resolving explicit migration source {}",
                            source.path.display()
                        )
                    })?,
                    source.digest.clone(),
                ))
            })
            .collect::<Result<BTreeSet<_>>>()?;
        anyhow::ensure!(
            declared.len() == resolution.sources.len(),
            "duplicate physical source in explicit resolution for {}",
            resolution.target_locator
        );
        anyhow::ensure!(
            actual == declared,
            "explicit resolution physical sources or digests do not match inventory for {}",
            resolution.target_locator
        );
        let selected = resolution
            .sources
            .iter()
            .filter(|source| source.decision == ResolutionDecision::Select)
            .collect::<Vec<_>>();
        anyhow::ensure!(
            selected.len() == 1,
            "explicit resolution must select exactly one physical source for {}",
            resolution.target_locator
        );
        let selected = selected[0];
        let selected_path = selected.path.canonicalize()?;
        let source = originals
            .iter()
            .find(|source| source.path == selected_path && source.digest == selected.digest)
            .context("selected migration source disappeared")?;
        let mut target = source.bytes.clone();
        if let Some(task_id) = resolution.repair_task_id.as_deref() {
            target = super::migration_repairs::repair_task_identity(
                kind,
                &source.path,
                &resolution.target_locator,
                &target,
                task_id,
            )?;
        }
        let rejected_sources = resolution
            .sources
            .iter()
            .filter(|source| source.decision == ResolutionDecision::Reject)
            .map(|source| {
                Ok(ResolutionSource {
                    path: source.path.canonicalize()?,
                    digest: source.digest.clone(),
                    decision: source.decision,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        anyhow::ensure!(
            rejected_sources.len() + 1 == resolution.sources.len(),
            "every non-selected physical source must be explicitly rejected"
        );
        let applied = AppliedResolution {
            source_locators: resolution.source_locators.clone(),
            target_locator: resolution.target_locator.clone(),
            selected_source: selected_path,
            selected_source_digest: selected.digest.clone(),
            rejected_sources,
            target_digest: sha256(&target),
            repair_task_id: resolution.repair_task_id.clone(),
        };
        for locator in &resolution.source_locators {
            documents.remove(locator);
        }
        documents.insert(
            resolution.target_locator.clone(),
            InventoryDocument {
                locator: resolution.target_locator.clone(),
                bytes: target,
                sources: originals,
                repair: None,
                resolution: Some(applied),
            },
        );
    }
    Ok(())
}

fn source_paths(kind: &str) -> Result<&'static [&'static str]> {
    match kind {
        "initiative" => Ok(&["backlog/initiatives"]),
        "project" => Ok(&["backlog/projects"]),
        "task" => Ok(&[
            "backlog/tasks",
            "backlog/completed",
            "backlog/drafts",
            "backlog/archive/tasks",
        ]),
        "goals" => Ok(&["backlog/goals.yml"]),
        "ranking" => Ok(&["backlog/ranking.yml"]),
        "config" => Ok(&[
            "backlog/config.yml",
            ".backlog/config.yml",
            "backlog.config.yml",
        ]),
        _ => bail!("kind '{kind}' is not enabled for cutover yet"),
    }
}

fn sibling_worktrees(root: &Path) -> Result<Vec<PathBuf>> {
    let root = root
        .canonicalize()
        .context("resolving migration repository")?;
    let status = git_cmd()
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "--git-dir"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        if root.join(".git").exists() {
            bail!("cannot inventory Git worktrees; repair repository metadata");
        }
        return Ok(vec![root]);
    }
    let output = git_bytes(
        &root,
        &["worktree", "list", "--porcelain", "-z"],
        MAX_GIT_LIST_BYTES,
    )?;
    let mut paths = Vec::new();
    for field in output.split(|byte| *byte == 0) {
        if let Some(path) = field.strip_prefix(b"worktree ") {
            let path = PathBuf::from(std::str::from_utf8(path).context("non-UTF8 worktree path")?)
                .canonicalize()
                .context(
                    "unavailable worktree; repair or remove its registration before migration",
                )?;
            if !paths.contains(&path) {
                paths.push(path);
            }
            if paths.len() > MAX_WORKTREES {
                bail!("too many worktrees for bounded migration inventory");
            }
        }
    }
    anyhow::ensure!(
        paths.contains(&root),
        "current checkout is absent from Git worktree list"
    );
    anyhow::ensure!(
        !paths.is_empty(),
        "Git worktree list has no primary checkout"
    );
    Ok(paths)
}

fn inventory_directory(
    root: &Path,
    relative: &str,
    documents: &mut BTreeMap<String, InventoryDocument>,
    budget: &mut InventoryBudget,
) -> Result<()> {
    let dir = root.join(relative);
    if !source_exists_without_symlinks(root, &dir)? {
        return Ok(());
    }
    for (index, entry) in fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .enumerate()
    {
        if index >= MAX_DOCUMENTS {
            bail!("directory exceeds bounded entry count: {}", dir.display());
        }
        let path = entry?.path();
        if path.extension().and_then(|v| v.to_str()) != Some("md") {
            continue;
        }
        let locator = path
            .strip_prefix(root)?
            .to_str()
            .context("non-UTF8 document locator")?
            .to_owned();
        inventory_file(root, path, locator, documents, budget)?;
    }
    Ok(())
}

fn inventory_file(
    root: &Path,
    path: PathBuf,
    locator: String,
    documents: &mut BTreeMap<String, InventoryDocument>,
    budget: &mut InventoryBudget,
) -> Result<()> {
    if !source_exists_without_symlinks(root, &path)? {
        return Ok(());
    }
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_file() || meta.len() > MAX_DOCUMENT_BYTES {
        bail!(
            "unsupported or oversized migration source {}",
            path.display()
        );
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)?
        .take(MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        bail!(
            "migration source grew beyond byte limit: {}",
            path.display()
        );
    }
    budget.account(bytes.len())?;
    merge_source(documents, locator, path, bytes)
}

fn source_exists_without_symlinks(root: &Path, path: &Path) -> Result<bool> {
    let mut cursor = root.to_path_buf();
    for component in path.strip_prefix(root)?.components() {
        cursor.push(component);
        match fs::symlink_metadata(&cursor) {
            Ok(meta) if meta.file_type().is_symlink() => {
                bail!("symlinked migration source path {}", cursor.display())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(true)
}

/// Bound subprocess output before allocating source/tree listings. Never fetch or mutate refs.
fn git_bytes(root: &Path, args: &[&str], limit: usize) -> Result<Vec<u8>> {
    let mut child = git_cmd()
        .arg("-C")
        .arg(root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .context("missing Git stdout")?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > limit {
        let _ = child.kill();
        let _ = child.wait();
        read?;
        bail!("Git migration inventory output exceeds bounded size");
    }
    let status = child.wait()?;
    if !status.success() {
        bail!("Git migration inventory failed: {}", args.join(" "));
    }
    Ok(bytes)
}

fn ancestor(root: &Path, tip: &str, head: &str) -> Result<bool> {
    let status = git_cmd()
        .arg("-C")
        .arg(root)
        .args(["merge-base", "--is-ancestor", tip, head])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => bail!("cannot determine branch ancestry for migration: {tip}"),
    }
}

fn branch_locator(kind: &str, path: &str, relatives: &[&str]) -> Option<String> {
    let matches = relatives.iter().any(|relative| {
        if relative.ends_with(".yml") {
            path == *relative
        } else {
            Path::new(path).parent() == Some(Path::new(relative)) && path.ends_with(".md")
        }
    });
    matches.then(|| {
        if kind == "config" {
            "backlog/config.yml".into()
        } else {
            path.into()
        }
    })
}

fn inventory_branches(
    root: &Path,
    kind: &str,
    relatives: &[&str],
    documents: &BTreeMap<String, InventoryDocument>,
    budget: &mut InventoryBudget,
) -> Result<Vec<(String, String)>> {
    // `worktree list` succeeds for a Git repo even with an unborn HEAD. Non-Git is supported.
    let probe = git_cmd()
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--git-dir"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !probe.success() {
        return Ok(Vec::new());
    }
    let refs = git_bytes(
        root,
        &[
            "for-each-ref",
            "--count=257",
            "--format=%(refname) %(objectname)",
            "refs/heads/",
        ],
        MAX_GIT_LIST_BYTES,
    )?;
    let mut refs: Vec<(String, String)> = std::str::from_utf8(&refs)?
        .lines()
        .map(|line| {
            let (name, oid) = line
                .split_once(' ')
                .context("malformed Git branch inventory")?;
            Ok((name.to_string(), oid.to_string()))
        })
        .collect::<Result<_>>()?;
    if refs.len() > MAX_BRANCHES {
        bail!(
            "more than {MAX_BRANCHES} local branches; reconcile branch inventory before migration"
        );
    }
    if refs.is_empty() {
        return Ok(refs);
    }
    let worktrees = git_bytes(
        root,
        &["worktree", "list", "--porcelain", "-z"],
        MAX_GIT_LIST_BYTES,
    )?;
    let primary = worktrees
        .split(|b| *b == 0)
        .find_map(|field| field.strip_prefix(b"worktree "))
        .context("Git inventory has no primary worktree")?;
    let primary = Path::new(std::str::from_utf8(primary)?);
    let current_head =
        String::from_utf8(git_bytes(root, &["rev-parse", "--verify", "HEAD"], 128)?)?
            .trim()
            .to_string();
    let primary_head =
        String::from_utf8(git_bytes(primary, &["rev-parse", "--verify", "HEAD"], 128)?)?
            .trim()
            .to_string();
    for (branch, tip) in &refs {
        // Inspect all unmerged refs, including old tips. This includes allocator's 30-day
        // active window without letting an older branch-only record disappear at cutover.
        // Merged ancestors are obsolete history, not divergent current sources.
        if ancestor(root, tip, &primary_head)? {
            continue;
        }
        inspect_branch(
            root,
            &primary_head,
            (branch, tip),
            kind,
            relatives,
            documents,
            budget,
        )?;
    }
    refs.push(("HEAD".into(), current_head));
    refs.push(("PRIMARY_HEAD".into(), primary_head));
    refs.sort();
    Ok(refs)
}

fn inspect_branch(
    root: &Path,
    primary_head: &str,
    branch_ref: (&str, &str),
    kind: &str,
    relatives: &[&str],
    documents: &BTreeMap<String, InventoryDocument>,
    budget: &mut InventoryBudget,
) -> Result<()> {
    let (branch, tip) = branch_ref;
    // Only branch-changed paths are candidates. An unchanged file inherited from
    // a common ancestor is obsolete history when the checked-out copy was edited.
    let base = String::from_utf8(git_bytes(root, &["merge-base", primary_head, tip], 128)?)?;
    let mut deleted_args = vec![
        "diff",
        "--name-only",
        "--diff-filter=D",
        "--no-renames",
        "-z",
        base.trim(),
        tip,
        "--",
    ];
    deleted_args.extend_from_slice(relatives);
    let deleted = git_bytes(root, &deleted_args, MAX_GIT_LIST_BYTES)?;
    for path in deleted
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = std::str::from_utf8(path).context("non-UTF8 deleted branch source")?;
        if branch_locator(kind, path, relatives).is_some() {
            bail!("unmerged branch deletion {branch}:{path}; reconcile explicitly before cutover");
        }
    }
    let mut changed_args = vec!["diff", "--name-only", "-z", base.trim(), tip, "--"];
    changed_args.extend_from_slice(relatives);
    let changed = git_bytes(root, &changed_args, MAX_GIT_LIST_BYTES)?;
    let changed: BTreeSet<&[u8]> = changed
        .split(|b| *b == 0)
        .filter(|name| !name.is_empty())
        .collect();
    if changed.len() > MAX_DOCUMENTS {
        bail!("branch diff exceeds bounded source count: {branch}");
    }
    let mut args = vec!["ls-tree", "-r", "-z", tip, "--"];
    args.extend_from_slice(relatives);
    let tree = git_bytes(root, &args, MAX_GIT_LIST_BYTES)?;
    let mut entries = 0;
    for entry in tree.split(|b| *b == 0).filter(|entry| !entry.is_empty()) {
        entries += 1;
        if entries > MAX_DOCUMENTS {
            bail!("branch tree exceeds bounded source count: {branch}");
        }
        let text = std::str::from_utf8(entry).context("non-UTF8 branch source path")?;
        let (metadata, path) = text.split_once('\t').context("malformed Git tree entry")?;
        if !changed.contains(path.as_bytes()) {
            continue;
        }
        let Some(locator) = branch_locator(kind, path, relatives) else {
            continue;
        };
        let fields: Vec<_> = metadata.split(' ').collect();
        if fields.len() != 3 || !matches!(fields[0], "100644" | "100755") || fields[1] != "blob" {
            bail!("unsupported branch migration source {branch}:{path}");
        }
        let bytes = git_bytes(
            root,
            &["cat-file", "blob", fields[2]],
            MAX_DOCUMENT_BYTES as usize,
        )?;
        budget.account(bytes.len())?;
        let document = documents.get(&locator).or_else(|| {
            documents.values().find(|document| {
                document.resolution.as_ref().is_some_and(|resolution| {
                    resolution
                        .source_locators
                        .iter()
                        .any(|source| source == &locator)
                })
            })
        });
        let preserved = document.is_some_and(|document| {
            document.resolution.is_some()
                && document.sources.iter().any(|source| source.bytes == bytes)
        });
        if !preserved {
            bail!("unmerged branch source {branch}:{path} is absent from or differs from checked-out inventory; reconcile explicitly before cutover");
        }
    }
    Ok(())
}

fn merge_source(
    documents: &mut BTreeMap<String, InventoryDocument>,
    locator: String,
    path: PathBuf,
    bytes: Vec<u8>,
) -> Result<()> {
    let source = InventorySource {
        path,
        bytes: bytes.clone(),
        digest: sha256(&bytes),
        stale_proof: None,
        addition_proof: None,
    };
    if let Some(existing) = documents.get_mut(&locator) {
        existing.sources.push(source);
    } else {
        documents.insert(
            locator.clone(),
            InventoryDocument {
                locator,
                bytes,
                sources: vec![source],
                repair: None,
                resolution: None,
            },
        );
    }
    Ok(())
}

fn source_tree<'a>(path: &Path, worktrees: &'a [PathBuf]) -> Result<&'a PathBuf> {
    worktrees
        .iter()
        .filter(|tree| path.starts_with(tree))
        .max_by_key(|tree| tree.components().count())
        .context("source has no worktree")
}

fn reconcile_task_locators(
    kind: &str,
    worktrees: &[PathBuf],
    documents: &mut BTreeMap<String, InventoryDocument>,
) -> Result<()> {
    if kind != "task" || worktrees.len() < 2 {
        return Ok(());
    }
    let primary = &worktrees[0];
    let mut by_id = BTreeMap::<String, Vec<String>>::new();
    for (locator, document) in documents.iter() {
        if document.resolution.is_some() {
            continue;
        }
        if let Some(source) = document
            .sources
            .iter()
            .find(|source| source_tree(&source.path, worktrees).ok() == Some(primary))
        {
            let task = super::parse_task_text(
                &source.path,
                super::BacklogTaskSource::Active,
                std::str::from_utf8(&source.bytes)?,
            )?
            .0;
            by_id
                .entry(task.id.to_ascii_uppercase())
                .or_default()
                .push(locator.clone());
        }
    }
    let absent = documents
        .iter()
        .filter(|(_, document)| {
            document.resolution.is_none()
                && !document
                    .sources
                    .iter()
                    .any(|source| source_tree(&source.path, worktrees).ok() == Some(primary))
        })
        .map(|(locator, _)| locator.clone())
        .collect::<Vec<_>>();
    for locator in absent {
        let mut old = documents
            .remove(&locator)
            .context("old lifecycle document disappeared")?;
        let first = &old.sources[0];
        let task = super::parse_task_text(
            &first.path,
            super::BacklogTaskSource::Active,
            std::str::from_utf8(&first.bytes)?,
        )?
        .0;
        let Some(matches) = by_id.get(&task.id.to_ascii_uppercase()) else {
            documents.insert(locator, old);
            continue; // A separate absence proof distinguishes new additions from deletions.
        };
        anyhow::ensure!(
            matches.len() == 1,
            "secondary task {} at {} has ambiguous primary public ID: {}",
            task.id,
            first.path.display(),
            matches.join(", ")
        );
        let selected = documents
            .get_mut(&matches[0])
            .context("primary lifecycle document disappeared")?;
        let primary_source = selected
            .sources
            .iter()
            .find(|source| source_tree(&source.path, worktrees).ok() == Some(primary))
            .context("primary lifecycle source missing")?;
        for source in &mut old.sources {
            let secondary = source_tree(&source.path, worktrees)?;
            let parsed = super::parse_task_text(
                &source.path,
                super::BacklogTaskSource::Active,
                std::str::from_utf8(&source.bytes)?,
            )?
            .0;
            anyhow::ensure!(
                parsed.id.eq_ignore_ascii_case(&task.id),
                "old lifecycle copies disagree on task identity"
            );
            source.stale_proof = Some(
                stale_source_proof(primary, secondary, &primary_source.path, source, true)
                    .with_context(|| format!("task {} source {} cannot yield to primary {}: competing or uncommitted lifecycle changes", task.id, source.path.display(), primary_source.path.display()))?,
            );
        }
        selected.sources.extend(old.sources);
    }
    Ok(())
}

fn prove_secondary_additions(
    kind: &str,
    worktrees: &[PathBuf],
    documents: &mut BTreeMap<String, InventoryDocument>,
) -> Result<()> {
    if worktrees.len() < 2 {
        return Ok(());
    }
    let primary = &worktrees[0];
    let primary_head =
        String::from_utf8(git_bytes(primary, &["rev-parse", "--verify", "HEAD"], 128)?)?
            .trim()
            .to_owned();
    let mut task_ids = BTreeMap::<String, BTreeSet<String>>::new();
    for document in documents.values_mut() {
        if document.resolution.is_some() {
            continue;
        }
        if document
            .sources
            .iter()
            .any(|source| source_tree(&source.path, worktrees).ok() == Some(primary))
        {
            continue;
        }
        anyhow::ensure!(
            document
                .sources
                .iter()
                .all(|source| source.bytes == document.bytes),
            "competing secondary additions for {} require reconciliation",
            document.locator
        );
        for source in &mut document.sources {
            let secondary = source_tree(&source.path, worktrees)?;
            let relative = source
                .path
                .strip_prefix(secondary)?
                .to_str()
                .context("non-UTF8 new source path")?;
            let secondary_head = String::from_utf8(git_bytes(
                secondary,
                &["rev-parse", "--verify", "HEAD"],
                128,
            )?)?
            .trim()
            .to_owned();
            let bases = String::from_utf8(git_bytes(
                primary,
                &["merge-base", "--all", &primary_head, &secondary_head],
                1024,
            )?)?;
            let bases = bases.lines().collect::<Vec<_>>();
            anyhow::ensure!(
                bases.len() == 1,
                "new source {} ({}) has no unique common base with primary",
                source.path.display(),
                document.locator
            );
            let base = bases[0];
            let relevant = if kind == "config" {
                source_paths(kind)?.to_vec()
            } else {
                vec![relative]
            };
            for path in relevant {
                anyhow::ensure!(git_blob_if_exists(primary, &primary_head, path)?.is_none() && git_blob_if_exists(primary, base, path)?.is_none(), "{} has no primary source but {} exists in primary HEAD/common base; possible deletion, reconcile source {}", document.locator, path, source.path.display());
            }
            if kind == "task" {
                let task = super::parse_task_text(
                    &source.path,
                    super::BacklogTaskSource::Active,
                    std::str::from_utf8(&source.bytes)?,
                )?
                .0;
                let id = task.id.to_ascii_uppercase();
                for revision in [&primary_head, &base.to_owned()] {
                    if !task_ids.contains_key(revision) {
                        task_ids.insert(revision.clone(), task_ids_at_revision(primary, revision)?);
                    }
                    anyhow::ensure!(!task_ids[revision].contains(&id), "task {} at {} has no primary source but its public ID exists in primary HEAD/common base {}; reconcile possible deletion or rename", task.id, source.path.display(), revision);
                }
            }
            source.addition_proof = Some(NewSourceProof { primary_head: primary_head.clone(), secondary_head, merge_base: base.into(), reason: "source absent from primary HEAD and unique common base; no primary task-ID collision" });
        }
    }
    Ok(())
}

fn task_ids_at_revision(root: &Path, revision: &str) -> Result<BTreeSet<String>> {
    let mut args = vec!["ls-tree", "-r", "-z", revision, "--"];
    args.extend_from_slice(source_paths("task")?);
    let listing = git_bytes(root, &args, MAX_GIT_LIST_BYTES)?;
    let mut ids = BTreeSet::new();
    let mut total_bytes = 0usize;
    for (index, entry) in listing
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .enumerate()
    {
        anyhow::ensure!(
            index < MAX_DOCUMENTS,
            "primary task identity inventory exceeds limit"
        );
        let text = std::str::from_utf8(entry)?;
        let (metadata, path) = text
            .split_once('\t')
            .context("malformed historical task tree")?;
        if branch_locator("task", path, source_paths("task")?).is_none() {
            continue;
        }
        let fields = metadata.split_whitespace().collect::<Vec<_>>();
        anyhow::ensure!(
            fields.len() == 3 && matches!(fields[0], "100644" | "100755") && fields[1] == "blob",
            "unsupported historical task source {path}"
        );
        let content = git_bytes(
            root,
            &["cat-file", "blob", fields[2]],
            MAX_DOCUMENT_BYTES as usize,
        )?;
        total_bytes = total_bytes
            .checked_add(content.len())
            .context("historical task size overflow")?;
        anyhow::ensure!(
            total_bytes <= MAX_TOTAL_BYTES,
            "historical task identity inventory exceeds byte limit"
        );
        let task = super::parse_task_text(
            Path::new(path),
            super::BacklogTaskSource::Active,
            std::str::from_utf8(&content)?,
        )?
        .0;
        ids.insert(task.id.to_ascii_uppercase());
    }
    Ok(ids)
}

fn verify_missing_copies(
    worktrees: &[PathBuf],
    documents: &BTreeMap<String, InventoryDocument>,
) -> Result<()> {
    if worktrees.len() < 2 {
        return Ok(());
    }
    let primary = &worktrees[0];
    for document in documents.values() {
        if document.resolution.is_some() {
            continue;
        }
        let anchor = document
            .sources
            .iter()
            .find(|source| source_tree(&source.path, worktrees).ok() == Some(primary))
            .or_else(|| document.sources.first())
            .context("document has no source")?;
        let anchor_tree = source_tree(&anchor.path, worktrees)?;
        let anchor_head = String::from_utf8(git_bytes(
            anchor_tree,
            &["rev-parse", "--verify", "HEAD"],
            128,
        )?)?
        .trim()
        .to_owned();
        let relative = anchor
            .path
            .strip_prefix(anchor_tree)?
            .to_str()
            .context("non-UTF8 source path")?;
        for secondary in worktrees.iter().filter(|tree| *tree != anchor_tree) {
            // A proven old lifecycle source is present in this document's provenance.
            if document
                .sources
                .iter()
                .any(|source| source_tree(&source.path, worktrees).ok() == Some(secondary))
            {
                continue;
            }
            let status = git_bytes(
                secondary,
                &[
                    "status",
                    "--porcelain=v1",
                    "--untracked-files=all",
                    "--",
                    relative,
                ],
                MAX_GIT_LIST_BYTES,
            )?;
            anyhow::ensure!(
                status.is_empty(),
                "missing secondary source has uncommitted/staged deletion: {}",
                secondary.join(relative).display()
            );
            let head = String::from_utf8(git_bytes(
                secondary,
                &["rev-parse", "--verify", "HEAD"],
                128,
            )?)?
            .trim()
            .to_owned();
            anyhow::ensure!(
                git_blob_if_exists(secondary, &head, relative)?.is_none(),
                "tracked secondary source is missing: {}",
                secondary.join(relative).display()
            );
            let bases = String::from_utf8(git_bytes(
                primary,
                &["merge-base", "--all", &anchor_head, &head],
                1024,
            )?)?;
            let bases = bases.lines().collect::<Vec<_>>();
            anyhow::ensure!(bases.len() == 1, "missing source has no unique common base");
            anyhow::ensure!(
                git_blob_if_exists(anchor_tree, bases[0], relative)?.is_none(),
                "secondary committed deletion of {}; reconcile explicitly before cutover",
                relative
            );
        }
    }
    Ok(())
}

fn git_blob_if_exists(root: &Path, revision: &str, relative: &str) -> Result<Option<Vec<u8>>> {
    let listing = git_bytes(
        root,
        &["ls-tree", "-z", revision, "--", relative],
        MAX_GIT_LIST_BYTES,
    )?;
    if listing.is_empty() {
        return Ok(None);
    }
    let fields = std::str::from_utf8(&listing)?
        .trim_end_matches('\0')
        .split_once('\t')
        .context("malformed source tree entry")?
        .0
        .split_whitespace()
        .collect::<Vec<_>>();
    anyhow::ensure!(
        fields.len() == 3 && matches!(fields[0], "100644" | "100755") && fields[1] == "blob",
        "unsupported committed source mode"
    );
    Ok(Some(git_bytes(
        root,
        &["cat-file", "blob", fields[2]],
        MAX_DOCUMENT_BYTES as usize,
    )?))
}

fn reconcile_sources(
    worktrees: &[PathBuf],
    documents: &mut BTreeMap<String, InventoryDocument>,
) -> Result<()> {
    let primary = worktrees
        .first()
        .context("migration has no primary source root")?;
    for document in documents.values_mut() {
        if document.resolution.is_some() {
            continue;
        }
        if document
            .sources
            .iter()
            .all(|source| source.bytes == document.bytes)
        {
            continue;
        }
        let primary_sources = document
            .sources
            .iter()
            .filter(|source| {
                source.path.starts_with(primary)
                    && worktrees
                        .iter()
                        .filter(|tree| source.path.starts_with(tree))
                        .max_by_key(|tree| tree.components().count())
                        == Some(primary)
            })
            .collect::<Vec<_>>();
        anyhow::ensure!(primary_sources.len() == 1, "divergent copies of {} have no unique primary source; reconcile explicitly before cutover", document.locator);
        let selected = primary_sources[0].bytes.clone();
        let primary_path = primary_sources[0].path.clone();
        for source in &mut document.sources {
            if source.bytes == selected {
                continue;
            }
            let secondary = source_tree(&source.path, worktrees)?;
            anyhow::ensure!(
                secondary != primary,
                "divergent config aliases in primary checkout require explicit reconciliation"
            );
            let proof = stale_source_proof(
                primary,
                secondary,
                &primary_path,
                source,
                source.stale_proof.is_some(),
            )
            .with_context(|| {
                format!(
                    "divergent copies of {}: {} and {}; reconcile explicitly before cutover",
                    document.locator,
                    primary_path.display(),
                    source.path.display()
                )
            })?;
            source.stale_proof = Some(proof);
        }
        document.bytes = selected;
    }
    Ok(())
}

fn stale_source_proof(
    primary: &Path,
    secondary: &Path,
    primary_path: &Path,
    source: &InventorySource,
    lifecycle_move: bool,
) -> Result<StaleSourceProof> {
    let relative = source
        .path
        .strip_prefix(secondary)?
        .to_str()
        .context("non-UTF8 secondary source path")?;
    anyhow::ensure!(
        lifecycle_move || primary_path.strip_prefix(primary)? == Path::new(relative),
        "different config aliases cannot be reconciled by worktree ancestry"
    );
    let status = git_bytes(
        secondary,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--",
            relative,
        ],
        MAX_GIT_LIST_BYTES,
    )?;
    anyhow::ensure!(
        status.is_empty(),
        "secondary source has uncommitted or staged changes"
    );
    let primary_head =
        String::from_utf8(git_bytes(primary, &["rev-parse", "--verify", "HEAD"], 128)?)?
            .trim()
            .to_string();
    let secondary_head = String::from_utf8(git_bytes(
        secondary,
        &["rev-parse", "--verify", "HEAD"],
        128,
    )?)?
    .trim()
    .to_string();
    let at_head = git_bytes(
        secondary,
        &["show", &format!("{secondary_head}:{relative}")],
        MAX_DOCUMENT_BYTES as usize,
    )?;
    anyhow::ensure!(
        at_head == source.bytes,
        "secondary bytes differ from its committed HEAD"
    );
    let bases = String::from_utf8(git_bytes(
        primary,
        &["merge-base", "--all", &primary_head, &secondary_head],
        1024,
    )?)?;
    let bases = bases.lines().collect::<Vec<_>>();
    anyhow::ensure!(
        bases.len() == 1,
        "source ancestry has no unique common base"
    );
    let base = bases[0];
    let at_base = git_bytes(
        primary,
        &["show", &format!("{base}:{relative}")],
        MAX_DOCUMENT_BYTES as usize,
    )?;
    anyhow::ensure!(
        at_base == source.bytes,
        "secondary contains committed changes since the common base"
    );
    Ok(StaleSourceProof {
        primary_head,
        secondary_head,
        merge_base: base.into(),
        reason: "clean secondary equals its HEAD and unique common-base bytes",
    })
}

pub fn content_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256(bytes: &[u8]) -> String {
    content_digest(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inventory_preserves_custom_bytes_and_rejects_divergence() {
        let root = tempfile::tempdir().expect("fixture");
        fs::create_dir_all(root.path().join("backlog/initiatives")).expect("directory");
        let raw = b"---\nname: Long term\ncustom: {nested: [1, null]}\n---\n## Unknown\nKeep me\n";
        fs::write(root.path().join("backlog/initiatives/long.md"), raw).expect("source");
        let inventory = inventory_kind(root.path(), "initiative").expect("inventory");
        assert_eq!(inventory.documents.len(), 1);
        assert_eq!(inventory.documents[0].bytes, raw);
        let mut map = BTreeMap::new();
        merge_source(&mut map, "same".into(), "a".into(), raw.to_vec()).expect("first");
        merge_source(&mut map, "same".into(), "b".into(), b"other".to_vec()).unwrap();
        assert!(reconcile_sources(&[root.path().to_path_buf()], &mut map).is_err());
        assert_eq!(map["same"].bytes, raw);
    }

    fn git(root: &Path, args: &[&str]) -> String {
        let output = git_cmd()
            .arg("-C")
            .arg(root)
            .args([
                "-c",
                "user.name=Migration test",
                "-c",
                "user.email=migration@example.invalid",
            ])
            .args(args)
            .output()
            .expect("git process");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("utf8 Git output")
            .trim()
            .to_owned()
    }

    fn git_fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("fixture");
        git(root.path(), &["init", "-q", "-b", "main"]);
        fs::create_dir_all(root.path().join("backlog/initiatives")).expect("directory");
        fs::write(root.path().join("backlog/initiatives/shared.md"), b"shared").expect("source");
        git(root.path(), &["add", "."]);
        git(root.path(), &["commit", "-qm", "initial"]);
        root
    }

    #[test]
    fn unmerged_branch_only_definition_refuses_cutover() {
        let root = git_fixture();
        git(root.path(), &["checkout", "-qb", "unmerged"]);
        fs::write(
            root.path().join("backlog/initiatives/branch-only.md"),
            b"preserve me",
        )
        .expect("source");
        git(root.path(), &["add", "."]);
        git(root.path(), &["commit", "-qm", "branch-only definition"]);
        git(root.path(), &["checkout", "-q", "main"]);
        let error =
            inventory_kind(root.path(), "initiative").expect_err("unmerged data must block");
        assert!(error
            .to_string()
            .contains("refs/heads/unmerged:backlog/initiatives/branch-only.md"));
        assert!(!root
            .path()
            .join("backlog/initiatives/branch-only.md")
            .exists());
    }

    #[test]
    fn identical_branch_source_requires_explicit_physical_resolution() {
        let root = git_fixture();
        git(root.path(), &["checkout", "-qb", "unmerged"]);
        fs::write(
            root.path().join("backlog/initiatives/branch-only.md"),
            b"preserve me",
        )
        .expect("source");
        git(root.path(), &["add", "."]);
        git(root.path(), &["commit", "-qm", "branch-only definition"]);
        git(root.path(), &["checkout", "-q", "main"]);
        fs::write(
            root.path().join("backlog/initiatives/branch-only.md"),
            b"preserve me",
        )
        .expect("matching uncommitted copy");
        let error = inventory_kind(root.path(), "initiative").expect_err("branch needs review");
        assert!(
            error.to_string().contains("reconcile explicitly"),
            "{error}"
        );
        let locator = "backlog/initiatives/branch-only.md";
        let path = root.path().join(locator).canonicalize().unwrap();
        let plan = MigrationResolutionFile {
            version: 1,
            kind: "initiative".into(),
            resolutions: vec![MigrationResolution {
                source_locators: vec![locator.into()],
                target_locator: locator.into(),
                sources: vec![ResolutionSource {
                    path: path.clone(),
                    digest: content_digest(b"preserve me"),
                    decision: ResolutionDecision::Select,
                }],
                repair_task_id: None,
            }],
            digest: "reviewed-plan".into(),
        };
        let before = inventory_kind_with_resolutions(root.path(), "initiative", Some(&plan))
            .expect("explicitly selected physical branch bytes");
        assert_eq!(before.documents.len(), 2);
        let tree = git(root.path(), &["rev-parse", "unmerged^{tree}"]);
        let parent = git(root.path(), &["rev-parse", "unmerged"]);
        let next = git(
            root.path(),
            &[
                "commit-tree",
                &tree,
                "-p",
                &parent,
                "-m",
                "same tree newer ref",
            ],
        );
        git(root.path(), &["update-ref", "refs/heads/unmerged", &next]);
        let after = inventory_kind_with_resolutions(root.path(), "initiative", Some(&plan))
            .expect("same explicitly resolved bytes still accepted");
        assert_ne!(before.branch_refs, after.branch_refs);
        assert_ne!(before.digest(), after.digest());
        assert_eq!(
            after.digest(),
            inventory_kind_with_resolutions(root.path(), "initiative", Some(&plan))
                .unwrap()
                .digest()
        );
    }

    #[test]
    fn obsolete_ancestor_and_unchanged_branch_content_do_not_block() {
        let root = git_fixture();
        git(root.path(), &["branch", "merged-ancestor"]);
        git(root.path(), &["checkout", "-qb", "unrelated-work"]);
        fs::write(root.path().join("README.md"), b"other work").expect("source");
        git(root.path(), &["add", "."]);
        git(root.path(), &["commit", "-qm", "unrelated work"]);
        git(root.path(), &["checkout", "-q", "main"]);
        fs::write(
            root.path().join("backlog/initiatives/shared.md"),
            b"new current content",
        )
        .expect("source");
        git(root.path(), &["add", "."]);
        git(root.path(), &["commit", "-qm", "new definition content"]);
        let inventory = inventory_kind(root.path(), "initiative")
            .expect("historical content is not active branch change");
        assert_eq!(inventory.documents[0].bytes, b"new current content");
    }

    #[test]
    fn inventory_budget_counts_duplicate_sources_and_accepts_exact_bound() {
        let mut budget = InventoryBudget {
            bytes: MAX_TOTAL_BYTES - 1,
            sources: MAX_DOCUMENTS - 1,
        };
        budget.account(1).expect("inclusive limits");
        assert!(
            budget.account(0).is_err(),
            "zero-byte duplicate sources still count"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_parent_directory_is_rejected() {
        let root = tempfile::tempdir().expect("fixture");
        let external = tempfile::tempdir().expect("outside");
        fs::create_dir_all(external.path().join("initiatives")).expect("directory");
        fs::write(external.path().join("initiatives/one.md"), b"outside").expect("source");
        std::os::unix::fs::symlink(external.path(), root.path().join("backlog")).expect("symlink");
        let error = inventory_kind(root.path(), "initiative").expect_err("symlinked parent");
        assert!(error
            .to_string()
            .contains("symlinked migration source path"));
    }
    fn linked_fixture(root: &Path, secondary: &Path) {
        git(
            root,
            &[
                "worktree",
                "add",
                "-qb",
                "secondary",
                secondary.to_str().unwrap(),
            ],
        );
    }

    #[test]
    fn clean_stale_secondary_uses_primary_and_preserves_every_original_byte() {
        let primary = git_fixture();
        let parent = tempfile::tempdir().unwrap();
        let secondary = parent.path().join("a-secondary");
        linked_fixture(primary.path(), &secondary);
        fs::write(secondary.join("README.md"), "unrelated branch work").unwrap();
        git(&secondary, &["add", "."]);
        git(&secondary, &["commit", "-qm", "unrelated"]);
        let locator = "backlog/initiatives/shared.md";
        fs::write(primary.path().join(locator), b"primary newer raw bytes").unwrap();
        git(primary.path(), &["add", "."]);
        git(primary.path(), &["commit", "-qm", "primary update"]);
        let inventory = inventory_kind(&secondary, "initiative").unwrap();
        assert_eq!(
            inventory.worktrees[0],
            primary.path().canonicalize().unwrap()
        );
        assert_eq!(inventory.documents[0].bytes, b"primary newer raw bytes");
        assert_eq!(
            inventory.documents[0]
                .sources
                .iter()
                .filter(|source| source.stale_proof.is_some())
                .count(),
            1
        );
        let mut store = crate::storage::Store::open(parent.path().join("db.sqlite3")).unwrap();
        let repo = store.bind_repository(primary.path()).unwrap();
        let plan = inventory.capture_plan(repo.clone()).unwrap();
        assert_eq!(store.apply_migration(&plan).unwrap(), 1);
        assert_eq!(
            store
                .read(&repo, "initiative", locator)
                .unwrap()
                .unwrap()
                .content,
            b"primary newer raw bytes"
        );
        let connection = rusqlite::Connection::open(store.path()).unwrap();
        let mut statement = connection
            .prepare("SELECT source_path,content FROM migration_sources ORDER BY source_path")
            .unwrap();
        let originals = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(originals.len(), 2);
        assert!(originals.iter().any(|(path, bytes)| Path::new(path)
            == secondary.join(locator).canonicalize().unwrap()
            && bytes == b"shared"));
        assert!(originals.iter().any(|(path, bytes)| Path::new(path)
            == primary.path().join(locator).canonicalize().unwrap()
            && bytes == b"primary newer raw bytes"));
        assert_eq!(fs::read(secondary.join(locator)).unwrap(), b"shared");
        assert_eq!(
            fs::read(primary.path().join(locator)).unwrap(),
            b"primary newer raw bytes"
        );
    }

    #[test]
    fn dirty_or_competing_secondary_changes_are_not_stale() {
        let primary = git_fixture();
        let parent = tempfile::tempdir().unwrap();
        let secondary = parent.path().join("secondary");
        linked_fixture(primary.path(), &secondary);
        let locator = "backlog/initiatives/shared.md";
        fs::write(primary.path().join(locator), b"primary newer").unwrap();
        git(primary.path(), &["add", "."]);
        git(primary.path(), &["commit", "-qm", "primary update"]);
        fs::write(secondary.join(locator), b"secondary dirty").unwrap();
        assert!(inventory_kind(primary.path(), "initiative").is_err());
        git(&secondary, &["add", "."]);
        git(&secondary, &["commit", "-qm", "competing update"]);
        assert!(inventory_kind(primary.path(), "initiative").is_err());
    }

    #[test]
    fn secondary_deleted_sources_and_primary_absence_refuse_resurrection() {
        let primary = git_fixture();
        let parent = tempfile::tempdir().unwrap();
        let secondary = parent.path().join("secondary");
        linked_fixture(primary.path(), &secondary);
        let locator = "backlog/initiatives/shared.md";
        fs::remove_file(secondary.join(locator)).unwrap();
        assert!(inventory_kind(primary.path(), "initiative").is_err());
        git(&secondary, &["add", "-u"]);
        git(&secondary, &["commit", "-qm", "delete on branch"]);
        assert!(inventory_kind(primary.path(), "initiative").is_err());
        assert!(inventory_kind(&secondary, "initiative").is_err());
        // An unchecked-out unmerged deletion must also be seen by branch inventory.
        git(
            primary.path(),
            &["worktree", "remove", secondary.to_str().unwrap()],
        );
        let error = inventory_kind(primary.path(), "initiative").unwrap_err();
        assert!(
            error.to_string().contains("unmerged branch deletion"),
            "{error}"
        );
    }

    #[test]
    fn primary_lifecycle_move_reconciles_old_task_path_without_duplicate_record() {
        let primary = git_fixture();
        let active = "backlog/tasks/task-7 - Original.md";
        let completed = "backlog/completed/task-7 - Original.md";
        let raw = "---\nid: TASK-7\ntitle: Original\nstatus: To Do\ncustom: [keep, null]\n---\n";
        fs::create_dir_all(primary.path().join("backlog/tasks")).unwrap();
        fs::write(primary.path().join(active), raw).unwrap();
        git(primary.path(), &["add", "."]);
        git(primary.path(), &["commit", "-qm", "task baseline"]);
        let parent = tempfile::tempdir().unwrap();
        let secondary = parent.path().join("secondary");
        linked_fixture(primary.path(), &secondary);
        fs::create_dir_all(primary.path().join("backlog/completed")).unwrap();
        fs::rename(primary.path().join(active), primary.path().join(completed)).unwrap();
        let done = raw.replace("status: To Do", "status: Done");
        fs::write(primary.path().join(completed), &done).unwrap();
        git(primary.path(), &["add", "-A"]);
        git(primary.path(), &["commit", "-qm", "complete task"]);
        let inventory = inventory_kind(&secondary, "task").unwrap();
        inventory.validate_native().unwrap();
        assert_eq!(inventory.documents.len(), 1);
        assert_eq!(inventory.documents[0].locator, completed);
        assert_eq!(inventory.documents[0].bytes, done.as_bytes());
        assert_eq!(inventory.documents[0].sources.len(), 2);
        let mut store = crate::storage::Store::open(parent.path().join("state.sqlite3")).unwrap();
        let repo = store.bind_repository(primary.path()).unwrap();
        store
            .apply_migration(&inventory.capture_plan(repo.clone()).unwrap())
            .unwrap();
        assert!(store.read(&repo, "task", active).unwrap().is_none());
        assert_eq!(store.list(&repo, "task").unwrap().len(), 1);
        assert_eq!(fs::read_to_string(secondary.join(active)).unwrap(), raw);
        assert_eq!(
            fs::read_to_string(primary.path().join(completed)).unwrap(),
            done
        );
    }

    #[test]
    fn captured_reconciled_plan_rejects_changed_original() {
        let primary = git_fixture();
        let parent = tempfile::tempdir().unwrap();
        let secondary = parent.path().join("secondary");
        linked_fixture(primary.path(), &secondary);
        let locator = "backlog/initiatives/shared.md";
        fs::write(primary.path().join(locator), b"new primary").unwrap();
        let inventory = inventory_kind(primary.path(), "initiative").unwrap();
        let mut store = crate::storage::Store::open(parent.path().join("state.sqlite3")).unwrap();
        let repo = store.bind_repository(primary.path()).unwrap();
        let plan = inventory.capture_plan(repo.clone()).unwrap();
        fs::write(secondary.join(locator), b"changed after preview").unwrap();
        assert!(store.apply_migration(&plan).is_err());
        assert!(!store.authority(&repo, "initiative").unwrap());
        assert!(store.list(&repo, "initiative").unwrap().is_empty());
        assert!(inventory.capture_plan(repo).is_err());
    }
    #[test]
    fn absent_primary_source_never_resurrects_a_secondary_copy() {
        let primary = git_fixture();
        let parent = tempfile::tempdir().unwrap();
        let secondary = parent.path().join("secondary");
        linked_fixture(primary.path(), &secondary);
        fs::remove_file(primary.path().join("backlog/initiatives/shared.md")).unwrap();
        let error = inventory_kind(&secondary, "initiative").unwrap_err();
        assert!(error.to_string().contains("no primary source"), "{error}");
    }

    #[test]
    fn explicit_absent_primary_selection_captures_and_applies_retained_source() {
        let primary = git_fixture();
        let parent = tempfile::tempdir().unwrap();
        let secondary = parent.path().join("secondary");
        linked_fixture(primary.path(), &secondary);
        let locator = "backlog/initiatives/shared.md";
        fs::remove_file(primary.path().join(locator)).unwrap();
        let selected_path = secondary.join(locator).canonicalize().unwrap();
        let plan = MigrationResolutionFile {
            version: 1,
            kind: "initiative".into(),
            resolutions: vec![MigrationResolution {
                source_locators: vec![locator.into()],
                target_locator: locator.into(),
                sources: vec![ResolutionSource {
                    path: selected_path.clone(),
                    digest: content_digest(b"shared"),
                    decision: ResolutionDecision::Select,
                }],
                repair_task_id: None,
            }],
            digest: "reviewed-absence".into(),
        };
        let inventory =
            inventory_kind_with_resolutions(primary.path(), "initiative", Some(&plan)).unwrap();
        inventory.validate_native().unwrap();
        let mut store = crate::storage::Store::open(parent.path().join("state.sqlite3")).unwrap();
        let repo = store.bind_repository(primary.path()).unwrap();
        let captured = inventory.capture_plan(repo.clone()).unwrap();
        assert_eq!(store.apply_migration(&captured).unwrap(), 1);
        assert_eq!(
            store
                .read(&repo, "initiative", locator)
                .unwrap()
                .unwrap()
                .content,
            b"shared"
        );
        assert_eq!(fs::read(selected_path).unwrap(), b"shared");
        assert!(!primary.path().join(locator).exists());
    }
    #[test]
    fn new_native_domain_only_in_secondary_is_reviewed_union_with_addition_proof() {
        for committed in [false, true] {
            let parent = tempfile::tempdir().unwrap();
            let primary = parent.path().join("primary");
            let secondary = parent.path().join("secondary");
            fs::create_dir_all(&primary).unwrap();
            git(&primary, &["init", "-q", "-b", "main"]);
            fs::write(primary.join("README.md"), "baseline without backlog").unwrap();
            git(&primary, &["add", "."]);
            git(&primary, &["commit", "-qm", "baseline"]);
            linked_fixture(&primary, &secondary);
            let originals = [
                (
                    "config",
                    "backlog/config.yml",
                    "statuses: [To Do, Done]\ncustom: {keep: true}\n",
                ),
                (
                    "project",
                    "backlog/projects/new.md",
                    "---\nname: New Project\nstatus: Planned\ncustom: [keep, null]\n---\n",
                ),
                (
                    "task",
                    "backlog/tasks/task-7 - New.md",
                    "---\nid: TASK-7\ntitle: New\nstatus: To Do\ncustom: [keep, null]\n---\n",
                ),
            ];
            for (_, locator, bytes) in &originals {
                let path = secondary.join(locator);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, bytes).unwrap();
            }
            if committed {
                git(&secondary, &["add", "."]);
                git(&secondary, &["commit", "-qm", "native backlog additions"]);
            }
            let mut store =
                crate::storage::Store::open(parent.path().join("isolated.sqlite3")).unwrap();
            let repo = store.bind_repository(&primary).unwrap();
            for (kind, locator, bytes) in &originals {
                let inventory = inventory_kind(&secondary, kind);
                if committed {
                    let error = inventory.expect_err("unmerged branch additions need resolution");
                    assert!(
                        error.to_string().contains("reconcile explicitly"),
                        "{error}"
                    );
                    continue;
                }
                let inventory = inventory.unwrap();
                inventory.validate_native().unwrap();
                assert_eq!(inventory.documents.len(), 1);
                assert_eq!(inventory.documents[0].bytes, bytes.as_bytes());
                assert!(inventory.documents[0].sources[0].addition_proof.is_some());
                assert!(inventory.documents[0].sources[0].stale_proof.is_none());
                let from_primary = inventory_kind(&primary, kind).unwrap();
                assert_eq!(inventory.worktrees, from_primary.worktrees);
                assert_eq!(
                    inventory.documents[0].locator,
                    from_primary.documents[0].locator
                );
                assert_eq!(
                    inventory.documents[0].bytes,
                    from_primary.documents[0].bytes
                );
                store
                    .apply_migration(&inventory.capture_plan(repo.clone()).unwrap())
                    .unwrap();
                assert_eq!(
                    store.read(&repo, kind, locator).unwrap().unwrap().content,
                    bytes.as_bytes()
                );
                assert_eq!(fs::read_to_string(secondary.join(locator)).unwrap(), *bytes);
            }
            assert!(!primary.join("backlog").exists());
        }
    }

    #[test]
    fn new_path_with_deleted_primary_task_id_is_not_a_new_addition() {
        let primary = git_fixture();
        let old = "backlog/tasks/task-7 - Old.md";
        fs::create_dir_all(primary.path().join("backlog/tasks")).unwrap();
        let raw = "---\nid: TASK-7\ntitle: Existing identity\n---\n";
        fs::write(primary.path().join(old), raw).unwrap();
        git(primary.path(), &["add", "."]);
        git(primary.path(), &["commit", "-qm", "existing task"]);
        let parent = tempfile::tempdir().unwrap();
        let secondary = parent.path().join("secondary");
        linked_fixture(primary.path(), &secondary);
        fs::remove_file(primary.path().join(old)).unwrap();
        git(primary.path(), &["add", "-u"]);
        git(primary.path(), &["commit", "-qm", "delete task in primary"]);
        fs::remove_file(secondary.join(old)).unwrap();
        let new = "backlog/tasks/task-7 - New-name.md";
        fs::write(secondary.join(new), raw).unwrap();
        let error = inventory_kind(&secondary, "task").unwrap_err();
        assert!(
            error.to_string().contains("TASK-7") && error.to_string().contains(new),
            "{error}"
        );
        assert!(error.to_string().contains("public ID exists"), "{error}");
    }
}
