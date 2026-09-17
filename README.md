# Switchbard

A terminal companion for your Claude Code and Codex sessions.

Keep the work visible while your agent codes. Use `sbt` to browse tasks and pull requests, and give your agent `sb` commands to read the same task, update progress, and leave a useful handoff for the next session. Your task context lives locally and survives a chat reset.

**Early alpha.** Switchbard is developed quickly and dogfooded daily. Expect bugs and changing behavior. Start with a small repository, keep backups, and report problems through [GitHub Issues](https://github.com/benpchandler/switchbard/issues).

The supported interface is **`sbt`**, the terminal UI. **`sb`** provides the same task write layer for scripts and agents. The desktop GUI is deprecated; its source and older releases remain available, but new product work and public installation focus on the TUI.

In `v0.4.0-alpha.2` and current source builds, from any `sbt` page press `i` to capture an idea or `b` to capture a bug for the repository being viewed; Enter saves once and Esc cancels. These captures use the repository's native defaults and keep their destination pinned through saving and retry. The task menu's `t b` still assigns the ball. Colon commands `:bug` and `:idea` remain the legacy tool-report route; see [repository idea and bug capture](docs/repo-report-capture.md) for routing and state details.

## Work alongside your agent

### Turn a request into a task

Open `sbt` in your repository, press `t` then `n`, and capture the work you want done. Add a description and acceptance criteria so the next session can see the goal. You can also create it from the terminal:

```sh
sb create "Add a search filter" --description "Let users filter the task list by title."
sb list
```

### Give Claude or Codex a shared starting point

Ask your agent: "Read TASK-1 with `sb view TASK-1`. Follow this repository's instructions, implement the task, run the relevant checks, and append progress and remaining work with `sb edit TASK-1 --append-notes`. Ask before marking it Done."

Replace `TASK-1` with the ID returned by `sb create`. Both agents can use the CLI through their terminal tools. The release includes offline agent instructions. Install them explicitly with `sbt skill install --agent both`; existing custom skills are preserved. It does not register hooks or change credentials.

### Resume after a reset or switch sessions

```sh
sb view TASK-1
sb edit TASK-1 --append-notes "Filter implemented; keyboard navigation still needs checking."
```

Descriptions, criteria, plans, and notes give the next session a starting point without reconstructing the previous chat. Keep notes specific: what changed, what was tested, and what remains uncertain. Review the task and pull request in `sbt`, then explicitly mark the task Done when you accept the outcome. A checked criterion or merged PR does not automatically finish it.

Claude Code also has an optional live-claim protocol. Codex can use the task CLI, but automatic Claude hooks and session identity are not a native Codex integration. See [agent workflows](docs/agent-workflows.md) for commands and integration limits.

## Install

The TUI release installer installs `sb` and `sbt` together without Rust, Node, Python, or a desktop application. Release targets are macOS Apple Silicon, macOS Intel, and Linux x86_64. It verifies the published archive checksum before installing.

```sh
installer=$(mktemp)
curl -fsSL https://raw.githubusercontent.com/benpchandler/switchbard/v0.4.0-alpha.1/scripts/install-release.sh -o "$installer"
bash "$installer" --version v0.4.0-alpha.1
rm -f "$installer"
```

The example installs the developer-onboarding TUI alpha explicitly. The installer defaults to `~/.local/bin`; follow its `PATH` guidance if needed. Older GUI-only releases cannot be installed with it; you can also [build and configure from source](docs/INSTALL-TUI.md#build-from-source). No npm package or Homebrew formula is currently provided.

If `~/.local/bin` is outside your `PATH`, add it to your shell configuration and open a new terminal:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

## Check your setup and connect your agent

The onboarding commands below are in the current source and are scheduled for
the upcoming alpha2 release. The published alpha1 binary does not contain
these commands. From your Git repository:

```sh
sbt doctor
sbt doctor --github       # optional remote GitHub access check
sbt skill install --agent both
sbt agent-prompt          # copy this into a Claude Code or Codex session
```

SQLite is embedded; no database installation or server is needed. `doctor` reports readiness and next steps without creating a workspace. Remote checks are opt-in. Skills install to your personal Claude Code and Codex skill directories, with separate `--agent claude` or `--agent codex` choices. Inspect the payload with `sbt skill show`. See [diagnostics and agent setup](docs/developer-onboarding.md) for JSON output, repair steps, custom instructions, and limitations.

## First run

Open a terminal in an existing Git repository, then run:

```sh
sbt
```

For an unconfigured repository, Switchbard explains setup and asks before creating a workspace. Accept suggested task references and workflow stages, or customize them. Setup stores the workspace locally; it creates no `backlog/` directory. Existing legacy workspaces keep their current storage until you explicitly migrate them.

- Press **`?`** for the current keyboard controls.
- Press **`t`**, then **`n`** to create your first task.
- Press **Enter** to inspect the selected task; **Tab** switches pages.
- Press **`q`** to quit.

To choose a repository explicitly, run `sbt --repo /path/to/repo`. For unattended setup, run `sbt --repo /path/to/repo init --yes`. See [onboarding and recovery](docs/sbt-onboarding.md) for customization, cancellation, and existing-workspace behavior.

## What you can do

- Organize tasks into initiatives, projects, and sub-issues, with criteria, dependencies, due dates, and custom fields.
- Filter, sort, group, and paint terminal views. Your last view resumes per repository, with bounded view history.
- Inspect repository pull requests and their checks, review, and merge observations. GitHub features reuse optional `gh` authentication; the installer does not install or log in to it. See [GitHub setup and permissions](docs/INSTALL-TUI.md#connect-github-optional).
- See local coding-agent sessions and task claims. Integrations depend on the agent tools installed on your machine.
- Use `sb` to create, inspect, and update tasks from scripts or agents through the same core write layer.

```sh
sb --help
sb --repo /path/to/repo list
sbt build-id
```

Some pages and integrations are still incomplete. Task completion is explicit; checked criteria or a merged PR do not automatically mark a task Done.

## Local data and alpha recovery

Switchbard requires no account and sends no usage analytics. Task data is stored locally, normally in `~/.switchbard/switchbard.sqlite3`; legacy repositories may still use Backlog-format files. The TUI writes local diagnostic events to `~/.switchbard/tui-events.jsonl`, which can contain repository paths and action details. Review diagnostic material before sharing it publicly.

Before upgrades or important changes, create a database backup:

```sh
sb --repo /path/to/repo storage backup --file /private/path/switchbard-backup.sqlite3
```

Keep legacy task files under your normal backup/version-control policy too. See [central storage and recovery](docs/central-storage.md) for authority, migration, export, and restore. Public release installation does not enable background updates; upgrading is an explicit choice. The author's source auto-install loop is a separate opt-in developer workflow.

When reporting a bug, include `sbt build-id`, your OS and terminal, the steps to reproduce, and the error shown. Do not post private task text or unreviewed logs. Capture shortcuts can differ between the published alpha and newer source builds; neither route submits a GitHub issue automatically.

## Build and configure from scratch

Build the terminal tools with Rust 1.95 or newer and a native C compiler:

```sh
git clone https://github.com/benpchandler/switchbard.git
cargo build --locked --manifest-path switchbard/Cargo.toml --release -p switchbard-task -p switchbard-tui
./switchbard/target/release/sbt --repo /path/to/your/repo
```

Run the resulting `sbt` in your own Git repository and choose custom task prefixes and workflow stages at first start. You can also configure a new workspace explicitly:

```sh
./switchbard/target/release/sbt --repo /path/to/your/repo init --yes \
  --task-prefix APP --status Inbox --status Doing --status Done
```

Setup does not overwrite an existing workspace. See [the full source and configuration guide](docs/INSTALL-TUI.md#build-from-source) for installing both binaries, toolchain requirements, and existing-workspace behavior.

Mise pins the repository toolchain. Contributors should read [CLAUDE.md](CLAUDE.md), install the tracked hooks with `mise run hooks-install`, and use the repository's validation gates. Core domain logic lives in `switchbard-core`, the terminal UI in `switchbard-tui`, and the `sb` CLI in `switchbard-task`. The deprecated GUI and optional dispatch/orchestrator components remain separate from the public terminal install.

[Installation](docs/INSTALL-TUI.md) · [View history](docs/tui-view-history.md) · [Task planning](docs/task-planning-progress.md) · [MIT license](LICENSE)
