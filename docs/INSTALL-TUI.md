# Install the Switchbard terminal tools

The supported public package contains `sbt` (terminal UI), `sb` (CLI), and the MIT license. It needs no Rust, Node, Python, GUI application, or xplan sidecar at runtime. The initial release targets are macOS Apple Silicon, macOS Intel, and Linux x86_64. Windows and Linux ARM have no prebuilt package.

## Install a release

Use a terminal with Bash, `curl`, `tar`, and a SHA-256 utility (`shasum` on macOS or `sha256sum` on Linux):

```sh
installer=$(mktemp)
curl -fsSL https://raw.githubusercontent.com/benpchandler/switchbard/v0.4.0-alpha.1/scripts/install-release.sh -o "$installer"
bash "$installer" --version v0.4.0-alpha.1
rm -f "$installer"
```

The example selects the developer-onboarding TUI alpha explicitly. Without `--version`, the installer chooses GitHub's latest stable release, which may still be an older GUI-only release. An absent TUI asset produces an error rather than installing the GUI. Use the source route below if you prefer to build your own binaries.

The installer downloads the matching archive and `.sha256` file, verifies the checksum, and installs both tools into `~/.local/bin`. It refuses to replace existing binaries without `--replace`. To choose a different directory, add `--bin-dir /path/to/bin`.

If the installer says the directory is outside your `PATH`, add this line to your shell configuration (`~/.zshrc` or `~/.bashrc`) and open a new terminal:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Verify installation:

```sh
sb --version
sbt build-id
```

Git must be installed to work with repositories. Local task setup and use require no GitHub account. The Pull Requests page requires the optional [GitHub CLI](https://cli.github.com/) (`gh`) and `gh auth login`; agent visibility depends on the relevant local agent tools. Missing optional integrations do not turn local task use into an account requirement.

The Linux archive uses a static musl build and does not require desktop libraries. macOS archives are native binaries; broad compatibility with older macOS versions is not yet established. Check the release notes for the tested OS versions.

## Connect GitHub (optional)

Switchbard runs the `gh` executable on your terminal's `PATH`. It reuses GitHub CLI authentication; it does not run its own OAuth flow, ask for your password, or maintain a Switchbard token store. The installer does not install `gh` or log you in. Local tasks still work without it.

Install [GitHub CLI](https://cli.github.com/), then authenticate from your terminal:

```sh
gh auth login --hostname github.com --web
gh auth status --hostname github.com
```

In your repository, verify the account and repository that `gh` can access before opening the Pull Requests page:

```sh
git remote -v
gh repo view --json nameWithOwner,url
gh pr list --limit 5
```

If those commands fail, fix the CLI login or repository access first. Git push credentials or an SSH key do not establish `gh` API authentication. Multiple accounts use GitHub CLI's active account for the host. `GH_TOKEN` or `GITHUB_TOKEN` in the launching terminal override stored authentication; review your environment without printing tokens. Organization SSO or token approval policies may require additional authorization from the organization.

GitHub CLI normally saves browser-login credentials in the system credential store, but can fall back to a plaintext file if that store is unavailable. Check `gh auth status` for the location. See the official [login](https://cli.github.com/manual/gh_auth_login), [environment](https://cli.github.com/manual/gh_help_environment), and [authentication status](https://cli.github.com/manual/gh_auth_status) documentation. Do not include token output or unreviewed authentication details in a public bug report.

### Permissions and actions

| Feature | Access used |
| --- | --- |
| Local tasks and setup | Local files/database; no GitHub permission |
| PR list, checks, reviews, and merge readiness | Read access to the repository and associated metadata |
| Confirmed PR merge | Repository merge permission and a token permitted to perform the mutation; GitHub policies still apply |

Switchbard does not request a separate permission grant or narrow your existing `gh` credentials. Standard `gh auth login` uses GitHub CLI's OAuth permissions; classic-token login documents `repo`, `read:org`, and `gist`, which are broader than simply reading PRs. A custom fine-grained read-only token may restrict writes, but a complete minimum-permission token recipe has not been validated for this alpha's GraphQL queries. We do not promise one here.

Opening the PR page performs reads. Merging requires a separate confirmation, a fresh observation, and matching PR head; it does not use an admin bypass. A successful merge never marks the local task Done automatically. Missing `gh`, expired credentials, an inaccessible repository, network errors, or restricted metadata show an error or unknown state, rather than proving that there are no PRs or that checks passed. Retry after fixing the underlying problem. The current PR page accepts canonical `github.com` repositories only; GitHub Enterprise hosts are unsupported. Organization-specific permission combinations are not comprehensively verified.

Current source and the upcoming alpha2 release offer `sbt doctor --github` to check access with actionable results and `sbt agent-prompt` for agent-assisted setup. The published v0.4.0-alpha.1 lacks these commands. They have no account switcher or automatic login. See [developer onboarding](developer-onboarding.md). Installing Switchbard also does not install or authenticate Claude Code or Codex.

## Manual download

A terminal release contains these platform archives with matching `.sha256` files:

- `switchbard-tui-macos-arm64.tar.gz`
- `switchbard-tui-macos-x86_64.tar.gz`
- `switchbard-tui-linux-x86_64.tar.gz`

Download the matching pair from [GitHub Releases](https://github.com/benpchandler/switchbard/releases). Verify before extracting, using `shasum -a 256 -c ARCHIVE.sha256` on macOS or `sha256sum -c ARCHIVE.sha256` on Linux. Each archive contains `sb`, `sbt`, and `LICENSE` at its root. Keep both tools from the same release. A checksum verifies the download against the release's checksum; these alpha binaries are not independently signed or notarized.

## First launch

Run `sbt --repo /path/to/your/git/repo`. An unconfigured repository offers explained, cancellable setup with suggested task references and workflow stages. Accept the suggestions or customize them. New workspaces use the local central database; existing legacy workspaces keep their authority. Press `?` for help, `t` then `n` to create a task, Enter to inspect it, and `q` to quit.

For setup without opening the UI:

```sh
sbt --repo /path/to/repo init
```

For unattended setup, add `--yes`. See [onboarding and recovery](sbt-onboarding.md) for customization and failure behavior.

## Upgrade and recovery

Public installation enables no automatic updater. Back up important local data, read the release notes, and select the new tag explicitly:

```sh
sb --repo /path/to/repo storage backup --file /private/path/switchbard-backup.sqlite3
installer=$(mktemp)
curl -fsSL https://raw.githubusercontent.com/benpchandler/switchbard/vNEXT/scripts/install-release.sh -o "$installer"
bash "$installer" --version vNEXT --replace
rm -f "$installer"
```

Replace `vNEXT` with a published TUI release tag. `--replace` is an explicit binary replacement; it does not restore or downgrade database state. Keep legacy task files in your normal backup policy as well. See [central storage](central-storage.md) for verified restore into a new database destination. The author's launchd source auto-install loop is separate and opt-in; it is unnecessary for release users.

## Build from source

Install Rust 1.95 or newer, Git, and a native C toolchain (Xcode Command Line Tools on macOS; a C compiler/linker on Linux). Lua is built from vendored source. No GUI desktop libraries or mission sidecar are needed for these two targets.

```sh
git clone https://github.com/benpchandler/switchbard.git
cargo build --locked --manifest-path switchbard/Cargo.toml --release -p switchbard-task -p switchbard-tui
./switchbard/target/release/sbt --repo /path/to/your/repo
```

These commands use Cargo's default target directory. If `CARGO_TARGET_DIR` is set, binaries are under that directory instead. To install the pair yourself, choose an empty directory on your `PATH`, or explicitly back up any existing binaries before replacing them:

```sh
mkdir -p "$HOME/.local/bin"
install -m 755 switchbard/target/release/sb "$HOME/.local/bin/sb"
install -m 755 switchbard/target/release/sbt "$HOME/.local/bin/sbt"
export PATH="$HOME/.local/bin:$PATH"
```

These copy commands replace files at the destination; they do not apply the maintainer installer's lineage checks. Run them only when you have chosen to replace that pair. Use the actual build directory if you set `CARGO_TARGET_DIR`.

### Configure your own workspace

Run `sbt --repo /path/to/your/git/repo` and choose customization at the setup prompt, or supply your settings explicitly:

```sh
sbt --repo /path/to/your/git/repo init --yes \
  --task-prefix APP --status Inbox --status Doing --status Done
```

The first stage is the initial status; keep `Done` for completed tasks. Prefixes normalize to uppercase. New workspaces use the local central database and create no repository `backlog/` directory. Re-running setup preserves existing settings; these flags configure a new workspace, rather than reset one. Existing legacy workspaces retain their storage until an explicit migration. See [onboarding](sbt-onboarding.md) and [storage and recovery](central-storage.md).

Building from source does not require a Claude or Codex account. Install and authenticate your chosen agent separately; install `gh` and run `gh auth login` only if you want GitHub features. See [agent workflows](agent-workflows.md) to connect your sessions through CLI instructions.

Contributors using Mise should follow [CLAUDE.md](../CLAUDE.md) for the guarded source installer and repository gates.

## Report an alpha bug

Use [GitHub Issues](https://github.com/benpchandler/switchbard/issues) with your `sbt build-id`, OS, terminal, steps, and visible error. Diagnostic events stay locally at `~/.switchbard/tui-events.jsonl`; review paths and task details before sharing logs. `:bug` and `:idea` in the TUI capture tasks in your current repository and do not publish them to GitHub.
