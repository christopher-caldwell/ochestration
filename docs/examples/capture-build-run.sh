#!/usr/bin/env bash
set -uo pipefail

usage() {
  cat <<'TXT'
Capture one Orchestrate Build invocation for later analysis.

Required:
  EFFORT=<effort-id-or-selector>

Optional:
  ORCHESTRATE_ROOT=<store root>   Default: ~/.orchestration
  PROJECT_ROOT=<git repository>  Default: current repository
  CAPTURE_ROOT=<output parent>    Default: ~/orchestrate-build-captures

Example:
  EFFORT="effort-abc123" bash docs/examples/capture-build-run.sh
TXT
}

fail() {
  printf 'capture-build-run: %s\n' "$*" >&2
  exit 2
}

for cmd in orchestrate git python3; do
  command -v "$cmd" >/dev/null 2>&1 || fail "required command not found: $cmd"
done

[[ -n "${EFFORT:-}" ]] || { usage >&2; fail "EFFORT is required"; }

ORCHESTRATE_ROOT="${ORCHESTRATE_ROOT:-$HOME/.orchestration}"
PROJECT_ROOT="${PROJECT_ROOT:-$PWD}"
CAPTURE_ROOT="${CAPTURE_ROOT:-$HOME/orchestrate-build-captures}"

PROJECT_ROOT="$(git -C "$PROJECT_ROOT" rev-parse --show-toplevel 2>/dev/null)" \
  || fail "PROJECT_ROOT is not inside a Git repository"
PROJECT_ROOT="$(cd "$PROJECT_ROOT" && pwd -P)"

ORCHESTRATE_ROOT="$(python3 - "$ORCHESTRATE_ROOT" <<'PY'
import os, sys
print(os.path.abspath(os.path.expanduser(sys.argv[1])))
PY
)"
CAPTURE_ROOT="$(python3 - "$CAPTURE_ROOT" <<'PY'
import os, sys
print(os.path.abspath(os.path.expanduser(sys.argv[1])))
PY
)"

python3 - "$PROJECT_ROOT" "$CAPTURE_ROOT" <<'PY' || fail "CAPTURE_ROOT must be outside the target repository"
import os, sys
project = os.path.realpath(sys.argv[1])
capture = os.path.realpath(sys.argv[2])
try:
    inside = os.path.commonpath([project, capture]) == project
except ValueError:
    inside = False
raise SystemExit(1 if inside else 0)
PY

safe_effort="$(printf '%s' "$EFFORT" | tr -c 'A-Za-z0-9._-' '_')"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
CAPTURE_DIR="$CAPTURE_ROOT/${safe_effort}-${stamp}-$$"
mkdir -p "$CAPTURE_DIR/capture" "$CAPTURE_DIR/git" "$CAPTURE_DIR/store"

status_before="$CAPTURE_DIR/capture/status.before.json"
if ! orchestrate --root "$ORCHESTRATE_ROOT" status --effort "$EFFORT" >"$status_before" 2>"$CAPTURE_DIR/capture/status.before.stderr.log"; then
  cat "$CAPTURE_DIR/capture/status.before.stderr.log" >&2
  fail "could not resolve effort before Build"
fi

BUILD_DIR="$(python3 - "$status_before" <<'PY'
import json, sys
with open(sys.argv[1], encoding='utf-8') as f:
    value = json.load(f)
print(value['details']['build_dir'])
PY
)" || fail "status output did not contain details.build_dir"
EFFORT_DIR="$(dirname "$BUILD_DIR")"

python3 - "$ORCHESTRATE_ROOT" "$EFFORT_DIR" <<'PY' || fail "resolved effort is outside ORCHESTRATE_ROOT"
import os, sys
root = os.path.realpath(sys.argv[1])
effort = os.path.realpath(sys.argv[2])
try:
    inside = os.path.commonpath([root, effort]) == root
except ValueError:
    inside = False
raise SystemExit(0 if inside else 1)
PY

{
  printf 'captured_at_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'project_root=%s\n' "$PROJECT_ROOT"
  printf 'orchestrate_root=%s\n' "$ORCHESTRATE_ROOT"
  printf 'effort=%s\n' "$EFFORT"
  printf 'orchestrate_path=%s\n' "$(command -v orchestrate)"
  orchestrate --version 2>&1 || true
  git --version 2>&1 || true
  uname -a 2>&1 || true
  for provider in codex claude cursor-agent; do
    if command -v "$provider" >/dev/null 2>&1; then
      printf '%s_path=%s\n' "$provider" "$(command -v "$provider")"
      "$provider" --version 2>&1 || true
    fi
  done
} >"$CAPTURE_DIR/capture/versions.txt"

(
  cd "$PROJECT_ROOT" || exit 1
  git rev-parse HEAD >"$CAPTURE_DIR/git/head.before.txt"
  git symbolic-ref --short -q HEAD >"$CAPTURE_DIR/git/branch.before.txt" 2>/dev/null || true
  git status --porcelain=v1 --untracked-files=all >"$CAPTURE_DIR/git/status.before.txt"
)

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf '%s\n' "$started_at" >"$CAPTURE_DIR/capture/started-at.txt"

(
  cd "$PROJECT_ROOT" || exit 1
  RUST_BACKTRACE="${RUST_BACKTRACE:-1}" \
  RUST_LIB_BACKTRACE="${RUST_LIB_BACKTRACE:-1}" \
    orchestrate --root "$ORCHESTRATE_ROOT" build --effort "$EFFORT"
) >"$CAPTURE_DIR/capture/controller.stdout.log" \
  2>"$CAPTURE_DIR/capture/controller.stderr.log"
build_exit=$?

finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf '%s\n' "$finished_at" >"$CAPTURE_DIR/capture/finished-at.txt"
printf '%s\n' "$build_exit" >"$CAPTURE_DIR/capture/process-exit-code.txt"

orchestrate --root "$ORCHESTRATE_ROOT" status --effort "$EFFORT" \
  >"$CAPTURE_DIR/capture/status.after.json" \
  2>"$CAPTURE_DIR/capture/status.after.stderr.log" || true
orchestrate --root "$ORCHESTRATE_ROOT" journal --effort "$EFFORT" \
  >"$CAPTURE_DIR/capture/journal.json" \
  2>"$CAPTURE_DIR/capture/journal.stderr.log" || true

(
  cd "$PROJECT_ROOT" || exit 1
  git rev-parse HEAD >"$CAPTURE_DIR/git/head.after.txt" || true
  git symbolic-ref --short -q HEAD >"$CAPTURE_DIR/git/branch.after.txt" 2>/dev/null || true
  git status --porcelain=v1 --untracked-files=all >"$CAPTURE_DIR/git/status.after.txt" || true
  git diff --binary >"$CAPTURE_DIR/git/unstaged.diff" || true
  git diff --cached --binary >"$CAPTURE_DIR/git/staged.diff" || true
  git log --graph --decorate --oneline --all -n 200 >"$CAPTURE_DIR/git/log.txt" || true
  git ls-files --others --exclude-standard >"$CAPTURE_DIR/git/untracked-files.txt" || true

  before="$(cat "$CAPTURE_DIR/git/head.before.txt" 2>/dev/null || true)"
  after="$(cat "$CAPTURE_DIR/git/head.after.txt" 2>/dev/null || true)"
  if [[ -n "$before" && -n "$after" ]]; then
    git diff --binary "$before" "$after" >"$CAPTURE_DIR/git/build.diff" || true
    git rev-list --reverse "$before..$after" >"$CAPTURE_DIR/git/build-commits.txt" || true
  fi

  mkdir -p "$CAPTURE_DIR/git/untracked"
  while IFS= read -r -d '' file; do
    [[ -f "$file" ]] || continue
    mkdir -p "$CAPTURE_DIR/git/untracked/$(dirname "$file")"
    cp -p "$file" "$CAPTURE_DIR/git/untracked/$file"
  done < <(git ls-files --others --exclude-standard -z)

  git bundle create "$CAPTURE_DIR/git/repository.bundle" --all HEAD \
    >"$CAPTURE_DIR/git/bundle.stdout.log" \
    2>"$CAPTURE_DIR/git/bundle.stderr.log" || true
)

# Preserve the effort at its original store-relative path so artifact references remain easy to follow.
effort_rel="$(python3 - "$ORCHESTRATE_ROOT" "$EFFORT_DIR" <<'PY'
import os, sys
print(os.path.relpath(os.path.realpath(sys.argv[2]), os.path.realpath(sys.argv[1])))
PY
)"
effort_copy="$CAPTURE_DIR/store/$effort_rel"
mkdir -p "$(dirname "$effort_copy")"
cp -R "$EFFORT_DIR" "$effort_copy"

# Contained Build review/audit checkouts are shared Git clones and are not portable by themselves.
# Their exact commits are retained by the Git bundle and matching store snapshots below.
if [[ -d "$effort_copy/build/artifacts" ]]; then
  find "$effort_copy/build/artifacts" -type d \( -name source -o -name verification \) -prune -exec rm -rf {} \; 2>/dev/null || true
fi

for meta in "$ORCHESTRATE_ROOT/store.json" "$(dirname "$(dirname "$EFFORT_DIR")")/project.json"; do
  if [[ -f "$meta" ]]; then
    rel="$(python3 - "$ORCHESTRATE_ROOT" "$meta" <<'PY'
import os, sys
print(os.path.relpath(os.path.realpath(sys.argv[2]), os.path.realpath(sys.argv[1])))
PY
)"
    mkdir -p "$CAPTURE_DIR/store/$(dirname "$rel")"
    cp -p "$meta" "$CAPTURE_DIR/store/$rel"
  fi
done

# Copy only source snapshots actually referenced somewhere in this effort.
python3 - "$effort_copy" "$ORCHESTRATE_ROOT" "$CAPTURE_DIR/store" <<'PY'
import re, shutil, sys
from pathlib import Path

effort = Path(sys.argv[1])
root = Path(sys.argv[2])
out = Path(sys.argv[3])
pattern = re.compile(r'(?<![0-9a-f])[0-9a-f]{40}(?![0-9a-f])')
commits = set()
for path in effort.rglob('*'):
    if not path.is_file() or path.suffix not in {'.json', '.jsonl', '.md', '.toml', '.txt'}:
        continue
    try:
        text = path.read_text(encoding='utf-8', errors='ignore')
    except OSError:
        continue
    commits.update(pattern.findall(text))

for commit in sorted(commits):
    source = root / 'snapshots' / commit
    if not source.is_dir():
        continue
    dest = out / 'snapshots' / commit
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(source, dest, symlinks=True)
PY

python3 - "$CAPTURE_DIR" "$started_at" "$finished_at" "$build_exit" <<'PY'
import json, sys
from pathlib import Path

root = Path(sys.argv[1])
stdout = root / 'capture' / 'controller.stdout.log'
semantic = None
operation = None
if stdout.exists():
    for line in reversed(stdout.read_text(encoding='utf-8', errors='replace').splitlines()):
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict):
            semantic = value.get('semantic_outcome')
            operation = value.get('operation_status')
            if semantic is not None or operation is not None:
                break
manifest = {
    'format': 1,
    'started_at_utc': sys.argv[2],
    'finished_at_utc': sys.argv[3],
    'process_exit_code': int(sys.argv[4]),
    'operation_status': operation,
    'semantic_outcome': semantic,
    'notes': [
        'Provider structured stdout is retained in each Build action transport.jsonl.',
        'Outer provider/controller stderr is retained in capture/controller.stderr.log.',
        'Native provider home/session directories are intentionally not swept because they may contain unrelated private conversations.',
        'Contained shared Git review/audit checkouts are omitted; referenced snapshots and repository.bundle preserve their committed source.'
    ],
}
(root / 'capture' / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
PY

python3 - "$CAPTURE_DIR" <<'PY' >"$CAPTURE_DIR/inventory.sha256"
import hashlib, sys
from pathlib import Path
root = Path(sys.argv[1])
for path in sorted(p for p in root.rglob('*') if p.is_file() and p.name != 'inventory.sha256'):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b''):
            h.update(chunk)
    print(f'{h.hexdigest()}  {path.relative_to(root)}')
PY

archive="$CAPTURE_DIR.zip"
python3 - "$CAPTURE_DIR" "$archive" <<'PY'
import sys, zipfile
from pathlib import Path
root = Path(sys.argv[1])
archive = Path(sys.argv[2])
with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED, allowZip64=True) as zf:
    for path in sorted(root.rglob('*')):
        if path.is_file():
            zf.write(path, Path(root.name) / path.relative_to(root))
PY

printf 'Build exit code: %s\n' "$build_exit"
printf 'Capture directory: %s\n' "$CAPTURE_DIR"
printf 'Archive: %s\n' "$archive"
printf 'Interpret capture/manifest.json semantic_outcome; process exit code alone is not the Build verdict.\n'

exit "$build_exit"
