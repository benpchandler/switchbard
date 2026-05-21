# Installing Hive on macOS

Hive alpha builds are distributed as an unnotarized DMG without Developer ID
signing. That keeps the first release simple, but macOS will warn the first time
you open the app.

## Install From The DMG

1. Download `Hive-v0.1.0-macos-arm64.dmg` from the GitHub Release.
2. Open the DMG.
3. Drag `Hive.app` into `Applications`.
4. In `Applications`, right-click `Hive.app` and choose `Open`.
5. Click `Open` again when macOS says the developer cannot be verified.

After the first successful open, Hive launches normally.

If macOS does not show an `Open` button, go to `System Settings` -> `Privacy &
Security` and use `Open Anyway` for Hive.

## What To Expect

Hive is local-first. It stores configuration at `~/.hive/config.toml` and writes
service logs under `$TMPDIR/hive-logs/`. It scans local listening processes and
git worktrees; it does not send your repo data anywhere.

## Verify The Download

Each release includes a `.sha256` file next to the DMG:

```sh
shasum -a 256 -c Hive-v0.1.0-macos-arm64.dmg.sha256
```

The command should print `OK`.

## Build Your Own Copy

```sh
git clone https://github.com/benpchandler/hive
cd hive
mise trust
mise install
mise run package
open target/dist/Hive-v0.1.0-macos-arm64.dmg
```

## Current Limits

Hive is alpha software and macOS-only. The release is not Developer ID signed or
notarized yet, so use it only if you are comfortable installing an app directly
from this repository.
