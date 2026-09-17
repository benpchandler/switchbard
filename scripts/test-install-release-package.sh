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
  bash "$root/scripts/install-release.sh" --version v0.4.0-alpha.2 --bin-dir "$scratch/home/bin with spaces"
binary_dir="$scratch/home/bin with spaces"
package_path="$binary_dir:$PATH"
"$binary_dir/sb" --version
"$binary_dir/sbt" --version
git -C "$scratch/repo" init
for attempt in 1 2; do
  printf 'Setup attempt %s\n' "$attempt"
  env HOME="$scratch/home" PATH="$package_path" "$binary_dir/sbt" --repo "$scratch/repo" init --yes
done
env HOME="$scratch/home" PATH="$package_path" "$binary_dir/sb" --repo "$scratch/repo" create 'Installed release smoke task'
env HOME="$scratch/home" PATH="$package_path" "$binary_dir/sb" --repo "$scratch/repo" list | grep -F 'Installed release smoke task'
env HOME="$scratch/home" PATH="$package_path" "$binary_dir/sbt" --repo "$scratch/repo" doctor --json > "$scratch/doctor.json"
ruby -rjson -e '
  report = JSON.parse(File.read(ARGV.fetch(0)))
  abort "Unexpected doctor schema" unless report.fetch("schema_version") == 1
  abort "Required doctor failure" unless report.fetch("required_failed") == false
  checks = report.fetch("checks").to_h { |check| [check.fetch("id"), check] }
  %w[git sqlite database workspace].each do |id|
    abort "Unhealthy #{id}" unless checks.fetch(id).fetch("status") == "ok"
  end
'  "$scratch/doctor.json"
env HOME="$scratch/home" PATH="$package_path" "$binary_dir/sbt" agent-prompt > "$scratch/agent-prompt"
[[ -s "$scratch/agent-prompt" ]]
env HOME="$scratch/home" PATH="$package_path" "$binary_dir/sbt" skill install --agent both
for skill in "$scratch/home/.claude/skills/switchbard/SKILL.md" "$scratch/home/.agents/skills/switchbard/SKILL.md"; do
  [[ -s "$skill" ]]
done
cmp "$scratch/home/.claude/skills/switchbard/SKILL.md" "$scratch/home/.agents/skills/switchbard/SKILL.md"
printf '%s\n' 'actual release package installation, onboarding, diagnostics and agent skills passed'
