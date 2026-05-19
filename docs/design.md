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
    Procfile,                                  // root Procfile entry
    DockerCompose,                             // opaque `docker compose up`
    NodePackage,                               // any package.json dev/start/serve — root or workspace member
    Custom,                                    // user-defined
}

pub struct Service {
    pub id: ServiceId,
    pub repo_id: RepoId,
    pub name: String,                          // "web", "api", "@scope/admin", "compose"
    pub command: String,                       // shell command, run via /bin/sh -c (possibly mise-wrapped)
    pub cwd: PathBuf,                          // relative to worktree root: "." for root, "apps/web" for a sub-package
    pub env: BTreeMap<String, String>,         // explicit overrides; deterministic order
    pub port_hint: Option<u16>,                // null if detection couldn't infer
    pub source: WorkflowKind,                  // where this came from at detection time
    pub disabled: bool,                        // user hid it; detector won't re-show it on re-detect
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
    fn add_detected_services(&self, repo_id: RepoId, svcs: Vec<NewService>) -> Result<Vec<Service>>;
    // ^ inserts only new (repo_id, name) pairs. Never overwrites user edits or disabled rows.
    fn list_services(&self, repo_id: RepoId, include_disabled: bool) -> Result<Vec<Service>>;
    fn update_service(&self, svc: &Service) -> Result<()>;
    fn set_service_disabled(&self, id: ServiceId, disabled: bool) -> Result<()>;

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
    pub name: String,                          // package name or Procfile entry name
    pub command: String,
    pub cwd: PathBuf,                          // "." for root, "apps/web" for a sub-package
    pub port_hint: Option<u16>,
    pub source: WorkflowKind,
}
```

The default impl is a **composite with mixed semantics**:

- **Procfile is authoritative.** If a Procfile exists at the repo root, only its entries are returned — the user explicitly declared their workflow.
- Otherwise, every other sub-detector runs and their results are **merged**, deduplicated by `name` (collision policy: WorkspaceDetector > RootNodePackageDetector > DockerComposeDetector, with a warning surfaced to the UI).

Sub-detectors (when no Procfile):

1. **WorkspaceDetector** — the monorepo workhorse. Reads `package.json#workspaces` (npm/yarn/bun) and/or `pnpm-workspace.yaml`. Resolves globs, walks each member's `package.json`, and emits one Service per workspace member that has a `dev`/`start`/`serve` script. Per-package fields:
   - `name = <package-name>` (member's `package.json#name`, falling back to dir name)
   - `cwd = <member-path-relative-to-root>`
   - `command = <pm> run <script>` where `<pm>` comes from root lockfile sniffing: `pnpm-lock.yaml` → `pnpm`, `yarn.lock` → `yarn`, `bun.lockb` → `bun`, else `npm`
   - Script priority: `dev` > `start` > `serve`; only the first found per package surfaces by default (the rest are addable later via the editor)
2. **RootNodePackageDetector** — if root `package.json` is *not* a workspace root and has a `dev`/`start`/`serve` script, emit one Service named after the package with `cwd = "."`.
3. **DockerComposeDetector** — if `docker-compose.yml` / `compose.yaml` / `compose.yml` is present at the root, emit one opaque Service `name: "compose", command: "docker compose up", cwd: "."` *in addition to* whatever the Node detectors found.
4. **TurborepoOverlay** — post-process: if `turbo.json` is present, rewrite each NodePackage `command` to `turbo run <script> --filter=<name>` so Turbo's pipeline + cache apply.
5. **NxOverlay** — post-process: if `nx.json` plus per-project `project.json` files are present, rewrite NodePackage commands to `nx run <project>:<target>`.
6. **MiseWrap** — final post-process: if `.mise.toml` or `.tool-versions` is present at the repo root, wrap every command in `mise exec -- <cmd>` so per-repo tool versions activate. (`asdf` recognized as a fallback if mise isn't installed.)

Returns empty only if none of these match. User can always add Custom services manually.

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
6. `workflow_detector.detect(path)` → services attached to the Repo via `db.add_detected_services(...)`. Composite walks Procfile (authoritative) or merges WorkspaceDetector + RootNodePackageDetector + DockerComposeDetector, then applies Turbo/Nx/Mise overlays.
7. `git_inspector.list_worktrees(path)` → enumerate every worktree of this Repo. For each: `db.upsert_worktree(...)`.
8. Emit `RepoAdded` (if new) + `WorktreeAdded` for each new worktree.
9. Spawn background task: `git_inspector.fetch_and_remote_sha(...)` per worktree; emit `GitRefreshed`.

### Start Service (in a specific Worktree)
Caller passes `(service_id, worktree_id)`. Concurrent runs of the same service across **different** worktrees are expected and allowed. Concurrent runs of the same service in the **same** worktree are refused — stop the existing one first.

1. Verify the service is not `disabled` — refuse with a clear error if so (user must re-enable first).
2. `db.active_runs_for_worktree(worktree_id)` — refuse if a Run for this service already exists with no `ended_at`.
3. Resolve `worktree.path` from the DB. Effective spawn cwd = `worktree.path + service.cwd`.
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
    disabled INTEGER NOT NULL DEFAULT 0,       -- user hid it; detector won't re-show on rescan
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
- **Python / Cargo monorepo detection** — JS workspaces (npm/yarn/pnpm/bun) + Turbo/Nx are first-class in v1. Python multi-package (`apps/api/pyproject.toml`, uv workspaces, Poetry) and Cargo workspace bins are v1.5. Custom services in the meantime. Open: prioritize Python next since polyglot Node-front / Python-back monorepos are common?
- **mise wrap default-on** — MiseWrap activates automatically when `.mise.toml` / `.tool-versions` is present, with a per-service "raw shell" toggle for users who manage tool versions another way. Confirm or flip?
- **Restart-on-new-SHA** — automatic toggle per service, or always manual? Recommend manual button + visible drift badge in v1.
- **Cross-platform** — Linux probably free (lsof, setsid, sh all present). Windows would need `netstat`/`Get-NetTCPConnection`, job objects instead of process groups, `cmd.exe` shell. Out of scope for v1; keep the supervisor/scanner traits clean so a Windows impl can slot in later.

## Workspace Monorepos

**Monorepos are the central case, not an edge case.** The user adds a monorepo, sees every sub-package's dev script as its own Service ready to start, and never opens a config editor. Detection is the product.

### What v1 auto-detects

| Repo shape | How v1 handles it | Resulting Services |
|---|---|---|
| `package.json#workspaces: ["apps/*", "packages/*"]` (npm / yarn / bun) | WorkspaceDetector walks globs, reads each child's `package.json` | One Service per workspace package that has a `dev` / `start` / `serve` script |
| `pnpm-workspace.yaml` | Same as above with pnpm as the command prefix | Same |
| Bun workspaces (`bun.lockb` + `workspaces` field) | Same with bun prefix | Same |
| Root `package.json` scripts (non-workspace repo) | RootNodePackageDetector | One Service for the highest-priority script found |
| `turbo.json` present | TurborepoOverlay rewrites every NodePackage command | `turbo run dev --filter=<name>` per package — honors Turbo's pipeline + remote cache |
| `nx.json` + per-project `project.json` files | NxOverlay rewrites | `nx run <project>:<target>` per project |
| `.mise.toml` / `.tool-versions` at repo root | MiseWrap wraps every command (Node and Compose alike) | `mise exec -- <original command>` — activates per-repo Node/Python/etc. versions |
| `docker-compose.yml` at root | DockerComposeDetector | One opaque Service `compose`, **in addition to** the Node Services |
| Procfile at root | Authoritative — overrides everything else | One Service per Procfile entry; Node/Compose detectors skipped |

### Inferences and defaults

- **Package manager** is determined by root lockfile: `pnpm-lock.yaml` → `pnpm`, `yarn.lock` → `yarn`, `bun.lockb` → `bun`, else `npm`. Per-workspace lockfiles aren't honored in v1 (rare in practice).
- **Script priority per package** is `dev` > `start` > `serve`. Only the first found surfaces by default; the others are addable from the service editor with one click.
- **Service naming** uses the workspace package's `name` field (including scopes like `@acme/web`); falls back to the directory name. UI shows `<repo-name> / <service-name>` so scoped names don't fight long repo paths.
- **Port hints** are best-effort: parse common patterns from the resolved command (`--port 3000`, `PORT=4000`, `-p 5173`) and from `package.json` config keys. Null when unsure — `port_observer` resolves the truth at runtime regardless.

### Re-detect semantics (the load-bearing part)

A monorepo grows. After `git pull` adds `apps/admin/`, the user clicks **Re-detect** and Hive adds the new Service without touching anything else. Specifically:

- Detector emits its full current view of the repo.
- `db.add_detected_services` only inserts `(repo_id, name)` pairs not already present.
- Existing services keep their `command`, `cwd`, `env`, `disabled` flag, and `port_hint` unchanged.
- Services that no longer appear in detection output stay in the DB if they have any historical runs (so log history isn't orphaned); otherwise they're marked hidden and offered as a one-click cleanup in the UI.

### Out of scope for v1 (additive in v1.5)

- **Python multi-package monorepos** — `apps/api/pyproject.toml` + `apps/worker/pyproject.toml`, uv workspaces, Poetry. The trait is a new sub-detector (`PythonPackageDetector`) with no schema change.
- **Cargo workspace bins** — `Cargo.toml#workspace.members` with `[[bin]]` sections. Same shape.
- **Custom shell-script conventions** — `bin/dev`, `scripts/start.sh`. Possibly auto-surface if executable and named conventionally.

For these in v1, the user adds Custom Services. The data model already accommodates them; only the detector menu grows.
