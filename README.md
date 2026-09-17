# Switchbard

A local-first terminal workspace for tasks, pull requests, and coding agents.

**Early alpha.** Switchbard is developed quickly and dogfooded daily. Expect bugs and changing behavior. Start with a small repository, keep backups, and report problems through [GitHub Issues](https://github.com/benpchandler/switchbard/issues).

The supported interface is **`sbt`**, the terminal UI. **`sb`** provides the same task write layer for scripts and agents. The desktop GUI is deprecated; its source and older releases remain available, but new product work and public installation focus on the TUI.

From any `sbt` page, press `i` to capture an idea or `b` to capture a bug for the repository being viewed; Enter saves once and Esc cancels. These captures use the repository's native defaults and keep their destination pinned through saving and retry. The task menu's `t b` still assigns the ball. Colon commands `:bug` and `:idea` remain the legacy tool-report route; see [repository idea and bug capture](docs/repo-report-capture.md) for routing and state details.

## Install

The TUI release installer installs `sb` and `sbt` together without Rust, Node, Python, or a desktop application. Release targets are macOS Apple Silicon, macOS Intel, and Linux x86_64. It verifies the published archive checksum before installing.

```sh
installer=$(mktemp)
curl -fsSL https://raw.githubusercontent.com/benpchandler/switchbard/v0.4.0-alpha.1/scripts/install-release.sh -o "$installer"
bash "$installer" --version v0.4.0-alpha.1
rm -f "$installer"
```

The example selects the first alpha tag explicitly and requires that release to contain the TUI archives. The installer defaults to `~/.local/bin`; follow its `PATH` guidance if needed. Older GUI-only releases cannot be installed with it; if no compatible TUI release is available yet, use the source route in [the installation guide](docs/INSTALL-TUI.md). No npm package or Homebrew formula is currently provided.

If `~/.local/bin` is outside your `PATH`, add it to your shell configuration and open a new terminal:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

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
- Inspect repository pull requests and their checks, review, and merge observations. GitHub features require the optional GitHub CLI and authentication.
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

When reporting a bug, include `sbt build-id`, your OS and terminal, the steps to reproduce, and the error shown. Do not post private task text or unreviewed logs. TUI commands `:bug` and `:idea` capture tasks in your current repository; they do not submit GitHub issues.

## Development

Build the terminal tools with Rust 1.95 or newer and a native C compiler:

```sh
git clone https://github.com/benpchandler/switchbard.git
cargo build --manifest-path switchbard/Cargo.toml --release -p switchbard-task -p switchbard-tui
./switchbard/target/release/sbt --repo /path/to/your/repo
```

Mise pins the repository toolchain. Contributors should read [CLAUDE.md](CLAUDE.md), install the tracked hooks with `mise run hooks-install`, and use the repository's validation gates. Core domain logic lives in `switchbard-core`, the terminal UI in `switchbard-tui`, and the `sb` CLI in `switchbard-task`. The deprecated GUI and optional dispatch/orchestrator components remain separate from the public terminal install.

[Installation](docs/INSTALL-TUI.md) · [View history](docs/tui-view-history.md) · [Task planning](docs/task-planning-progress.md) · [MIT license](LICENSE)
