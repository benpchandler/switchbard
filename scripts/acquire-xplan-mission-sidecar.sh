#!/usr/bin/env bash
# Acquire one already-built xplan helper only after every tracked pin matches.
set -euo pipefail

fail() {
  echo "xplan sidecar acquisition rejected: $1" >&2
  exit 1
}

PIN=""
SOURCE=""
ARCHIVE=""
TARGET_OS=""
TARGET_ARCH=""
MODE=""
DESTINATION=""
while (($#)); do
  case "$1" in
    --pin) PIN="${2:-}"; shift 2 ;;
    --source) SOURCE="${2:-}"; shift 2 ;;
    --artifact) ARCHIVE="${2:-}"; shift 2 ;;
    --target-os) TARGET_OS="${2:-}"; shift 2 ;;
    --arch) TARGET_ARCH="${2:-}"; shift 2 ;;
    --mode) MODE="${2:-}"; shift 2 ;;
    --destination) DESTINATION="${2:-}"; shift 2 ;;
    *) fail "unknown or incomplete argument" ;;
  esac
done

[[ -f "$PIN" ]] || fail "pin is unavailable"
[[ -d "$SOURCE/.git" || -f "$SOURCE/.git" ]] || fail "source checkout is unavailable"
[[ -f "$ARCHIVE" ]] || fail "artifact archive is unavailable"
case "$TARGET_OS-$TARGET_ARCH" in
  macos-arm64) TARGET_KEY="macos-arm64" ;;
  linux-x86_64) TARGET_KEY="linux-x86_64" ;;
  *) fail "target must be macos arm64 or linux x86_64" ;;
esac
[[ "$MODE" == "local" || "$MODE" == "ci" ]] || fail "mode must be local or ci"
[[ -n "$DESTINATION" && "$DESTINATION" != "/" ]] || fail "destination is unsafe"
[[ ! -e "$DESTINATION" ]] || fail "destination already exists"

PIN_FIELDS="$(python3 - "$PIN" "$TARGET_KEY" <<'PY'
import json, pathlib, re, sys
path = pathlib.Path(sys.argv[1])
target_key = sys.argv[2]
def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise SystemExit(f"duplicate pin field: {key}")
        result[key] = value
    return result
try:
    data = json.loads(path.read_text(), object_pairs_hook=unique_object)
except (OSError, ValueError):
    raise SystemExit("pin JSON is invalid") from None
required = {
    "schema_version", "xplan_repository", "xplan_source_revision", "uv_lock_sha256",
    "protocol_version", "operations", "writer", "runtime_model", "targets",
}
if set(data) != required:
    raise SystemExit("pin keys are not exact")
if data["schema_version"] != 1:
    raise SystemExit("pin schema mismatch")
if data["protocol_version"] != "xplan-mission-sidecar-v1":
    raise SystemExit("protocol pin mismatch")
if data["operations"] != ["hello", "queue_mission", "get_pending_decision", "resume_decision"]:
    raise SystemExit("operation pin mismatch")
if data["writer"] != "xplan" or data["runtime_model"] != "one-shot":
    raise SystemExit("authority pin mismatch")
if not isinstance(data["targets"], dict) or set(data["targets"]) != {"macos-arm64", "linux-x86_64"}:
    raise SystemExit("target pin map is not exact")
for name, expected in {
    "macos-arm64": ("macos", "arm64"),
    "linux-x86_64": ("linux", "x86_64"),
}.items():
    entry = data["targets"][name]
    if not isinstance(entry, dict) or set(entry) != {"target_os", "arch", "archive_name", "manifest_sha256"}:
        raise SystemExit(f"target pin entry is invalid: {name}")
    if any(not isinstance(value, str) or not value for value in entry.values()):
        raise SystemExit(f"target pin field is invalid: {name}")
    if (entry["target_os"], entry["arch"]) != expected:
        raise SystemExit(f"target pin tuple is invalid: {name}")
    if re.fullmatch(r"[0-9a-f]{64}", entry["manifest_sha256"]) is None:
        raise SystemExit(f"target manifest digest is invalid: {name}")
target = data["targets"].get(target_key)
target_required = {"target_os", "arch", "archive_name", "manifest_sha256"}
if not isinstance(target, dict) or set(target) != target_required:
    raise SystemExit("selected target pin is invalid")
expected = {
    "macos-arm64": ("macos", "arm64"),
    "linux-x86_64": ("linux", "x86_64"),
}[target_key]
if (target["target_os"], target["arch"]) != expected:
    raise SystemExit("selected target tuple is invalid")
archives = {entry["archive_name"] for entry in data["targets"].values()}
manifests = {entry["manifest_sha256"] for entry in data["targets"].values()}
if len(archives) != 2 or len(manifests) != 2:
    raise SystemExit("target artifact identities must be unique")
values = []
for key in ("xplan_repository", "xplan_source_revision", "uv_lock_sha256"):
    value = data[key]
    if not isinstance(value, str) or not value:
        raise SystemExit(f"invalid pin field: {key}")
    values.append(value)
if re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", data["xplan_source_revision"]) is None:
    raise SystemExit("source revision pin is invalid")
if re.fullmatch(r"[0-9a-f]{64}", data["uv_lock_sha256"]) is None:
    raise SystemExit("lock digest pin is invalid")
for key in ("target_os", "arch", "archive_name", "manifest_sha256"):
    value = target[key]
    if not isinstance(value, str) or not value:
        raise SystemExit(f"invalid target pin field: {key}")
    values.append(value)
if re.fullmatch(r"[0-9a-f]{64}", target["manifest_sha256"]) is None:
    raise SystemExit("manifest digest pin is invalid")
print("\t".join(values))
PY
)" || fail "pin JSON is invalid"
[[ "$PIN_FIELDS" != *$'\n'* ]] || fail "pin JSON is invalid"
IFS=$'\t' read -r PIN_REPOSITORY PIN_REVISION PIN_LOCK PIN_TARGET PIN_ARCH PIN_ARCHIVE PIN_MANIFEST <<< "$PIN_FIELDS"

[[ -n "$PIN_MANIFEST" ]] || fail "pin JSON is invalid"

SOURCE_REVISION="$(git -C "$SOURCE" rev-parse HEAD 2>/dev/null)" || fail "source revision is unavailable"
[[ "$SOURCE_REVISION" == "$PIN_REVISION" ]] || fail "source revision does not match pin"
[[ -z "$(git -C "$SOURCE" status --porcelain --untracked-files=normal 2>/dev/null)" ]] || fail "source checkout is dirty"
SOURCE_REMOTE="$(git -C "$SOURCE" remote get-url origin 2>/dev/null)" || fail "source repository is unavailable"
python3 - "$SOURCE_REMOTE" "$PIN_REPOSITORY" <<'PY' || fail "source repository does not match pin"
import re, sys
def canonical(value: str) -> str:
    value = re.sub(r"^git@github\.com:", "https://github.com/", value)
    value = re.sub(r"^ssh://git@github\.com/", "https://github.com/", value)
    return value.removesuffix("/").removesuffix(".git").lower()
raise SystemExit(0 if canonical(sys.argv[1]) == canonical(sys.argv[2]) else 1)
PY

[[ -f "$SOURCE/sidecar/uv.lock" ]] || fail "pinned lockfile is unavailable"
LOCK_DIGEST="$(shasum -a 256 "$SOURCE/sidecar/uv.lock" | awk '{print $1}')"
[[ "$LOCK_DIGEST" == "$PIN_LOCK" ]] || fail "lock digest does not match pin"
[[ "$(basename "$ARCHIVE")" == "$PIN_ARCHIVE" ]] || fail "archive name does not match pin"

STAGE="$(mktemp -d "${TMPDIR:-/tmp}/switchbard-sidecar.XXXXXX")"
cleanup() { rm -rf "$STAGE"; }
trap cleanup EXIT
python3 - "$ARCHIVE" "$STAGE" <<'PY' || fail "artifact archive is unsafe or unreadable"
import pathlib, tarfile, sys
archive, target = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]).resolve()
with tarfile.open(archive, "r:gz") as packed:
    for member in packed.getmembers():
        candidate = (target / member.name).resolve()
        if target not in candidate.parents and candidate != target:
            raise SystemExit("archive path escapes extraction root")
        if member.issym() or member.islnk():
            link = (candidate.parent / member.linkname).resolve()
            if target not in link.parents and link != target:
                raise SystemExit("archive link escapes extraction root")
    packed.extractall(target, filter="data")
PY

MANIFEST="$(find "$STAGE" -name manifest.json -type f -print)"
[[ -n "$MANIFEST" && "$(printf '%s\n' "$MANIFEST" | wc -l | tr -d ' ')" == "1" ]] || fail "archive must contain exactly one manifest"
MANIFEST_DIGEST="$(shasum -a 256 "$MANIFEST" | awk '{print $1}')"
[[ "$MANIFEST_DIGEST" == "$PIN_MANIFEST" ]] || fail "manifest digest does not match pin"
ARTIFACT_ROOT="$(dirname "$MANIFEST")"
VERIFY="$SOURCE/scripts/verify_mission_sidecar_artifact.py"
[[ -x "$VERIFY" ]] || fail "artifact verifier is unavailable"
"$VERIFY" \
  --manifest "$MANIFEST" \
  --artifact "$ARTIFACT_ROOT" \
  --expected-manifest-digest "$PIN_MANIFEST" \
  --expected-source "$PIN_REVISION" \
  --expected-lock "$PIN_LOCK" \
  --expected-target "$PIN_TARGET" \
  --expected-arch "$PIN_ARCH" >/dev/null || fail "artifact verification failed"

HELPER_REL="$(python3 - "$MANIFEST" <<'PY'
import json, pathlib, sys
data = json.loads(pathlib.Path(sys.argv[1]).read_text())
matches = [item["path"] for item in data["files"] if pathlib.PurePosixPath(item["path"]).name == "xplan-mission-sidecar"]
if len(matches) != 1:
    raise SystemExit("helper identity is not unique")
print(matches[0])
PY
)" || fail "helper identity is invalid"
[[ -x "$ARTIFACT_ROOT/$HELPER_REL" ]] || fail "helper is not executable"
mkdir -p "$(dirname "$DESTINATION")"
mv "$ARTIFACT_ROOT" "$DESTINATION"
trap - EXIT
rm -rf "$STAGE"

python3 - "$MODE" "$TARGET_KEY" "$DESTINATION/$HELPER_REL" "$DESTINATION/manifest.json" "$PIN_REVISION" "$PIN_MANIFEST" <<'PY'
import json, pathlib, sys
print(json.dumps({
    "mode": sys.argv[1],
    "target": sys.argv[2],
    "helper_path": str(pathlib.Path(sys.argv[3]).resolve()),
    "manifest_path": str(pathlib.Path(sys.argv[4]).resolve()),
    "source_revision": sys.argv[5],
    "manifest_digest": sys.argv[6],
}, sort_keys=True))
PY
