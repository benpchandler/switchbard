#!/usr/bin/env bash
# Mock only the network and platform; exercise real hashing, tar and filesystem writes.
set -euo pipefail
root="$(git rev-parse --show-toplevel)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/mock" "$scratch/package" "$scratch/downloads"
for binary in sb sbt; do
  printf '#!/bin/sh\nprintf "fixture %s\\n"\n' "$binary" > "$scratch/package/$binary"
  chmod +x "$scratch/package/$binary"
done
cat > "$scratch/mock/uname" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
case "$1" in -s) echo "${TEST_OS:-Linux}" ;; -m) echo "${TEST_ARCH:-x86_64}" ;; esac
MOCK
cat > "$scratch/mock/curl" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
[[ "${TEST_DOWNLOAD_FAIL:-0}" != 1 ]] || exit 22
url='' destination=''
while [[ $# -gt 0 ]]; do
  case "$1" in
    https:*) url="$1"; shift ;;
    -o) destination="$2"; shift 2 ;;
    *) shift ;;
  esac
done
printf '%s\n' "$url" >> "$TEST_URL_LOG"
cp "$TEST_DOWNLOADS/${url##*/}" "$destination"
MOCK
chmod +x "$scratch/mock/"*
export PATH="$scratch/mock:$PATH"
export TEST_DOWNLOADS="$scratch/downloads" TEST_URL_LOG="$scratch/urls"
make_release() {
  local platform="$1" asset
  asset="switchbard-tui-$platform.tar.gz"
  tar -C "$scratch/package" -czf "$scratch/downloads/$asset" sb sbt
  if command -v sha256sum >/dev/null; then
    (cd "$scratch/downloads" && sha256sum "$asset" > "$asset.sha256")
  else
    (cd "$scratch/downloads" && shasum -a 256 "$asset" > "$asset.sha256")
  fi
}
run_install() { bash "$root/scripts/install-release.sh" "$@"; }
expect_failure() {
  if "$@" > "$scratch/error" 2>&1; then echo 'expected failure' >&2; exit 1; fi
}
make_release linux-x86_64
run_install --version v0.4.0-alpha.1 --bin-dir "$scratch/path with spaces" > "$scratch/output"
[[ "$("$scratch/path with spaces/sb" --version)" == 'fixture sb' ]]
[[ "$("$scratch/path with spaces/sbt" --version)" == 'fixture sbt' ]]
grep -q '/download/v0.4.0-alpha.1/' "$scratch/urls"
expect_failure run_install --bin-dir "$scratch/path with spaces"
run_install --replace --bin-dir "$scratch/path with spaces" > "$scratch/output"
grep -q '/latest/download/' "$scratch/urls"
# Corrupt download must preserve both installed binaries.
printf corrupt >> "$scratch/downloads/switchbard-tui-linux-x86_64.tar.gz"
expect_failure run_install --replace --bin-dir "$scratch/path with spaces"
grep -q 'checksum mismatch' "$scratch/error"
[[ "$("$scratch/path with spaces/sb")" == 'fixture sb' ]]
[[ "$("$scratch/path with spaces/sbt")" == 'fixture sbt' ]]
# A second binary commit failure restores the original pair.
for binary in sb sbt; do
  printf '#!/bin/sh\nprintf "new fixture %s\\n"\n' "$binary" > "$scratch/package/$binary"
done
make_release linux-x86_64
cat > "$scratch/mock/mv" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
for argument in "$@"; do
  if [[ "$argument" == */.switchbard-install.*/sbt ]]; then exit 1; fi
done
exec "$TEST_REAL_MV" "$@"
MOCK
TEST_REAL_MV="$(command -v mv)"
export TEST_REAL_MV
chmod +x "$scratch/mock/mv"
expect_failure run_install --replace --bin-dir "$scratch/path with spaces"
[[ "$("$scratch/path with spaces/sb")" == 'fixture sb' ]]
[[ "$("$scratch/path with spaces/sbt")" == 'fixture sbt' ]]
rm "$scratch/mock/mv"
export TEST_DOWNLOAD_FAIL=1
expect_failure run_install --bin-dir "$scratch/network failure"
[[ ! -e "$scratch/network failure/sb" && ! -e "$scratch/network failure/sbt" ]]
unset TEST_DOWNLOAD_FAIL
export TEST_OS=Windows
expect_failure run_install --bin-dir "$scratch/unsupported"
[[ ! -e "$scratch/unsupported" ]]
export TEST_OS=Darwin
for arch in arm64 x86_64; do
  export TEST_ARCH="$arch"
  make_release "macos-$arch"
  run_install --bin-dir "$scratch/macos-$arch" > "$scratch/output"
done
expect_failure run_install --version '../../bad'
expect_failure run_install --bin-dir
# A checksum-valid archive with a symlink executable must also be rejected.
rm "$scratch/package/sb"
ln -s /bin/sh "$scratch/package/sb"
export TEST_OS=Linux TEST_ARCH=x86_64
make_release linux-x86_64
expect_failure run_install --bin-dir "$scratch/symlink"
[[ ! -e "$scratch/symlink/sb" && ! -e "$scratch/symlink/sbt" ]]
printf '%s\n' 'release installer tests passed'
