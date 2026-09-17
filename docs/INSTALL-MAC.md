# Installing Switchbard on macOS

Switchbard's supported interface is now the terminal UI. Install `sb` and `sbt` using [the TUI installation guide](INSTALL-TUI.md). Release targets include Apple Silicon (`arm64`) and Intel (`x86_64`). The terminal package does not require `Switchbard.app`, a DMG, or the bundled GUI mission sidecar.

## Deprecated desktop GUI

The desktop GUI is deprecated. Older [GitHub Releases](https://github.com/benpchandler/switchbard/releases) may include `Switchbard-*-macos-arm64.dmg`; these are historical GUI builds, not the current TUI install.

To use an older GUI build, verify its matching `.sha256` file with `shasum -a 256 -c`, open the DMG, and drag `Switchbard.app` into Applications. Those builds are not Developer ID signed or notarized. Use Finder's Control-click → Open flow, or System Settings → Privacy & Security → Open Anyway if macOS blocks the first launch.

GUI configuration lives at `~/.switchbard/config.toml`; service logs are under `$TMPDIR/switchbard-logs/`. GUI source remains in `crates/switchbard-gui`. Its historical bundle workflow requires the pinned xplan sidecar inputs described in [CLAUDE.md](../CLAUDE.md); these are unnecessary for building the terminal tools.
