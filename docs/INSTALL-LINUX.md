# Installing Switchbard on Linux

A prebuilt **x86_64** binary tarball is attached to each
[GitHub Release](https://github.com/benpchandler/switchbard/releases)
(`switchbard-vX.Y.Z-linux-x86_64.tar.gz`). There is no `.deb`, `.rpm`, or
`.AppImage` yet, and no prebuilt ARM build — build from source for those.

## Quick install (x86_64)

Download `switchbard-*-linux-x86_64.tar.gz` from the
[Releases page](https://github.com/benpchandler/switchbard/releases), then:

```sh
tar -xzf switchbard-*-linux-x86_64.tar.gz
cd switchbard-*-linux-x86_64
sha256sum -c ../switchbard-*-linux-x86_64.tar.gz.sha256   # optional integrity check
./switchbard
```

The prebuilt binary still needs the runtime shared libraries listed below
(libxkbcommon, Wayland/X11, libGL) plus `xdg-utils` — any normal desktop session
already has them. If launching fails with a missing `.so`, install the packages
from the next section.

## Requirements (build from source)

- Rust `1.95.0` with `rustfmt` and `clippy`
- `git`
- X11 or Wayland desktop session
- `xdg-utils` for opening ports in the default browser
- `xdg-desktop-portal` for the native folder picker on most desktops

On Ubuntu/Debian:

```sh
sudo apt-get install git build-essential pkg-config libxkbcommon-dev \
  libwayland-dev libx11-dev libxcb1-dev libxcb-render0-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libgl1-mesa-dev \
  xdg-utils xdg-desktop-portal
```

On Fedora:

```sh
sudo dnf install git gcc pkgconf-pkg-config libxkbcommon-devel \
  wayland-devel libX11-devel libxcb-devel mesa-libGL-devel \
  xdg-utils xdg-desktop-portal
```

## Build And Run

```sh
git clone https://github.com/benpchandler/switchbard
cd switchbard
cargo build --release -p switchbard-gui
./target/release/switchbard
```

Or install the binary onto your Cargo path:

```sh
cargo install --git https://github.com/benpchandler/switchbard --bin switchbard
switchbard
```

## Notes

Switchbard scans listening TCP sockets through `/proc/net/tcp`,
`/proc/net/tcp6`, and `/proc/<pid>/fd`, so Linux does not need `lsof`.

Configuration lives at `~/.switchbard/config.toml`. Logs of services
Switchbard started land in `$TMPDIR/switchbard-logs/`.
