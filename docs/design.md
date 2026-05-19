# Hive — Design

Contracts, reusable components, and workflows. Source of truth for the first vertical slice.

## Crate Layout

```
hive/
├── Cargo.toml                # workspace
├── crates/
│   ├── hive-core/            # domain types + business logic + traits
│   │   ├── src/
│   │   │   ├── types.rs
│   │   │   ├── db.rs         # trait + rusqlite impl
│   │   │   ├── workflow.rs   # detector trait + per-kind impls
│   │   │   ├── supervisor.rs # spawn/stop, log streaming
│   │   │   ├── port.rs       # port_observer
│   │   │   ├── git.rs        # git_inspector
│   │   │   ├── scan.rs       # listener_scanner + orphan_reconciler
│   │   │   └── events.rs     # broadcast bus
│   │   └── migrations/
│   └── hive-tauri/           # Tauri shell — thin
│       └── src/main.rs       # command/event registration
└── frontend/                 # Vite + TS, no framework yet
```

Why two crates: `hive-core` stays testable without Tauri/webview overhead. Every OS-touching component is behind a trait so unit tests use in-memory fakes.

## Domain Types (`hive-core/src/types.rs`)

```rust
pub struct RepoId(pub i64);
pub struct WorktreeId(pub i64);
pub struct ServiceId(pub i64);
pub struct RunId(pub i64);

// A Repo is the canonical project, identified by `git rev-parse --git-common-dir`.
// Multiple Worktrees can point at it (different branches, different SHAs, parallel checkouts).
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
    pub head_sha: String,                      // refreshed periodically
    pub added_at: DateTime<Utc>,
}

pub enum WorkflowKind {
    Procfile,
    NpmScripts,
    DockerCompose,
    Custom,             // hand-entered by user
}

pub struct Service {
    pub id: ServiceId,
    pub repo_id: RepoId,
    pub name: String,                          // "web", "api", "worker"
    pub command: String,                       // raw command string, run via /bin/sh -c
    pub cwd: PathBuf,                          // relative to repo root; "." by default
    pub env: BTreeMap<String, String>,         // explicit overrides; deterministic order
    pub port_hint: Option<u16>,                // null if detection couldn't infer
    pub source: WorkflowKind,                  // where this came from at detection time
}

pub enum RunStatus {
    Starting,                                  // spawned, not yet observed running
    Running,                                   // process alive
    Exited { code: i32 },                      // returned naturally (any code)
    Killed,                                    // we sent SIGTERM/SIGKILL via stop()
    Crashed { reason: String },                // unexpected signal / supervisor lost track
}

pub struct Run {
    pub id: RunId,
    pub service_id: ServiceId,
    pub worktree_id: WorktreeId,               // which checkout this run is bound to
    pub pid: u32,
    pub sha: String,                           // git HEAD captured at spawn (worktree's HEAD)
    pub started_at: DateTime<Utc>,
    pub status: RunStatus,
    pub port_observed: Option<u16>,            // populated by port_observer; null until seen
    pub log_path: PathBuf,                     // <data_dir>/logs/<run_id>.log
    pub ended_at: Option<DateTime<Utc>>,
}

// Combined view: per-worktree HEAD plus the repo's tracked remote / default branch.
// UI usually wants both together to render the drift badge.
pub struct GitState {
    pub worktree_id: WorktreeId,
    pub current_sha: String,                   // worktree's HEAD
    pub current_branch: Option<String>,
    pub default_branch: String,                // repo-level
    pub remote_url: Option<String>,            // repo-level
    pub newest_sha: Option<String>,            // origin/<current_branch> after fetch
    pub fetched_at: Option<DateTime<Utc>>,
}

pub struct LocalListener {
    pub pid: u32,
    pub port: u16,
    pub command_name: String,                  // lsof COMMAND column (truncated to ~15 chars)
}

pub struct ReconcileReport {
    pub matched: Vec<(Run, LocalListener)>,    // our runs that own a port
    pub orphans: Vec<LocalListener>,           // listening, but not in our runs table
    pub stale_runs: Vec<Run>,                  // run row exists, PID is gone
}
```

## Reusable Components

Each module exposes a trait. SQLite/lsof/git impls are the default; in-memory fakes drive tests.

### `db` — persistence
```rust
pub trait Db: Send + Sync {
    // Repos (one per project, keyed by common-dir)
    fn upsert_repo(&self, common_dir: &Path, primary_path: &Path, name: &str,
                   branch: &str, remote: Option<&str>) -> Result<RepoId>;
    fn list_repos(&self) -> Result<Vec<Repo>>;
    fn remove_repo(&self, id: RepoId) -> Result<()>;
    fn find_repo_by_common_dir(&self, common_dir: &Path) -> Result<Option<Repo>>;

    // Worktrees (many per Repo)
    fn upsert_worktree(&self, repo_id: RepoId, path: &Path,
                       branch: Option<&str>, head_sha: &str) -> Result<WorktreeId>;
    fn list_worktrees(&self, repo_id: RepoId) -> Result<Vec<Worktree>>;
    fn remove_worktree(&self, id: WorktreeId) -> Result<()>;

    // Services (defined on Repo — same workflow across all its worktrees)
    fn upsert_services(&self, repo_id: RepoId, svcs: Vec<NewService>) -> Result<Vec<Service>>;
    fn list_services(&self, repo_id: RepoId) -> Result<Vec<Service>>;
    fn update_service(&self, svc: &Service) -> Result<()>;

    // Runs (per Service per Worktree — concurrent runs across worktrees are expected)
    fn record_run(&self, run: NewRun) -> Result<RunId>;
    fn update_run_status(&self, id: RunId, status: RunStatus) -> Result<()>;
    fn set_run_port(&self, id: RunId, port: u16) -> Result<()>;
    fn active_runs(&self) -> Result<Vec<Run>>;                       // across all worktrees
    fn active_runs_for_worktree(&self, id: WorktreeId) -> Result<Vec<Run>>;
    fn runs_for_service(&self, id: ServiceId,
                        worktree: Option<WorktreeId>, limit: usize) -> Result<Vec<Run>>;
}
```
SQLite-backed default impl. Schema in `crates/hive-core/migrations/0001_init.sql`.

### `workflow_detector` — repo → service candidates
```rust
pub trait WorkflowDetector: Send + Sync {
    fn detect(&self, repo_root: &Path) -> Result<Vec<DetectedService>>;
}

pub struct DetectedService {
    pub name: String,
    pub command: String,
    pub port_hint: Option<u16>,
    pub source: WorkflowKind,
}
```
The default impl composes per-kind detectors and runs them in priority order, stopping at the first that returns a non-empty list:
1. `ProcfileDetector` — parses `Procfile` lines `name: command`.
2. `DockerComposeDetector` — opaque: returns a single `Service { name: "compose", command: "docker compose up", ... }`.
3. `NpmScriptsDetector` — reads `package.json`; surfaces `dev`, `start`, `serve` if present (one service each, named `npm:<script>`).
4. Returns empty if none match — user adds services manually via Custom.

### `supervisor` — process lifecycle
```rust
pub trait Supervisor: Send + Sync {
    async fn spawn(&self, svc: &Service, repo_root: &Path, log_path: &Path) -> Result<SpawnedRun>;
    async fn stop(&self, run_id: RunId, grace: Duration) -> Result<()>;
    fn subscribe(&self) -> broadcast::Receiver<SupervisorEvent>;
}

pub struct SpawnedRun {
    pub run_id: RunId,        // assigned by caller, passed in
    pub pid: u32,
}

pub enum SupervisorEvent {
    Started { run_id: RunId, pid: u32 },
    LogLine { run_id: RunId, stream: Stream, line: String },
    Exited { run_id: RunId, code: i32 },
    Crashed { run_id: RunId, reason: String },
}

pub enum Stream { Stdout, Stderr }
```
- Spawns via `tokio::process::Command` with a new process group (`setsid` on Unix) so `stop()` can kill the tree with `kill(-pgid, ...)`.
- Stop semantics: SIGTERM → wait `grace` → SIGKILL if still alive. Always emits `Exited` or `Crashed` (never silent).
- Log writer task: tee stdout/stderr to `log_path` and to `LogLine` events. Bounded broadcast channel; slow consumers see `RecvError::Lagged` rather than blocking the writer.

### `port_observer` — PID → port
```rust
pub async fn observe_port(pid: u32, deadline: Instant) -> Result<Option<u16>>;
```
Polls `lsof -iTCP -sTCP:LISTEN -a -p <pid> -P -n` every 250ms until a port appears or deadline. Returns the lowest port if the process has multiple (most servers bind exactly one; multi-port servers are edge cases).

### `git_inspector` — SHA + remote state
```rust
pub trait GitInspector: Send + Sync {
    fn common_dir(&self, path: &Path) -> Result<PathBuf>;            // `git rev-parse --git-common-dir`
    fn head_sha(&self, worktree: &Path) -> Result<String>;
    fn current_branch(&self, worktree: &Path) -> Result<Option<String>>;
    fn default_branch(&self, worktree: &Path) -> Result<String>;
    fn remote_url(&self, worktree: &Path) -> Result<Option<String>>;
    fn list_worktrees(&self, any_worktree: &Path) -> Result<Vec<WorktreeListing>>;   // parses `git worktree list --porcelain`
    async fn fetch_and_remote_sha(&self, worktree: &Path, branch: &str) -> Result<String>;
}

pub struct WorktreeListing {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head_sha: String,
}
```
Shells out to `git`. No libgit2 dep — simpler, matches the user's actual git config (credentials, hooks, SSH agent).

### `listener_scanner` — system-wide listening ports
```rust
pub trait ListenerScanner: Send + Sync {
    async fn scan(&self) -> Result<Vec<LocalListener>>;
}
```
Wraps `lsof -iTCP -sTCP:LISTEN -P -n -F pcnPL`. Filters to current user's UID. `-F` machine-parseable output (line-prefixed by field code) — never parse the human table.

### `orphan_reconciler` — runs ⨝ listeners (pure)
```rust
pub fn reconcile(runs: &[Run], listeners: &[LocalListener]) -> ReconcileReport;
```
No IO. Fully covered by unit tests with hand-crafted inputs.

### `events` — internal fan-out
A single `tokio::sync::broadcast::Sender<HiveEvent>` owned by the app. Components publish; Tauri layer subscribes and forwards to JS. Frontend never connects directly to a component.

```rust
pub enum HiveEvent {
    RepoAdded(Repo),
    RepoRemoved(RepoId),
    WorktreeAdded(Worktree),
    WorktreeRemoved(WorktreeId),
    RunStarted(Run),
    RunLogLine { run_id: RunId, stream: Stream, line: String },
    RunPortObserved { run_id: RunId, port: u16 },
    RunStopped { run_id: RunId, status: RunStatus },
    GitRefreshed { worktree_id: WorktreeId, state: GitState },
    ShaDriftDetected { worktree_id: WorktreeId, running_sha: String, newest_sha: String },
    OrphansUpdated(ReconcileReport),
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
6. `workflow_detector.detect(path)` → services attached to the Repo via `db.upsert_services(...)`.
7. `git_inspector.list_worktrees(path)` → enumerate every worktree of this Repo. For each: `db.upsert_worktree(...)`.
8. Emit `RepoAdded` (if new) + `WorktreeAdded` for each new worktree.
9. Spawn background task: `git_inspector.fetch_and_remote_sha(...)` per worktree; emit `GitRefreshed`.

### Start Service (in a specific Worktree)
Caller passes `(service_id, worktree_id)`. Concurrent runs of the same service across **different** worktrees are expected and allowed. Concurrent runs of the same service in the **same** worktree are refused — stop the existing one first.

1. `db.active_runs_for_worktree(worktree_id)` — refuse if a Run for this service already exists with no `ended_at`.
2. Resolve `worktree.path` from the DB. Effective spawn cwd = `worktree.path + service.cwd`.
3. `git_inspector.head_sha(worktree.path)` — snapshot SHA at spawn (the worktree's HEAD, not the Repo's).
4. Pre-allocate `RunId` via `db.record_run({ service_id, worktree_id, sha, log_path, status: Starting, pid: 0 })`.
5. `supervisor.spawn(svc, worktree.path, log_path)` → real PID.
6. `db.update_run_pid(run_id, pid)` and emit `RunStarted`.
7. Background task A: forward `SupervisorEvent`s to the event bus (filtered by `run_id`).
8. Background task B: after 500ms, `port_observer.observe_port(pid, now + 10s)`; on success `db.set_run_port(...)` and emit `RunPortObserved`.
9. On `SupervisorEvent::Exited` or `Crashed`: `db.update_run_status(...)` and emit `RunStopped`.

### Stop Run
Caller passes `run_id` directly. A service can have multiple concurrent runs (one per worktree), so service_id alone is ambiguous.

1. `supervisor.stop(run_id, Duration::from_secs(5))`.
2. Supervisor emits `Exited` / `Crashed`; the existing forwarder records final status. No extra DB write here.

### Refresh Git (background, every 5 min per worktree)
1. `git_inspector.head_sha(worktree.path)` — HEAD may have advanced.
2. `git_inspector.current_branch(worktree.path)` — branch may have changed.
3. `git_inspector.fetch_and_remote_sha(worktree.path, branch)` — coalesce: only one fetch per Repo per cycle (worktrees share the common-dir, so one fetch updates them all).
4. Update cached `GitState` per worktree; emit `GitRefreshed`.
5. For each active run in this worktree: if `run.sha != newest_sha`, emit `ShaDriftDetected`.

### Scan Worktrees (on repo focus, and after the user runs `git worktree add` outside the app)
1. For each Repo: `git_inspector.list_worktrees(repo.primary_path)`.
2. Diff against `db.list_worktrees(repo_id)`:
   - New paths → `db.upsert_worktree(...)` + emit `WorktreeAdded`.
   - Missing paths with no active runs → `db.remove_worktree(...)` + emit `WorktreeRemoved`. If active runs reference it, mark hidden and resolve when those runs end.
3. If `primary_path` was removed, promote another worktree to primary.

### Scan Orphans (on demand and every 30s while UI is focused)
1. `listener_scanner.scan()` → `Vec<LocalListener>`.
2. `db.active_runs()` — across all worktrees.
3. `orphan_reconciler.reconcile(...)`.
4. Emit `OrphansUpdated`.

## Tauri Contract (`hive-tauri`)

Thin layer. Each command is a 5–15 line function that calls into `hive-core`. No business logic here.

### Commands (JS → Rust, request/response)
| Command | Returns |
|---|---|
| `add_repo(path: string)` | `{ repo: Repo, worktrees: Worktree[], services: Service[] }` |
| `list_repos()` | `Repo[]` |
| `remove_repo(repo_id)` | `void` |
| `list_worktrees(repo_id)` | `Worktree[]` |
| `scan_worktrees(repo_id)` | `Worktree[]` |
| `list_services(repo_id)` | `Service[]` |
| `update_service(svc)` | `Service` |
| `start_service(service_id, worktree_id)` | `Run` |
| `stop_run(run_id)` | `void` |
| `active_runs()` | `Run[]` |
| `active_runs_for_worktree(worktree_id)` | `Run[]` |
| `run_logs(run_id, tail_lines)` | `string[]` |
| `git_state(worktree_id)` | `GitState` |
| `refresh_git(worktree_id)` | `GitState` |
| `scan_orphans()` | `ReconcileReport` |

### Events (Rust → JS, push) — same names/shape as `HiveEvent`
Frontend subscribes once at boot, mutates local view model on event.

Frontend stores nothing authoritative. On window focus or reconnect it calls `list_repos` + `active_runs` + `scan_orphans` to resync.

## Persistence (SQLite)

```sql
CREATE TABLE repos (
    id INTEGER PRIMARY KEY,
    common_dir TEXT NOT NULL UNIQUE,           -- canonical identity (`git rev-parse --git-common-dir`)
    primary_path TEXT NOT NULL,                -- the first worktree the user added
    name TEXT NOT NULL,
    default_branch TEXT NOT NULL,
    remote_url TEXT,
    created_at TEXT NOT NULL                   -- ISO-8601 UTC
);

CREATE TABLE worktrees (
    id INTEGER PRIMARY KEY,
    repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    path TEXT NOT NULL UNIQUE,                 -- absolute path to the worktree root
    branch TEXT,                               -- NULL if detached HEAD
    head_sha TEXT NOT NULL,                    -- refreshed by Refresh Git workflow
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
    source TEXT NOT NULL,                      -- WorkflowKind as string
    UNIQUE (repo_id, name)
);

CREATE TABLE runs (
    id INTEGER PRIMARY KEY,
    service_id INTEGER NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    worktree_id INTEGER NOT NULL REFERENCES worktrees(id) ON DELETE CASCADE,
    pid INTEGER NOT NULL DEFAULT 0,            -- 0 between record_run and spawn-complete
    sha TEXT NOT NULL,
    started_at TEXT NOT NULL,
    status_json TEXT NOT NULL,                 -- RunStatus serialized
    port_observed INTEGER,
    log_path TEXT NOT NULL,
    ended_at TEXT
);

CREATE INDEX runs_active ON runs(worktree_id, service_id) WHERE ended_at IS NULL;
CREATE INDEX runs_by_service ON runs(service_id, started_at DESC);
CREATE INDEX worktrees_by_repo ON worktrees(repo_id);
```

Logs: append-only files at `<data_dir>/logs/<run_id>.log`. No rotation in v1 — capped at 10 MB per active run; supervisor stops writing past the cap and notes truncation in the log.

`<data_dir>` resolution:
- macOS: `~/Library/Application Support/hive/`
- Linux: `$XDG_DATA_HOME/hive/` or `~/.local/share/hive/`
- (Windows out of scope for v1.)

## Open Questions

- **`.env` auto-merge** — read repo's `.env` automatically into spawn env, or require explicit `services.env_json`? Recommend auto-merge with `services.env_json` overriding.
- **Workspace sub-package detection** — see Workspace Monorepos section below. Should v1 auto-detect sub-package `dev` scripts, or punt to manual Custom services? Recommend punt for v1, design a `WorkspaceDetector` trait so it slots in cleanly later.
- **Restart-on-new-SHA** — automatic toggle per service, or always manual? Recommend manual button + visible drift badge in v1.
- **Cross-platform** — Linux probably free (lsof, setsid, sh all present). Windows would need `netstat`/`Get-NetTCPConnection`, job objects instead of process groups, `cmd.exe` shell. Out of scope for v1; keep the supervisor/scanner traits clean so a Windows impl can slot in later.

## Workspace Monorepos — Clarification

A "workspace monorepo" is a single repo with multiple sub-projects, each with its own dev workflow. Examples:
- pnpm/yarn/npm workspaces — `apps/web/package.json` and `apps/api/package.json` each have a `dev` script.
- Cargo workspace — multiple binaries under `crates/*` runnable independently.
- Polyglot — frontend in `web/` (`npm run dev`) and backend in `api/` (`uv run uvicorn ...`).

The v1 detector only reads the **root** (`Procfile`, root `package.json`, root `docker-compose.yml`). For a monorepo, that won't surface `apps/web/dev` and `apps/api/dev`. The fallback is: the user adds them as Custom services manually —

| name | command | cwd |
|---|---|---|
| `web` | `pnpm dev` | `apps/web` |
| `api` | `pnpm dev` | `apps/api` |

The data model already supports this (`Service.cwd` is per-service, relative to the worktree root), so a v2 `WorkspaceDetector` that walks `pnpm-workspace.yaml` / `package.json#workspaces` / `Cargo.toml#workspace.members` is purely a detector upgrade — no schema change required.
