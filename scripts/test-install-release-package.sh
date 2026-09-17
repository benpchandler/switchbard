#!/usr/bin/env bash
# Exercise the public installer with an actual native release archive.
# Network alone is substituted; binaries, checksums and onboarding are real.
set -euo pipefail
[[ $# -eq 1 ]] || { echo 'Usage: test-install-release-package.sh DIST_DIRECTORY' >&2; exit 1; }
root="$(git rev-parse --show-toplevel)"
distribution="$(cd "$1" && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/mock" "$scratch/home" "$scratch/repo"
cat > "$scratch/mock/curl" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
url='' destination=''
while [[ $# -gt 0 ]]; do
  case "$1" in
    https:*) url="$1"; shift ;;
    -o) destination="$2"; shift 2 ;;
    *) shift ;;
  esac
done
[[ -n "$url" && -n "$destination" ]]
cp "$TEST_RELEASE_DIST/${url##*/}" "$destination"
MOCK
chmod +x "$scratch/mock/curl"
env HOME="$scratch/home" PATH="$scratch/mock:$PATH" TEST_RELEASE_DIST="$distribution" \
  bash "$root/scripts/install-release.sh" --version v0.4.0-alpha.1 --bin-dir "$scratch/home/bin with spaces"
binary_dir="$scratch/home/bin with spaces"
"$binary_dir/sb" --version
"$binary_dir/sbt" --version
git -C "$scratch/repo" init
for attempt in 1 2; do
  printf 'Setup attempt %s\n' "$attempt"
  env HOME="$scratch/home" "$binary_dir/sbt" --repo "$scratch/repo" init --yes
done
env HOME="$scratch/home" "$binary_dir/sb" --repo "$scratch/repo" create 'Installed release smoke task'
env HOME="$scratch/home" "$binary_dir/sb" --repo "$scratch/repo" list | grep -F 'Installed release smoke task'
printf '%s\n' 'actual release package installation and onboarding passed'
