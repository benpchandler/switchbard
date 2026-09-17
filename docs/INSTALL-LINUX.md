# Installing Switchbard on Linux

Switchbard's supported interface is now the terminal UI. Install `sb` and `sbt` using [the TUI installation guide](INSTALL-TUI.md). The supported public release targets and source-build instructions are maintained there. There is no `.deb`, `.rpm`, or AppImage package.

The TUI runs in a terminal without X11, Wayland, OpenGL, or the GUI mission sidecar. GitHub and agent integrations require their respective optional tools. See the installation guide for runtime requirements and source builds.

## Deprecated desktop GUI

The desktop GUI is deprecated. Older [GitHub Releases](https://github.com/benpchandler/switchbard/releases) may contain `switchbard-*-linux-x86_64.tar.gz`, whose executable is `switchbard`. These are historical GUI packages, not the current `sb`/`sbt` terminal package.

GUI source remains in `crates/switchbard-gui`. Building it requires a native desktop session and the relevant X11/Wayland, xkbcommon, and OpenGL libraries; browser opening uses `xdg-open`. Those desktop dependencies are unnecessary for the terminal tools. The historical Linux GUI package may also include its pinned xplan mission sidecar.
