# Hive — Design

Contracts, reusable components, and workflows. Source of truth for the first vertical slice.

## What Hive Is

Hive is an **observability + control panel** for the dev servers already running on your machine. The thesis: developers don't lack process supervisors — they lack a clear picture of *what's listening, on what SHA, from which worktree of which repo*, and the ability to clean it up.

A typical day produces chaos: three worktrees of one project, each with its own `start_dev.sh` competing for the same port, plus an orphaned `npm run dev` from yesterday nobody remembered to kill. Hive's job is to make that legible:

- **What's listening right now?** (continuous `lsof` scan; primary view.)
- **Whose was that?** (process group → cwd → worktree → repo → SHA at spawn vs newest on the default branch.)
- **Make it stop.** (kill the PGID, not just the PID.)
- **Start a clean one.** (declared-workflow registry: pick a workflow the developer already wrote down, spawn it under our supervision.)

The supervisor (spawn/stop) is the smallest part of the product. The observability features (live scanner, attribution, SHA drift) are why you'd open the app.

## Crate Layout

```
hive/
├── Cargo.toml                # workspace
├── crates/
│   ├── hive-core/            # domain types + business logic + traits
│   │   ├── src/
│   │   │   ├── types.rs
│   │   │   ├── db.rs         # trait + rusqlite impl
│   │   │   ├── workflow.rs   # detector trait + per-source impls
│   │   │   ├── supervisor.rs # spawn/stop, log streaming, PGID tracking
│   │   │   ├── port.rs       # multi-port observer (PGID → ports)
│   │   │   ├── git.rs        # git_inspector
│   │   │   ├── scan.rs       # listener_scanner + orphan_reconciler
│   │   │   └── events.rs     # broadcast bus
│   │   └── migrations/
│   └── hive-tauri/           # Tauri shell — thin
│       └── src/main.rs       # command/event registration
└── frontend/                 # Vite + TS, no framework yet
```

Two crates so `hive-core` stays testable without Tauri/webview overhead. Every OS-touching component is behind a trait so unit tests use in-memory fakes.

## Workflow Sources (the detector menu)

Hive picks up workflows the developer **already declared** in their repo. There is no inference beyond what the repo's own files explicitly say. The detector composite runs every applicable source detector, merges results by `(name, command)`, and surfaces what each Service came from so the user can choose.

All sources are first-class, equally weighted. The developer chose the declaration style; Hive respects it.

### Explicit task / process declarations
| Source | Reads | Emits |
|---|---|---|
| **Procfile** | `Procfile`, `Procfile.dev` at root | One Service per line (`name: command`) |
| **Mise tasks** | `[tasks.*]` in `mise.toml` / `.mise.toml` | One Service per task; `command = mise run <name>` |
| **Makefile** | Targets in `Makefile` matching a configurable allow-list (`dev`, `start`, `run`, `serve`, `up`, `web`, `api`, `worker`, …) | One Service per matched target; `command = make <target>` |
| **Justfile** | Recipes in `justfile` matching the same allow-list | One Service per recipe; `command = just <recipe>` |
| **Taskfile** | Tasks in `Taskfile.yml` (go-task) | One Service per task; `command = task <name>` |
| **Shell scripts** | Executable files in `scripts/` and `bin/` matching `start*`, `dev*`, `run*`, `serve*` | One Service per script; `command = ./<rel-path>`, `source_file` records the path |

### Manifest-derived
| Source | Reads | Emits |
|---|---|---|
| **Python** | `pyproject.toml` — `[project.scripts]`, `[tool.poetry.scripts]`, plus `[tool.hatch.envs.*.scripts]` if present | One Service per entry. Prefix from lockfile: `uv run` if `uv.lock`, else `poetry run` if `poetry.lock`, else direct invocation |
| **Node** | `package.json#scripts` — `dev`, `start`, `serve` (root and each workspace member) | One Service per script. PM from root lockfile (`pnpm-lock.yaml` → pnpm, `yarn.lock` → yarn, `bun.lockb` → bun, else npm) |
| **Cargo** | `Cargo.toml` — `[[bin]]` entries, plus workspace members with bins | One Service per binary; `command = cargo run -p <pkg> --bin <name>` |
| **Go** | `go.mod` plus `cmd/<name>/main.go` convention | One Service per entry-point; `command = go run ./cmd/<name>`. If `.air.toml` present, swap to `air` |

### Container orchestration
| Source | Reads | Emits |
|---|---|---|
| **Docker Compose** | `docker-compose.yml`, `compose.yaml`, `compose.yml` at root | One opaque Service `compose` running `docker compose up`. v1 opaque; v1.5 optional per-container expansion |

### Workspace expansion
Each manifest-derived detector also walks workspace declarations:
- `package.json#workspaces`, `pnpm-workspace.yaml`
- `Cargo.toml#workspace.members`
- `pyproject.toml#[tool.uv.workspace]`
- `go.work` sub-modules

Resulting Services get `cwd` set to the sub-package path relative to the worktree root, and `name` from the package's declared name (falling back to directory name).

### Overlays (post-processors)
Applied to detected services after collection. Run in order:
- **MiseWrap** (default-on when `.mise.toml` or `.tool-versions` is present at the repo root): wraps every command in `mise exec -- <cmd>` so per-repo tool versions activate. Recognizes `asdf` as a fallback when mise isn't installed. Per-service "raw shell" toggle to bypass.
- **TurborepoOverlay** (when `turbo.json` present): rewrites Node Services to `turbo run <script> --filter=<name>`.
- **NxOverlay** (when `nx.json` + per-project `project.json` present): rewrites to `nx run <project>:<target>`.

### Merging rules
- All applicable detectors run; results flatten into one list.
- Dedup by `(name, command)` exact match.
- Two declarations with the same name and different commands → both surface. UI shows the source per Service; user disables whichever they don't want. (Common case: a Makefile `dev` target *and* `package.json#scripts.dev` — surface both, attribute each, let user pick.)
- No source is authoritative — Procfile is one input among many, not an override.

### Re-detect semantics
On every re-detect (manual button, or automatic on repo focus), `db.add_detected_services` only INSERTs new `(repo_id, name)` pairs. Existing rows keep their `command`, `cwd`, `env`, `disabled`, `port_hint`. Services that no longer appear in detection stay in the DB if they have historical runs; otherwise they're soft-removed with a UI cleanup prompt.

## Domain Types

```rust
pub struct RepoId(pub i64);
pub struct WorktreeId(pub i64);
pub struct ServiceId(pub i64);
pub struct RunId(pub i64);

// A Repo is the canonical project, identified by `git rev-parse --git-common-dir`.
// Multiple Worktrees can point at it (different branches / SHAs / parallel checkouts).
pub struct Repo {
    pub id: RepoId,
    pub common_dir: PathBuf,                   // canonical identity (the shared .git dir)
    pub primary_path: PathBuf,                 // the first worktree the user added
    pub name: String,
    pub default_branch: String,
    pub remote_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct Worktree {
    pub id: WorktreeId,
    pub repo_id: RepoId,
    pub path: PathBuf,                         // absolute path to the worktree root
    pub branch: Option<String>,                // None if detached HEAD
    pub head_sha: String,                      // refreshed by Refresh Git
    pub added_at: DateTime<Utc>,
}

pub enum WorkflowSource {
    Procfile,
    MiseTask,
    Makefile,
    Justfile,
    Taskfile,
    ShellScript,
    PythonScript,
    NodeScript,
    CargoBin,
    GoEntrypoint,
    DockerCompose,
    Custom,
}

pub struct Service {
    pub id: ServiceId,
    pub repo_id: RepoId,
    pub name: String,
    pub command: String,                       // shell command (after any MiseWrap / Turbo / Nx rewriting)
    pub cwd: PathBuf,                          // relative to worktree root; "." for root services
    pub env: BTreeMap<String, String>,         // explicit overrides; deterministic order
    pub port_hint: Option<u16>,                // best-effort parse; null when unsure
    pub source: WorkflowSource,                // where this was detected
    pub source_file: Option<PathBuf>,          // "Makefile", "scripts/start_lyon.sh", "pyproject.toml", …
    pub disabled: bool,                        // user hid it; detector won't re-show on rescan
}

pub enum RunStatus {
    Starting,
    Running,
    Exited { code: i32 },                      // process returned naturally (any code)
    Killed,                                    // we sent SIGTERM/SIGKILL via stop()
    Crashed { reason: String },                // unexpected termination
}

pub struct Run {
    pub id: RunId,
    pub service_id: ServiceId,
    pub worktree_id: WorktreeId,
    pub pid: u32,                              // leader (immediate child)
    pub pgid: i32,                             // process group (== leader pid since we setsid). Used for kill(-pgid).
    pub sha: String,                           // git HEAD captured at spawn (worktree's HEAD)
    pub started_at: DateTime<Utc>,
    pub status: RunStatus,
    pub ports_observed: Vec<u16>,              // every TCP port any PID in the PGID is listening on
    pub log_path: PathBuf,                     // <data_dir>/logs/<run_id>.log
    pub ended_at: Option<DateTime<Utc>>,
}

// Combined view: per-worktree HEAD plus repo-level remote/default branch metadata.
pub struct GitState {
    pub worktree_id: WorktreeId,
    pub current_sha: String,
    pub current_branch: Option<String>,
    pub default_branch: String,
    pub remote_url: Option<String>,
    pub newest_sha: Option<String>,            // origin/<current_branch> after fetch
    pub fetched_at: Option<DateTime<Utc>>,
}

// A single TCP listener on the local machine.
pub struct LocalListener {
    pub pid: u32,
    pub pgid: i32,
    pub port: u16,
    pub command_name: String,                  // lsof COMMAND column
    pub cwd: Option<PathBuf>,                  // from `lsof -p <pid>` CWD entry; powers worktree attribution
}

pub struct ManagedRun {
    pub run: Run,
    pub listeners: Vec<LocalListener>,         // every listener whose PGID matches run.pgid
}

pub struct OrphanListener {
    pub listener: LocalListener,
    pub guessed_worktree_id: Option<WorktreeId>,   // matched by cwd prefix
    pub guessed_service_id: Option<ServiceId>,     // matched by port_hint or command_name
}

pub struct ReconcileReport {
    pub managed: Vec<ManagedRun>,              // runs we own, with their observed ports
    pub orphans: Vec<OrphanListener>,          // listening but unattributed; with best-guess attribution
    pub stale_runs: Vec<Run>,                  // run row exists, PGID is gone (process died, status not yet flushed)
}
```

## Reusable Components

Each module exposes a trait. SQLite/lsof/git impls are the default; in-memory fakes drive tests.

### `db` — persistence
```rust
pub trait Db: Send + Sync {
    // Repos
    fn upsert_repo(&self, common_dir: &Path, primary_path: &Path, name: &str,
                   branch: &str, remote: Option<&str>) -> Result<RepoId>;
    fn list_repos(&self) -> Result<Vec<Repo>>;
    fn remove_repo(&self, id: RepoId) -> Result<()>;
    fn find_repo_by_common_dir(&self, common_dir: &Path) -> Result<Option<Repo>>;

    // Worktrees
    fn upsert_worktree(&self, repo_id: RepoId, path: &Path,
                       branch: Option<&str>, head_sha: &str) -> Result<WorktreeId>;
    fn list_worktrees(&self, repo_id: RepoId) -> Result<Vec<Worktree>>;
    fn remove_worktree(&self, id: WorktreeId) -> Result<()>;

    // Services
    fn add_detected_services(&self, repo_id: RepoId, svcs: Vec<NewService>) -> Result<Vec<Service>>;
    // ^ INSERTs only new (repo_id, name) pairs. Never overwrites user edits or `disabled` rows.
    fn list_services(&self, repo_id: RepoId, include_disabled: bool) -> Result<Vec<Service>>;
    fn update_service(&self, svc: &Service) -> Result<()>;
    fn set_service_disabled(&self, id: ServiceId, disabled: bool) -> Result<()>;

    // Runs
    fn record_run(&self, run: NewRun) -> Result<RunId>;
    fn update_run_pid_pgid(&self, id: RunId, pid: u32, pgid: i32) -> Result<()>;
    fn update_run_status(&self, id: RunId, status: RunStatus) -> Result<()>;
    fn set_run_ports(&self, id: RunId, ports: &[u16]) -> Result<()>;
    fn active_runs(&self) -> Result<Vec<Run>>;
    fn active_runs_for_worktree(&self, id: WorktreeId) -> Result<Vec<Run>>;
    fn runs_for_service(&self, id: ServiceId,
                        worktree: Option<WorktreeId>, limit: usize) -> Result<Vec<Run>>;
}
```

### `workflow_detector` — repo → declared workflows
```rust
pub trait WorkflowDetector: Send + Sync {
    fn detect(&self, repo_root: &Path) -> Result<Vec<DetectedService>>;
}

pub struct DetectedService {
    pub name: String,
    pub command: String,
    pub cwd: PathBuf,
    pub port_hint: Option<u16>,
    pub source: WorkflowSource,
    pub source_file: Option<PathBuf>,
}
```
Default impl is a composite over per-source detectors (see Workflow Sources above), then applies overlays (MiseWrap → TurborepoOverlay → NxOverlay).

### `supervisor` — process lifecycle
```rust
pub trait Supervisor: Send + Sync {
    async fn spawn(&self, svc: &Service, worktree_path: &Path, log_path: &Path)
        -> Result<SpawnedRun>;
    async fn stop(&self, run_id: RunId, grace: Duration) -> Result<()>;
    fn subscribe(&self) -> broadcast::Receiver<SupervisorEvent>;
}

pub struct SpawnedRun {
    pub run_id: RunId,
    pub pid: u32,                              // leader (immediate child)
    pub pgid: i32,                             // PGID == leader pid (we setsid)
}

pub enum SupervisorEvent {
    Started { run_id: RunId, pid: u32, pgid: i32 },
    LogLine { run_id: RunId, stream: Stream, line: String },
    Exited { run_id: RunId, code: i32 },       // leader exited
    Crashed { run_id: RunId, reason: String },
}

pub enum Stream { Stdout, Stderr }
```
- Spawns via `tokio::process::Command` with a new process group (`setsid` on Unix); the child PID becomes the PGID.
- `stop()` sends SIGTERM to `-pgid` → waits `grace` → SIGKILL to `-pgid`. Kills the entire tree (Python subprocesses, `npm` → Vite → esbuild, shell scripts spawning multiple servers).
- A run is "exited" when the **leader** (immediate child) exits. Any children that orphaned to init aren't supervised here — they show up as orphan listeners in the next scan, which is the intended behavior.

### `port_observer` — PGID → listening ports (multi-port)
```rust
pub async fn observe_ports(pgid: i32, deadline: Instant) -> Result<Vec<u16>>;
```
Polls `lsof -iTCP -sTCP:LISTEN -P -n -F pgcnPL` filtered to the PGID's PIDs every 250ms until the deadline. Returns every port any PID in the PGID is listening on.

Single-port is the easy case; multi-port (shell scripts spawning FastAPI + a Go service, docker-compose running web + worker + db, Node monorepo concurrently running two dev servers under one process) is the case to get right.

### `git_inspector` — SHA + remote state
```rust
pub trait GitInspector: Send + Sync {
    fn common_dir(&self, path: &Path) -> Result<PathBuf>;
    fn head_sha(&self, worktree: &Path) -> Result<String>;
    fn current_branch(&self, worktree: &Path) -> Result<Option<String>>;
    fn default_branch(&self, worktree: &Path) -> Result<String>;
    fn remote_url(&self, worktree: &Path) -> Result<Option<String>>;
    fn list_worktrees(&self, any_worktree: &Path) -> Result<Vec<WorktreeListing>>;
    async fn fetch_and_remote_sha(&self, worktree: &Path, branch: &str) -> Result<String>;
}

pub struct WorktreeListing {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head_sha: String,
}
```
Shells out to `git`. No libgit2 — preserves the user's credentials, hooks, SSH agent setup.

### `listener_scanner` — system-wide listening ports
```rust
pub trait ListenerScanner: Send + Sync {
    async fn scan(&self) -> Result<Vec<LocalListener>>;
}
```
Wraps `lsof -iTCP -sTCP:LISTEN -P -n -F pgcnPL` (note `g` field → PGID). For each PID seen, also issues `lsof -p <pid> -F n` to capture `cwd`, which powers worktree attribution.

### `orphan_reconciler` — runs ⨝ listeners (pure)
```rust
pub fn reconcile(
    runs: &[Run],
    listeners: &[LocalListener],
    worktrees: &[Worktree],
    services: &[Service],
) -> ReconcileReport;
```
Joins by **PGID** — a managed run owns every listener whose `pgid` matches `run.pgid`. For unmatched listeners (orphans), attempts attribution:
1. `cwd` is a prefix of a known worktree's path → `guessed_worktree_id`.
2. `port_hint` of a Service matches → `guessed_service_id`.
3. `command_name` ≈ a known Service's command leading token → tiebreaker.

Pure function, no IO; fully unit-tested with hand-crafted inputs.

### `events` — internal fan-out
A single `tokio::sync::broadcast::Sender<HiveEvent>` owned by the app. Tauri layer forwards to JS.

```rust
pub enum HiveEvent {
    RepoAdded(Repo),
    RepoRemoved(RepoId),
    WorktreeAdded(Worktree),
    WorktreeRemoved(WorktreeId),
    RunStarted(Run),
    RunLogLine { run_id: RunId, stream: Stream, line: String },
    RunPortsObserved { run_id: RunId, ports: Vec<u16> },
    RunStopped { run_id: RunId, status: RunStatus },
    GitRefreshed { worktree_id: WorktreeId, state: GitState },
    ShaDriftDetected { worktree_id: WorktreeId, running_sha: String, newest_sha: String },
    ListenersUpdated(ReconcileReport),         // primary live-view feed
}
```

## Workflows (multi-step orchestrations)

These live in `hive-core` as free functions that compose the components above. The Tauri layer is a thin call site.

### Add Repo (or adopt a new worktree of a known Repo)
1. User picks folder (Tauri `dialog::open`).
2. `git_inspector.common_dir(path)` — fail fast if not a git repo. The common-dir is the Repo identity.
3. `db.find_repo_by_common_dir(...)`:
   - If present, skip to step 7 — this is a new Worktree of a known Repo.
   - If absent, proceed.
4. `git_inspector.default_branch(path)`, `remote_url(path)`.
5. `db.upsert_repo(...)` → `RepoId`.
6. `workflow_detector.detect(path)` → services attached via `db.add_detected_services(...)`. Composite walks every Workflow Source and applies overlays.
7. `git_inspector.list_worktrees(path)` → enumerate every worktree of this Repo. For each: `db.upsert_worktree(...)`.
8. Emit `RepoAdded` (if new) + `WorktreeAdded` for each new worktree.
9. Spawn background task: `git_inspector.fetch_and_remote_sha(...)` per worktree; emit `GitRefreshed`.

### Start Service (in a specific Worktree)
Caller passes `(service_id, worktree_id)`. Concurrent runs of the same service across **different** worktrees are expected. Concurrent runs in the **same** worktree are refused.

1. Verify the service is not `disabled` — refuse with a clear error if so.
2. `db.active_runs_for_worktree(worktree_id)` — refuse if a Run for this service already exists with no `ended_at`.
3. Resolve `worktree.path`. Spawn cwd = `worktree.path + service.cwd`.
4. `git_inspector.head_sha(worktree.path)` — snapshot SHA at spawn (the worktree's HEAD).
5. Pre-allocate `RunId` via `db.record_run({ service_id, worktree_id, sha, log_path, status: Starting, pid: 0, pgid: 0 })`.
6. `supervisor.spawn(svc, worktree.path, log_path)` → `{ pid, pgid }`.
7. `db.update_run_pid_pgid(run_id, pid, pgid)` and emit `RunStarted`.
8. Background task A: forward `SupervisorEvent`s to the event bus (filtered by `run_id`).
9. Background task B: from 500ms post-spawn, poll `port_observer.observe_ports(pgid, now + 10s)`. After initial discovery, keep rescanning every 5s for the run's lifetime to catch ports bound late (a script that opens its second listener after warm-up). On change, `db.set_run_ports(...)` and emit `RunPortsObserved`.
10. On `SupervisorEvent::Exited` or `Crashed`: `db.update_run_status(...)` and emit `RunStopped`.

### Stop Run
Caller passes `run_id` directly. A service can have multiple concurrent runs (one per worktree), so `service_id` alone is ambiguous.

1. `supervisor.stop(run_id, Duration::from_secs(5))`.
2. Supervisor emits `Exited` / `Crashed`; the existing forwarder records final status.

### Refresh Git (background, every 5 min per worktree)
1. `git_inspector.head_sha(worktree.path)` — HEAD may have advanced.
2. `git_inspector.current_branch(worktree.path)` — branch may have changed.
3. `git_inspector.fetch_and_remote_sha(worktree.path, branch)` — coalesce one fetch per Repo per cycle (worktrees share the common-dir).
4. Update cached `GitState` per worktree; emit `GitRefreshed`.
5. For each active run in this worktree: if `run.sha != newest_sha`, emit `ShaDriftDetected`.

### Scan Worktrees (on repo focus, and after the user runs `git worktree add` externally)
1. For each Repo: `git_inspector.list_worktrees(repo.primary_path)`.
2. Diff against `db.list_worktrees(repo_id)`:
   - New paths → `db.upsert_worktree(...)` + emit `WorktreeAdded`.
   - Missing paths with no active runs → `db.remove_worktree(...)` + emit `WorktreeRemoved`. If active runs reference it, mark hidden and resolve when the runs stop.
3. If `primary_path` was removed, promote another worktree to primary.

### Scan Listeners (continuous — every 5s while UI is focused, every 30s in background)
The headline observability loop.

1. `listener_scanner.scan()` → `Vec<LocalListener>` (with PGID + cwd per listener).
2. `db.active_runs()`, `db.list_worktrees(_)` (all repos), `db.list_services(_, true)`.
3. `orphan_reconciler.reconcile(...)` joins by PGID and attributes orphans.
4. For each `ManagedRun` whose port set differs from the stored `Run.ports_observed`: `db.set_run_ports(...)`; emit `RunPortsObserved`.
5. For each `stale_run`: mark `Crashed { reason: "PGID disappeared between scans" }`.
6. Emit `ListenersUpdated(report)`.

## Tauri Contract

### Commands (JS → Rust, request/response)
| Command | Returns |
|---|---|
| `add_repo(path: string)` | `{ repo: Repo, worktrees: Worktree[], services: Service[] }` |
| `list_repos()` | `Repo[]` |
| `remove_repo(repo_id)` | `void` |
| `list_worktrees(repo_id)` | `Worktree[]` |
| `scan_worktrees(repo_id)` | `Worktree[]` |
| `list_services(repo_id, include_disabled)` | `Service[]` |
| `update_service(svc)` | `Service` |
| `redetect_services(repo_id)` | `Service[]` |
| `set_service_disabled(service_id, disabled)` | `void` |
| `start_service(service_id, worktree_id)` | `Run` |
| `stop_run(run_id)` | `void` |
| `active_runs()` | `Run[]` |
| `active_runs_for_worktree(worktree_id)` | `Run[]` |
| `run_logs(run_id, tail_lines)` | `string[]` |
| `git_state(worktree_id)` | `GitState` |
| `refresh_git(worktree_id)` | `GitState` |
| `scan_listeners()` | `ReconcileReport` |

### Events (Rust → JS, push)
Same names/shapes as `HiveEvent`. Frontend subscribes once at boot, mutates view model on event. `ListenersUpdated` is the highest-frequency event and the centerpiece of the primary view.

Frontend stores nothing authoritative. On window focus or reconnect it calls `list_repos` + `active_runs` + `scan_listeners` to resync.

## Persistence (SQLite)

```sql
CREATE TABLE repos (
    id INTEGER PRIMARY KEY,
    common_dir TEXT NOT NULL UNIQUE,           -- `git rev-parse --git-common-dir`
    primary_path TEXT NOT NULL,
    name TEXT NOT NULL,
    default_branch TEXT NOT NULL,
    remote_url TEXT,
    created_at TEXT NOT NULL                   -- ISO-8601 UTC
);

CREATE TABLE worktrees (
    id INTEGER PRIMARY KEY,
    repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    path TEXT NOT NULL UNIQUE,
    branch TEXT,
    head_sha TEXT NOT NULL,
    added_at TEXT NOT NULL
);

CREATE TABLE services (
    id INTEGER PRIMARY KEY,
    repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    command TEXT NOT NULL,
    cwd TEXT NOT NULL DEFAULT '.',
    env_json TEXT NOT NULL DEFAULT '{}',
    port_hint INTEGER,
    source TEXT NOT NULL,                      -- WorkflowSource as string
    source_file TEXT,                          -- "Makefile", "scripts/start_lyon.sh", …
    disabled INTEGER NOT NULL DEFAULT 0,
    UNIQUE (repo_id, name)
);

CREATE TABLE runs (
    id INTEGER PRIMARY KEY,
    service_id INTEGER NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    worktree_id INTEGER NOT NULL REFERENCES worktrees(id) ON DELETE CASCADE,
    pid INTEGER NOT NULL DEFAULT 0,            -- 0 between record_run and spawn-complete
    pgid INTEGER NOT NULL DEFAULT 0,
    sha TEXT NOT NULL,
    started_at TEXT NOT NULL,
    status_json TEXT NOT NULL,                 -- RunStatus serialized
    ports_json TEXT NOT NULL DEFAULT '[]',     -- Vec<u16> serialized
    log_path TEXT NOT NULL,
    ended_at TEXT
);

CREATE INDEX runs_active ON runs(worktree_id, service_id) WHERE ended_at IS NULL;
CREATE INDEX runs_by_service ON runs(service_id, started_at DESC);
CREATE INDEX worktrees_by_repo ON worktrees(repo_id);
```

Logs: append-only files at `<data_dir>/logs/<run_id>.log`, capped at 10 MB per active run (writer stops past the cap and notes truncation).

`<data_dir>` resolution:
- macOS: `~/Library/Application Support/hive/`
- Linux: `$XDG_DATA_HOME/hive/` or `~/.local/share/hive/`
- (Windows out of scope for v1.)

## Open Questions

- **Orphan adoption** — promote an orphan to a managed Run? We could record PID/PGID/start time/observed ports retroactively and let the user stop it like any managed run. We *can't* attach to its stdout post-hoc. Recommend: v1.5 — adopt for `stop` + observation, mark `log_path = "(adopted; no logs)"`.
- **Multi-process visibility under one Service** — when `start_lyon.sh` spawns FastAPI + a Go service, the UI shows one Service row with both ports listed underneath. Good enough for v1, or surface them as visually distinct sub-processes (would require per-PID-in-PGID inspection)? Recommend: one row, ports list visible; sub-processes in v1.5.
- **Restart-on-new-SHA** — auto toggle per service, or always manual? Recommend manual button + visible drift badge in v1.
- **Cross-platform** — Linux ~free (lsof, setsid, sh, /proc/<pid>/cwd all present). Windows is real work — `Get-NetTCPConnection` / `netstat`, job objects instead of process groups, `cmd.exe` shell. v2; keep traits clean.
- **Detector match lists** — Makefile/Justfile target allow-list (`dev`, `start`, `run`, `serve`, `up`, `web`, `api`, `worker`, …) lives where? Recommend: hard-coded in v1, configurable via app settings in v1.5.
