#!/usr/bin/env bash
set -uo pipefail

usage() {
  cat <<'TXT'
Capture one Orchestrate Build run, or collect an existing run's evidence.

Required:
  EFFORT=<effort-id>

Optional:
  MODE=collect|run                Default: collect (never dispatches the Build)
  ORCHESTRATE_ROOT=<store root>   Default: ~/.orchestration
  PROJECT_ROOT=<git repository>   Default: current repository
  CAPTURE_ROOT=<output parent>    Default: ~/orchestrate-build-captures

MODE=collect gathers the effort's durable evidence through
`orchestrate build export`, which selects its members before traversal and
verifies the archive. It dispatches nothing, so it is safe on a stopped,
blocked or historically failed run.

MODE=run additionally invokes `orchestrate build` first, exactly as an operator
would, and then collects the same evidence.

Examples:
  EFFORT="effort-abc123" bash docs/examples/capture-build-run.sh
  MODE=run EFFORT="effort-abc123" bash docs/examples/capture-build-run.sh
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

MODE="${MODE:-collect}"
case "$MODE" in
  collect|run) ;;
  *) fail "MODE must be collect or run" ;;
esac

ORCHESTRATE_ROOT="${ORCHESTRATE_ROOT:-$HOME/.orchestration}"
PROJECT_ROOT="${PROJECT_ROOT:-$PWD}"
CAPTURE_ROOT="${CAPTURE_ROOT:-$HOME/orchestrate-build-captures}"

PROJECT_ROOT="$(git -C "$PROJECT_ROOT" rev-parse --show-toplevel 2>/dev/null)" \
  || fail "PROJECT_ROOT is not inside a Git repository"
PROJECT_ROOT="$(cd "$PROJECT_ROOT" && pwd -P)"

abspath() {
  python3 - "$1" <<'PY'
import os, sys
print(os.path.abspath(os.path.expanduser(sys.argv[1])))
PY
}
ORCHESTRATE_ROOT="$(abspath "$ORCHESTRATE_ROOT")"
CAPTURE_ROOT="$(abspath "$CAPTURE_ROOT")"

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
mkdir -p "$CAPTURE_DIR/capture" "$CAPTURE_DIR/git"

status_before="$CAPTURE_DIR/capture/status.before.json"
if ! orchestrate --root "$ORCHESTRATE_ROOT" build status --effort "$EFFORT" >"$status_before" 2>"$CAPTURE_DIR/capture/status.before.stderr.log"; then
  cat "$CAPTURE_DIR/capture/status.before.stderr.log" >&2
  fail "could not resolve effort before collection"
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
  printf 'mode=%s\n' "$MODE"
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

build_exit=0
started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf '%s\n' "$started_at" >"$CAPTURE_DIR/capture/started-at.txt"
if [[ "$MODE" == "run" ]]; then
  (
    cd "$PROJECT_ROOT" || exit 1
    RUST_BACKTRACE="${RUST_BACKTRACE:-1}" \
    RUST_LIB_BACKTRACE="${RUST_LIB_BACKTRACE:-1}" \
      orchestrate --root "$ORCHESTRATE_ROOT" build --effort "$EFFORT"
  ) >"$CAPTURE_DIR/capture/controller.stdout.log" \
    2>"$CAPTURE_DIR/capture/controller.stderr.log"
  build_exit=$?
else
  printf 'MODE=collect: no Build was dispatched and no provider was invoked.\n' \
    >"$CAPTURE_DIR/capture/controller.stdout.log"
  : >"$CAPTURE_DIR/capture/controller.stderr.log"
fi
finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf '%s\n' "$finished_at" >"$CAPTURE_DIR/capture/finished-at.txt"
printf '%s\n' "$build_exit" >"$CAPTURE_DIR/capture/process-exit-code.txt"

orchestrate --root "$ORCHESTRATE_ROOT" build status --effort "$EFFORT" \
  >"$CAPTURE_DIR/capture/status.after.json" \
  2>"$CAPTURE_DIR/capture/status.after.stderr.log" || true
orchestrate --root "$ORCHESTRATE_ROOT" journal --effort "$EFFORT" \
  >"$CAPTURE_DIR/capture/journal.json" \
  2>"$CAPTURE_DIR/capture/journal.stderr.log" || true

# The effort's evidence comes from the exporter, which selects its expected
# members from durable state before traversing anything, excludes bulky
# generated and scratch trees by class, and verifies membership and hashes
# before promoting its archive. The wrapper never copies an effort tree itself.
export_archive="$CAPTURE_DIR/effort-evidence.zip"
export_json="$CAPTURE_DIR/capture/export.json"
export_exit=0
orchestrate --root "$ORCHESTRATE_ROOT" build export --effort "$EFFORT" --output "$export_archive" \
  >"$export_json" 2>"$CAPTURE_DIR/capture/export.stderr.log"
export_exit=$?

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

python3 - "$CAPTURE_DIR" "$started_at" "$finished_at" "$build_exit" "$export_exit" "$MODE" <<'PY'
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
export = None
export_path = root / 'capture' / 'export.json'
if export_path.exists():
    try:
        export = json.loads(export_path.read_text(encoding='utf-8'))
    except json.JSONDecodeError:
        export = None
cleanup_record = None
status_after = root / 'capture' / 'status.after.json'
if status_after.exists():
    try:
        cleanup_record = json.loads(status_after.read_text(encoding='utf-8'))['details'].get('cleanup')
    except (json.JSONDecodeError, KeyError, TypeError):
        cleanup_record = None
manifest = {
    'format': 2,
    'mode': sys.argv[6],
    'started_at_utc': sys.argv[2],
    'finished_at_utc': sys.argv[3],
    'build_process_exit_code': int(sys.argv[4]),
    'export_process_exit_code': int(sys.argv[5]),
    'operation_status': operation,
    'semantic_outcome': semantic,
    'export_status': None if export is None else export.get('semantic_outcome'),
    # Cleanup is a separate operator command; the wrapper records whatever the
    # controller last recorded about it and never runs it itself.
    'last_cleanup_record': cleanup_record,
    'notes': [
        'Build semantic outcome, cleanup outcome and export outcome are separate facts; a nonzero export exit never implies a failed Build and a passing Build never hides a failed export. This wrapper never runs cleanup.',
        'The effort evidence archive is effort-evidence.zip, produced by `orchestrate build export`, which selects expected members before traversal and verifies membership and hashes before promotion.',
        'Provider structured stdout is retained in each Build action transport.jsonl; raw provider stderr in provider-stderr.log; per-dispatch facts in invocation.json.',
        'Native provider home/session directories are intentionally not swept because they may contain unrelated private conversations.',
        'Contained Git review/audit/once-over checkouts are excluded by class; their exact commits are recorded in the exported Build state and git-evidence/checkout.json.'
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

printf 'Build process exit code: %s\n' "$build_exit"
printf 'Export process exit code: %s\n' "$export_exit"
printf 'Capture directory: %s\n' "$CAPTURE_DIR"
printf 'Archive: %s\n' "$archive"
printf 'Interpret capture/manifest.json: build process exit, export exit and semantic outcome are independent facts.\n'

if [[ "$export_exit" -ne 0 ]]; then
  printf 'capture-build-run: evidence export failed; the effort evidence archive is not a complete capture\n' >&2
  exit "$export_exit"
fi
exit "$build_exit"
