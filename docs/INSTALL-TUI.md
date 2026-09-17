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

The example selects the published first TUI alpha explicitly. Without `--version`, the installer chooses GitHub's latest stable release, which may still be an older GUI-only release. An absent TUI asset produces an error rather than installing the GUI. Use the source route below if you prefer to build your own binaries.

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
