#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

fake_bin="$scratch/bin"
test_repo="$scratch/repo"
bare_repo="$scratch/remote.git"
hook_log="$scratch/hooks.log"
mkdir -p "$fake_bin" "$test_repo" "$bare_repo"

cat > "$fake_bin/mise" <<'FAKE_MISE'
#!/usr/bin/env bash
set -euo pipefail
for name in GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR GIT_INDEX_FILE GIT_OBJECT_DIRECTORY GIT_NAMESPACE; do
  if [[ -n "${!name-}" ]]; then
    echo "$name leaked into mise" >&2
    exit 91
  fi
done
printf '%s\n' "$*" >> "$HOOK_TEST_LOG"
if [[ "${HOOK_TEST_FAIL_TASK-}" == "${2-}" ]]; then
  exit 42
fi
FAKE_MISE
chmod +x "$fake_bin/mise"

git -C "$test_repo" init -q
git -C "$test_repo" config user.name "Switchbard Hook Test"
git -C "$test_repo" config user.email "hooks@example.invalid"
cp -R "$repo_root/.githooks" "$test_repo/.githooks"
printf 'baseline\n' > "$test_repo/README.md"
git -C "$test_repo" add .githooks README.md
git -C "$test_repo" commit -q -m baseline

PATH="$fake_bin:$PATH" HOOK_TEST_LOG="$hook_log" \
  "$repo_root/scripts/install-git-hooks.sh" --repo "$test_repo" >/dev/null
if [[ "$(git -C "$test_repo" config --local --get core.hooksPath)" != ".githooks" ]]; then
  echo "hook installer did not use a worktree-relative path" >&2
  exit 1
fi

# Direct execution proves every Git variable is scrubbed before mise starts.
env PATH="$fake_bin:$PATH" HOOK_TEST_LOG="$hook_log" \
  GIT_DIR=sentinel GIT_WORK_TREE=sentinel GIT_COMMON_DIR=sentinel \
  GIT_INDEX_FILE=sentinel GIT_OBJECT_DIRECTORY=sentinel GIT_NAMESPACE=sentinel \
  "$test_repo/.githooks/pre-commit"
before_no_mistakes="$(wc -l < "$hook_log" | tr -d ' ')"
PATH="$fake_bin:$PATH" HOOK_TEST_LOG="$hook_log" \
  "$test_repo/.githooks/pre-push" no-mistakes ignored >/dev/null
after_no_mistakes="$(wc -l < "$hook_log" | tr -d ' ')"
if [[ "$before_no_mistakes" != "$after_no_mistakes" ]]; then
  echo "pre-push duplicated the gate before a no-mistakes handoff" >&2
  exit 1
fi

printf 'commit hook\n' > "$test_repo/commit.txt"
git -C "$test_repo" add commit.txt
PATH="$fake_bin:$PATH" HOOK_TEST_LOG="$hook_log" \
  git -C "$test_repo" commit -q -m "exercise pre-commit"

git -C "$bare_repo" init -q --bare
git -C "$test_repo" remote add origin "$bare_repo"
PATH="$fake_bin:$PATH" HOOK_TEST_LOG="$hook_log" \
  git -C "$test_repo" push -q -u origin HEAD
remote_before="$(git -C "$bare_repo" rev-parse HEAD)"

printf 'blocked commit\n' > "$test_repo/blocked-commit.txt"
git -C "$test_repo" add blocked-commit.txt
if PATH="$fake_bin:$PATH" HOOK_TEST_LOG="$hook_log" HOOK_TEST_FAIL_TASK=fmt \
  git -C "$test_repo" commit -q -m "must not commit"; then
  echo "pre-commit did not block a formatting failure" >&2
  exit 1
fi
git -C "$test_repo" restore --staged blocked-commit.txt
rm "$test_repo/blocked-commit.txt"

printf 'blocked push\n' > "$test_repo/blocked-push.txt"
git -C "$test_repo" add blocked-push.txt
PATH="$fake_bin:$PATH" HOOK_TEST_LOG="$hook_log" \
  git -C "$test_repo" commit -q -m "exercise failing pre-push"
if PATH="$fake_bin:$PATH" HOOK_TEST_LOG="$hook_log" HOOK_TEST_FAIL_TASK=preflight \
  git -C "$test_repo" push -q 2> "$scratch/expected-push-failure.log"; then
  echo "pre-push did not block a preflight failure" >&2
  exit 1
fi
if [[ "$(git -C "$bare_repo" rev-parse HEAD)" != "$remote_before" ]]; then
  echo "remote advanced despite a failed pre-push gate" >&2
  exit 1
fi

# Relative hooksPath must resolve to the linked worktree's checked-out hooks.
linked="$scratch/linked"
git -C "$test_repo" worktree add -q -b linked "$linked"
printf 'linked worktree\n' > "$linked/linked.txt"
git -C "$linked" add linked.txt
PATH="$fake_bin:$PATH" HOOK_TEST_LOG="$hook_log" \
  git -C "$linked" commit -q -m "exercise linked worktree hook"

fmt_count="$(grep -c '^run fmt$' "$hook_log")"
preflight_count="$(grep -c '^run preflight$' "$hook_log")"
if [[ "$fmt_count" -lt 5 || "$preflight_count" -ne 2 ]]; then
  echo "unexpected hook invocations: fmt=$fmt_count preflight=$preflight_count" >&2
  cat "$hook_log" >&2
  exit 1
fi

echo "Git hooks: PASS"
