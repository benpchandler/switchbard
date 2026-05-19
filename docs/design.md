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
pub struct ServiceId(pub i64);
pub struct RunId(pub i64);

pub struct Repo {
    pub id: RepoId,
    pub path: PathBuf,
    pub name: String,
    pub default_branch: String,
    pub created_at: DateTime<Utc>,
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
    pub pid: u32,
    pub sha: String,                           // git HEAD captured at spawn
    pub started_at: DateTime<Utc>,
    pub status: RunStatus,
    pub port_observed: Option<u16>,            // populated by port_observer; null until seen
    pub log_path: PathBuf,                     // <data_dir>/logs/<run_id>.log
    pub ended_at: Option<DateTime<Utc>>,
}

pub struct GitState {
    pub current_sha: String,                   // git rev-parse HEAD
    pub default_branch: String,
    pub remote_url: Option<String>,
    pub newest_sha: Option<String>,            // origin/<default_branch> after fetch
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
    fn add_repo(&self, path: &Path, name: &str, branch: &str) -> Result<RepoId>;
    fn list_repos(&self) -> Result<Vec<Repo>>;
    fn remove_repo(&self, id: RepoId) -> Result<()>;

    fn upsert_services(&self, repo_id: RepoId, svcs: Vec<NewService>) -> Result<Vec<Service>>;
    fn list_services(&self, repo_id: RepoId) -> Result<Vec<Service>>;
    fn update_service(&self, svc: &Service) -> Result<()>;

    fn record_run(&self, run: NewRun) -> Result<RunId>;
    fn update_run_status(&self, id: RunId, status: RunStatus) -> Result<()>;
    fn set_run_port(&self, id: RunId, port: u16) -> Result<()>;
    fn active_runs(&self) -> Result<Vec<Run>>;
    fn runs_for_service(&self, id: ServiceId, limit: usize) -> Result<Vec<Run>>;
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
    fn head_sha(&self, repo: &Path) -> Result<String>;
    fn default_branch(&self, repo: &Path) -> Result<String>;
    fn remote_url(&self, repo: &Path) -> Result<Option<String>>;
    async fn fetch_and_remote_sha(&self, repo: &Path, branch: &str) -> Result<String>;
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
    RunStarted(Run),
    RunLogLine { run_id: RunId, stream: Stream, line: String },
    RunPortObserved { run_id: RunId, port: u16 },
    RunStopped { run_id: RunId, status: RunStatus },
    GitRefreshed { repo_id: RepoId, state: GitState },
    ShaDriftDetected { repo_id: RepoId, running_sha: String, newest_sha: String },
    OrphansUpdated(ReconcileReport),
}
```

## Workflows (multi-step orchestrations)

These live in `hive-core` as free functions that compose the components above. The Tauri layer is a thin call site.

### Add Repo
1. User picks folder (Tauri `dialog::open`).
2. `git_inspector.head_sha(path)` — fail fast if not a git repo.
3. `git_inspector.default_branch(path)` and `remote_url(path)`.
4. `db.add_repo(path, name, branch)` → `RepoId`.
5. `workflow_detector.detect(path)` → `Vec<DetectedService>`.
6. `db.upsert_services(repo_id, detected)`.
7. Emit `HiveEvent::RepoAdded`.
8. Spawn background task: `git_inspector.fetch_and_remote_sha(...)`; emit `GitRefreshed`.

### Start Service
1. `db.active_runs()` — refuse if a Run exists for this service with no `ended_at` (one active run per service in v1).
2. `git_inspector.head_sha(repo_path)` — snapshot SHA at spawn time.
3. Pre-allocate `RunId` via `db.record_run({ service_id, sha, log_path, status: Starting, pid: 0 })`.
4. `supervisor.spawn(svc, repo_path, log_path)` → real PID.
5. `db.update_run_pid(run_id, pid)` and emit `RunStarted`.
6. Background task A: forward `SupervisorEvent`s to the event bus (filtered by `run_id`).
7. Background task B: after 500ms, `port_observer.observe_port(pid, now + 10s)`; on success `db.set_run_port(...)` and emit `RunPortObserved`.
8. On `SupervisorEvent::Exited` or `Crashed`: `db.update_run_status(...)` and emit `RunStopped`.

### Stop Service
1. `db.active_runs()` → find run for service_id (or accept `run_id` directly).
2. `supervisor.stop(run_id, Duration::from_secs(5))`.
3. Supervisor emits `Exited` / `Crashed`; the existing forwarder records final status. No extra DB write here.

### Refresh Git (background, every 5 min per repo)
1. For each repo: `git_inspector.fetch_and_remote_sha(...)`.
2. Update cached `GitState`; emit `GitRefreshed`.
3. If any active run on this repo has `sha != newest_sha`, emit `ShaDriftDetected`.

### Scan Orphans (on demand and every 30s while UI is focused)
1. `listener_scanner.scan()` → `Vec<LocalListener>`.
2. `db.active_runs()`.
3. `orphan_reconciler.reconcile(...)`.
4. Emit `OrphansUpdated`.

## Tauri Contract (`hive-tauri`)

Thin layer. Each command is a 5–15 line function that calls into `hive-core`. No business logic here.

### Commands (JS → Rust, request/response)
| Command | Returns |
|---|---|
| `add_repo(path: string)` | `Repo` |
| `list_repos()` | `Repo[]` |
| `remove_repo(repo_id)` | `void` |
| `list_services(repo_id)` | `Service[]` |
| `update_service(svc)` | `Service` |
| `start_service(service_id)` | `Run` |
| `stop_service(service_id)` | `void` |
| `active_runs()` | `Run[]` |
| `run_logs(run_id, tail_lines)` | `string[]` |
| `git_state(repo_id)` | `GitState` |
| `refresh_git(repo_id)` | `GitState` |
| `scan_orphans()` | `ReconcileReport` |

### Events (Rust → JS, push) — same names/shape as `HiveEvent`
Frontend subscribes once at boot, mutates local view model on event.

Frontend stores nothing authoritative. On window focus or reconnect it calls `list_repos` + `active_runs` + `scan_orphans` to resync.

## Persistence (SQLite)

```sql
CREATE TABLE repos (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    default_branch TEXT NOT NULL,
    created_at TEXT NOT NULL                   -- ISO-8601 UTC
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
    pid INTEGER NOT NULL DEFAULT 0,            -- 0 between record_run and spawn-complete
    sha TEXT NOT NULL,
    started_at TEXT NOT NULL,
    status_json TEXT NOT NULL,                 -- RunStatus serialized
    port_observed INTEGER,
    log_path TEXT NOT NULL,
    ended_at TEXT
);

CREATE INDEX runs_active ON runs(service_id) WHERE ended_at IS NULL;
CREATE INDEX runs_by_service ON runs(service_id, started_at DESC);
```

Logs: append-only files at `<data_dir>/logs/<run_id>.log`. No rotation in v1 — capped at 10 MB per active run; supervisor stops writing past the cap and notes truncation in the log.

`<data_dir>` resolution:
- macOS: `~/Library/Application Support/hive/`
- Linux: `$XDG_DATA_HOME/hive/` or `~/.local/share/hive/`
- (Windows out of scope for v1.)

## Open Questions

- **Multiple concurrent instances of one service** — allow or forbid? Recommend forbid in v1 (one active run per service; history persists). Reconsider if user wants to compare two SHAs side-by-side.
- **`.env` auto-merge** — read repo's `.env` automatically into spawn env, or require explicit `services.env_json`? Recommend auto-merge with `services.env_json` overriding.
- **Workspace monorepos (`apps/web` + `apps/api` under one repo)** — detect each workspace as its own service, or treat the whole repo as one detection unit? Defer; current model handles it manually (add multiple Custom services).
- **Restart-on-new-SHA** — automatic toggle per service, or always manual? Recommend manual button + visible drift badge in v1.
- **Cross-platform** — Linux probably free (lsof, setsid, sh all present). Windows would need `netstat`/`Get-NetTCPConnection`, job objects instead of process groups, `cmd.exe` shell. Out of scope for v1; keep the supervisor/scanner traits clean so a Windows impl can slot in later.
