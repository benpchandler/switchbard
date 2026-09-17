#!/usr/bin/env bash
# Install published sb/sbt binaries. No compiler or language runtime required.
set -euo pipefail

fail() { printf 'switchbard: %s\n' "$*" >&2; exit 1; }
usage() {
  printf '%s\n' 'Usage: install-release.sh [--version vX.Y.Z[-alpha.N]] [--bin-dir DIR] [--replace]' \
    'Default: latest stable release, ~/.local/bin. --replace explicitly replaces existing binaries.'
}

version=latest
bin_dir="${HOME:?HOME must be set}/.local/bin"
replace=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version|--bin-dir)
      [[ $# -ge 2 && -n "$2" ]] || fail "$1 needs a value"
      if [[ "$1" == --version ]]; then version="$2"; else bin_dir="$2"; fi
      shift 2 ;;
    --replace) replace=true; shift ;;
    --help|-h) usage; exit 0 ;;
    *) fail "unknown argument: $1" ;;
  esac
done
[[ "$version" == latest || "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$ ]] || fail 'version must be latest or a release tag such as v0.4.0-alpha.1'
case "$(uname -s)/$(uname -m)" in
  Darwin/arm64) platform=macos-arm64 ;;
  Darwin/x86_64) platform=macos-x86_64 ;;
  Linux/x86_64) platform=linux-x86_64 ;;
  *) fail 'supported platforms: macOS arm64/x86_64 and Linux x86_64' ;;
esac
for command in curl tar mktemp install mv cp chmod mkdir rm touch; do
  command -v "$command" >/dev/null || fail "required system tool missing: $command"
done
if command -v sha256sum >/dev/null; then hash_command=(sha256sum);
elif command -v shasum >/dev/null; then hash_command=(shasum -a 256);
else fail 'sha256sum or shasum is required'; fi
mkdir -p "$bin_dir"
bin_dir="$(cd "$bin_dir" && pwd)"
for binary in sb sbt; do
  if [[ -e "$bin_dir/$binary" || -L "$bin_dir/$binary" ]]; then
    [[ "$replace" == true ]] || fail "$bin_dir/$binary already exists; use --replace to explicitly replace it"
    [[ ! -d "$bin_dir/$binary" ]] || fail "$bin_dir/$binary is a directory"
  fi
done
scratch="$(mktemp -d "$bin_dir/.switchbard-install.XXXXXXXX")"
committing=false
success=false
cleanup() {
  local binary
  if [[ "$committing" == true && "$success" != true ]]; then
    for binary in sb sbt; do
      if [[ -e "$scratch/old-$binary" || -L "$scratch/old-$binary" ]]; then
        mv -f "$scratch/old-$binary" "$bin_dir/$binary"
      elif [[ -f "$scratch/replaced-$binary" ]]; then rm -f "$bin_dir/$binary"; fi
    done
  fi
  rm -rf "$scratch"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
asset="switchbard-tui-$platform.tar.gz"
base=https://github.com/benpchandler/switchbard/releases
if [[ "$version" == latest ]]; then base="$base/latest/download";
else base="$base/download/$version"; fi
for suffix in '' .sha256; do
  curl --fail --location --silent --show-error --connect-timeout 15 --max-time 180 \
    --proto '=https' --proto-redir '=https' "$base/$asset$suffix" -o "$scratch/$asset$suffix"
done
read -r expected filename extra < "$scratch/$asset.sha256"
[[ "$expected" =~ ^[a-fA-F0-9]{64}$ && "$filename" == "$asset" && -z "${extra:-}" ]] || fail 'invalid release checksum file'
actual="$("${hash_command[@]}" "$scratch/$asset")"
[[ "${actual%% *}" == "$expected" ]] || fail 'release checksum mismatch; nothing installed'
# Extract only named regular executables, never arbitrary archive paths/symlinks.
for binary in sb sbt; do
  entry="$(tar -tvzf "$scratch/$asset" "$binary")"
  [[ "$entry" == -* && "$entry" != *$'\n'* ]] || fail "archive member $binary must be one regular file"
  tar -xOzf "$scratch/$asset" "$binary" > "$scratch/$binary"
  [[ -s "$scratch/$binary" ]] || fail "release contains an empty $binary"
  chmod 755 "$scratch/$binary"
  "$scratch/$binary" --version
  if [[ -e "$bin_dir/$binary" || -L "$bin_dir/$binary" ]]; then
    cp -P "$bin_dir/$binary" "$scratch/old-$binary"
  fi
done
committing=true
for binary in sb sbt; do
  touch "$scratch/replaced-$binary"
  mv -f "$scratch/$binary" "$bin_dir/$binary"
done
success=true
printf 'Installed sb and sbt to %s\n' "$bin_dir"
case ":${PATH:-}:" in
  *":$bin_dir:"*) ;;
  *) printf "Add this directory to your shell PATH: export PATH=\"%s:\$PATH\"\n" "$bin_dir" ;;
esac
printf '%s\n' \
  'From your git repository:' \
  '  sbt doctor                         Check Git, workspace and terminal readiness' \
  '  sbt                                Open the terminal app and set up your workspace' \
  '  sbt doctor --github                Optional: check GitHub CLI and authentication' \
  '  sbt agent-prompt                   Print a copyable setup prompt for your agent' \
  '  sbt skill install --agent both     Optional: install Claude and Codex instructions' \
  'Use --agent claude or --agent codex to install instructions for only that agent.' \
  'Existing system Git and GitHub CLI are reused.' \
  'Switchbard is alpha software. Back up important task data before upgrading.'
