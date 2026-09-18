# Bugs, Codex and Inbox

Submitting `:bug <description>` first files a bug with the captured screen and action trail, then starts a Codex run in its own Git worktree. `:idea` remains capture-only. Open Inbox with `:inbox` or the page navigation.

Inbox displays questions, saved replies, review requests, failures and linked PRs. Enter opens a reply. Enter inside the reply inserts a line break; Ctrl-S sends; Esc leaves the editor with the draft saved. Each edit is persisted. A conflicting edit from another window leaves both the saved draft and the local editor intact rather than overwriting either. Ctrl-R deliberately combines both drafts for editing before a later send. Automatic binary reload waits while the reply editor is open. Sending resumes the exact Codex thread associated with that run.

The default Inbox keys are `d` to inspect the reviewed diff, `p` to open the PR confirmation, `r` to retry a failed run and `R` to reconcile an uncertain publication by checking GitHub. PageUp/PageDown scroll the request or diff. Enter on an already-open PR opens it in the browser. All Inbox bindings can be changed in `inbox_keys` in `~/.switchbard/tui.lua`; set a binding to `"none"` to remove it. The equivalent commands are `:diff`, `:publish`, `:retry` and `:reconcile`.

The PR confirmation names the task, repository, branch and reviewed commit. Enter is disabled until the whole confirmation fits. Publication runs the configured gate and publishes only the reviewed commit; a changed worktree or head refuses. Creating a PR does not merge it, mark the task Done or check the reporter's acceptance criterion.

## Configuration

```lua
return {
  bug_codex_binary = "codex",
  bug_gate_command = "mise run ci",
  inbox_keys = { d = "diff", p = "publish", r = "retry", R = "reconcile" },
  -- report_repo = "~/Dev/switchbard",
}
```

Codex must already be installed and authenticated. Its model configuration is inherited. Runs explicitly use workspace-write sandboxing and do not bypass approvals or sandbox restrictions. If a necessary operation is unavailable, the agent should ask through Inbox. Change `bug_gate_command` to the owning repository's actual validation command when it does not use `mise run ci`.

When `report_repo` is set, Inbox includes both the browsed repository and the report repository and labels the source on each request. `:dispatch <bug ID>` retries the file-to-dispatch step without filing another report. The existing `dispatch` label queue continues to use its existing runner; bug runs do not enter or drain it.

## Persistence and recovery

Core owns the execution aggregate in `~/.switchbard/bug-runs.sqlite3` (`SWITCHBARD_BUG_RUN_DB` overrides the path). The task remains authoritative for its content and planning fields. Run identity uses the Git common directory and the central task record identity when available; linked worktrees do not create duplicate runs. The original report snapshot is retained with the run.

A detached one-shot `sbt run-bug` supervisor continues after the terminal closes. Agent questions, owner replies, drafts and reviewed heads survive application restart. The supervisor records the exact Codex thread and process identity. Interrupted or unprovable outcomes stay explicit; retry is refused while a previous process may still be writing. An uncertain PR publication can be reconciled after its writer has stopped. Reconciliation adopts an existing PR with the exact repository, branch, head and base. If no PR exists and the remote branch is absent or still at the reviewed commit, it requeues the previously requested publication. A changed remote branch stays unresolved. The GitHub destination and reviewed commit are pinned before owner confirmation.

Agent logs and retained worktrees live beside the run database under `bug-run-artifacts`; launch diagnostics live in `bug-logs`. Worktrees are retained for inspection. No automatic worktree removal is part of this feature.

## Evidence and remaining acceptance

`crates/switchbard-core/src/bug_run/tests.rs` executes the durable store, real Git repositories and a controlled external-process protocol fixture. It checks concurrency, immutable report evidence, same-thread continuation, failure and interruption, reviewed-head guards, task reference preservation and unsupported storage versions. This proves the protocol and state transitions, not how a live model interprets the prompt.

`crates/switchbard-tui/tests/bug_inbox.rs` drives real keys against the actual app, native task writer and run store. It covers preserved replies, conflicting drafts, explicit publication, configured bindings, task-first capture and narrow/wide layouts. Existing TUI suites cover navigation and capture compatibility.

Live Codex interpretation, the owner's real Inbox response and the resulting authenticated PR remain separate acceptance steps. The mission ledger records their current state; fixture passes must not be reported as the completed paired dogfood run.
