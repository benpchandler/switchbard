---
name: switchbard
description: Use Switchbard's sb CLI to set up a local task workspace, read assigned tasks, preserve progress and handoffs, or update criteria and status alongside Claude Code or Codex. Use when the user asks to work from Switchbard tasks; not for developing Switchbard itself or assuming permission to merge, migrate, or finish work.
---

# Switchbard

Use `sb` to read and update the tasks the user sees in `sbt`. Follow the target repository's instructions and the user's scope. Installing this skill grants no new authority.

## First setup

Run `sbt --repo <DIR> doctor --json` for the intended repository. Inspect each check's status and next step. SQLite is embedded: do not install a SQL server or SQLite CLI. Diagnostics do not create a workspace.

If `sb` is missing from PATH, locate the paired installation with `sbt` and help the user configure PATH. Keep both from the same build. Repair required failures before setup; optional GitHub or agent checks do not block local tasks. Explain the missing dependency and ask before installing OS packages or changing shell configuration.

For an unconfigured workspace, offer `sbt --repo <DIR> init` so the user can choose task prefix and stages, or use `init --yes` when defaults are authorized. Existing settings and legacy task data must be preserved. Do not migrate, reset, or delete data to make setup pass.

Ask whether GitHub features are wanted before running `sbt --repo <DIR> doctor --github --json`. Guide the user through `gh auth login --hostname github.com --web` when needed; have the user complete authentication. Never read, print, or share tokens, passwords, or credential files. Git push/SSH authentication does not establish gh API authentication. Current PR features support github.com only. Existing gh privileges are reused; they are not narrowed by Switchbard.

## Work from a task

Use an explicit `--repo <DIR>` if working across repositories or worktrees. Start with:

```sh
sb --repo <DIR> list
sb --repo <DIR> view <ID>
```

Read the description, acceptance criteria, plan, notes, and dependencies. Preserve intent and unresolved decisions. Use command-specific `--help` rather than inventing flags. Create tasks only when the user requested or authorized them:

```sh
sb --repo <DIR> create "Task title" --description "Requested outcome" --ac "Checkable criterion"
```

Keep the actual returned ID; never assume TASK-1 or a default prefix. Use native commands, not direct database or backlog-file edits. Test the implementation in the repository using its required checks.

## Preserve progress and hand off

```sh
sb --repo <DIR> edit <ID> --append-notes "What changed, checks performed, remaining work and uncertainties."
sb --repo <DIR> edit <ID> --check-ac 1
```

Check only criteria actually satisfied. Notes should give the next session a useful starting point without overstating proof. If interrupted, explain what remains and why. Preserve existing notes and plans; do not replace them with a summary that drops user intent.

In a Claude Code session with valid session identity, `sb work claim <ID>` makes live work visible and `sb work release <ID> --note "What remains"` hands it back. Release is not Done. Automatic Codex hooks and accurate Codex live-claim attribution are not provided; use the regular task CLI in Codex.

Completion, checked criteria, a released claim, a merged PR, and deployment are separate facts. Mark Done only when the user's acceptance and authority permit it:

```sh
sb --repo <DIR> edit <ID> -s Done
```

Do not merge or activate automation simply because a task is ready. Keep owner review and other remaining gates explicit. Back up important local data before authorized upgrades or migrations with `sb --repo <DIR> storage backup --file <PRIVATE_PATH>`.
