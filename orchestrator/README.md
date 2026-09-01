# switchbard-orchestrator

The LangGraph orchestration agent for the Switchbard Task Queue
(trajectory: *Task Queue orchestration*, TASK-89). It drains the dispatch
queue in stack-rank order, drives each task through a durable per-task
graph, and hands every claim back honestly:

    claim -> prepare -> run_agent -> collect -> gate -> reconcile -> open_pr -> release

- **Protocol, not file edits.** Every task read/mutation goes through
  `sb queue ...` (the switchbard-task crate's binary) (claim, release, prompt) - the repo's
  one-writer invariant extends to the orchestrator.
- **Durable.** Every node boundary checkpoints to SQLite
  (`~/.switchbard/orchestrator/`, override with
  `SWITCHBARD_ORCHESTRATOR_HOME`). Kill the orchestrator mid-run and the
  next `drain` resumes the in-flight graph instead of orphaning the claim.
- **Completion-integrity.** `reconcile` refuses to call a run done on
  vibes: the branch must carry commits, the configured `--gate` must pass,
  and every acceptance criterion must be checked *in the run worktree's
  copy of the task*. Anything unproven interrupts with the exact
  remainder; the driver releases the claim as an honest failure carrying
  that remainder, and `resume` re-claims and re-evaluates.
- **Observable.** Each run appends JSONL events
  (`<stem>.events.jsonl` next to the run log in `$TMPDIR/switchbard-logs/`):
  run_start, node_enter/exit, heartbeat (with log size), interrupt
  (with remainder), release, run_end.

## Run it

```sh
cd orchestrator && uv sync
uv run python -m switchbard_orchestrator drain --repo ~/Dev/yourrepo \
    --gate "mise run ci" [--once] [--max-turns 50]
uv run python -m switchbard_orchestrator resume --repo ~/Dev/yourrepo --task TASK-7
uv run python -m switchbard_orchestrator status --repo ~/Dev/yourrepo
```

stdout is one line per completed run - `<ID>\t<outcome>\t<pr-or-reason>`;
narration goes to stderr. Drain is serial on purpose (GitHub rate-limit
reasoning, same as the Rust pipeline's `drain_dispatch_queue`).

## Tests

```sh
cargo build -p switchbard-task   # builds the `sb` binary the E2E tests drive
cd orchestrator && uv run pytest
```
