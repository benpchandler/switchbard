# Use Switchbard alongside Claude Code or Codex

Keep `sbt` open beside your coding session. The TUI is your view of the work; `sb` is the command interface your agent can use. Both use the same task write layer. The public installation contains no agent skill and modifies no Claude or Codex settings.

## Capture, work, hand off, review

From your configured repository:

```sh
sb create "Add a search filter" --description "Let users filter the task list by title."
sb list
sb view TASK-1
```

Use the actual returned ID. Ask Claude or Codex to read the task, follow the repository's instructions, make the change, and report relevant test results. Specify the scope and acceptance criteria before edits. The CLI has command-specific help: `sb create --help` and `sb edit --help`.

An agent can preserve a handoff with:

```sh
sb edit TASK-1 --append-notes "Implementation ready; tests passed; owner review remains."
```

If interrupted, describe what remains and why. Start the next session with `sb view TASK-1`. Notes preserve task context, not the complete previous conversation. Inspect the change and pull request before explicitly setting Done:

```sh
sb edit TASK-1 -s Done
```

Use `--repo /path/to/repo` when the terminal is outside the intended repository. Review dependencies and task details before assigning work; a task list does not establish that work is ready or finished.

## Optional Claude Code live claims

Inside a Claude Code session with `CLAUDE_CODE_SESSION_ID` and `CLAUDE_PID` available:

```sh
sb work claim TASK-1
sb work list
sb work release TASK-1 --note "Handing back for review; browser check remains."
```

Claims make live work visible in `sbt`. A release with unchecked criteria requires a note. Releasing returns the turn to the owner and does not mark Done. Claim status updates use the built-in In Progress label, so custom workflows may not get that status transition.

`sb work hook` is a Claude Code hook endpoint, but the public installer does not register hooks. Explicit session and process arguments are available via `sb work claim --help`; they must identify the real long-lived agent process. Native automatic Codex session discovery and hooks are not supplied; Codex can still read and update tasks with the regular CLI. The current live-claim implementation labels sessions as Claude, so explicit IDs do not provide accurate Codex agent attribution.

## Optional integrations and limits

Local tasks work without a GitHub or agent account. Authenticate your coding agent separately. Pull-request features need `gh` and `gh auth login`; missing or stale remote observations are not proof that a pull request is ready to merge. Automation and dispatch have separate configuration and are not activated by installing the terminal pair.

The release is an early alpha. Back up important task data before upgrades, keep unfinished work explicit, and report bugs with your build identity. See [installation and recovery](INSTALL-TUI.md).
