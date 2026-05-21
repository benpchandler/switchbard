# Installing Switchbard on macOS

Switchbard alpha builds are distributed as an unnotarized DMG without Developer ID
signing. That keeps the first release simple, but macOS will warn the first time
you open the app.

## Install From The DMG

1. Download `Switchbard-v0.1.1-macos-arm64.dmg` from the GitHub Release.
2. Open the DMG.
3. Drag `Switchbard.app` into `Applications`.
4. In `Applications`, right-click `Switchbard.app` and choose `Open`.
5. Click `Open` again when macOS says the developer cannot be verified.

After the first successful open, Switchbard launches normally.

If macOS does not show an `Open` button, go to `System Settings` -> `Privacy &
Security` and use `Open Anyway` for Switchbard.

## What To Expect

Switchbard is local-first. It stores configuration at `~/.switchbard/config.toml` and writes
service logs under `$TMPDIR/switchbard-logs/`. It scans local listening processes and
git worktrees; it does not send your repo data anywhere.

## Verify The Download

Each release includes a `.sha256` file next to the DMG:

```sh
shasum -a 256 -c Switchbard-v0.1.1-macos-arm64.dmg.sha256
```

The command should print `OK`.

## Build Your Own Copy

You need Rust `1.95.0` with `rustfmt` and `clippy`. Mise is the recommended
way to match the CI toolchain, but it is not required if your Rust install
already matches.

```sh
git clone https://github.com/benpchandler/switchbard
cd switchbard
mise trust
mise install
mise run package
open target/dist/Switchbard-v0.1.1-macos-arm64.dmg
```

Without mise:

```sh
git clone https://github.com/benpchandler/switchbard
cd switchbard
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings
RUSTFLAGS="-D warnings" cargo test --workspace --all-targets
bash scripts/package-dmg.sh
open target/dist/Switchbard-v0.1.1-macos-arm64.dmg
```

## Current Limits

Switchbard is alpha software and macOS-only. The release is not Developer ID signed or
notarized yet, so use it only if you are comfortable installing an app directly
from this repository.
