#!/bin/sh
# Build the isolated environment the historical recovery validation runs
# against.  It copies the real records byte for byte, repoints the project
# record at a disposable checkout, and never writes to the original store.
#
#   usage: prepare-isolated-environment.sh <work-directory> <product-source-repo>
#
# Then run, from the repository checkout:
#
#   ORCHESTRATE_HISTORICAL_RECOVERY_ROOT=<work-directory> \
#     cargo test -p orchestrate-build --test historical_recovery -- --ignored --nocapture
set -eu

work=$1
source_repo=$2
store=${ORCHESTRATE_STORE:-$HOME/.orchestration}
project=${ORCHESTRATE_PROJECT:-ai_orchestration}
effort=${ORCHESTRATE_EFFORT:-build-reliability-and-recovery}

rm -rf "$work"
mkdir -p "$work/store/projects"
# -a preserves the records' bytes and timestamps; the copy is independent of
# the original afterwards.  Only the recovered project's records are copied;
# the other projects in the store are unrelated to this effort.
cp -a "$store/store.json" "$work/store/store.json"
cp -a "$store/projects/$project" "$work/store/projects/$project"
# A disposable checkout with its own object store: nothing the validation
# commits can reach the product repository.
git clone --quiet --no-hardlinks "$source_repo" "$work/product"
rm -rf "$work/product/.claude"
rm -rf "$work/store/projects/$project/efforts/$effort/build/evidence"

python3 - "$work" "$project" <<'PY'
import json, os, sys

work, project = sys.argv[1], sys.argv[2]
path = os.path.join(work, "store", "projects", project, "project.json")
record = json.load(open(path))
if record["canonical_locator"] != os.path.realpath(os.path.join(work, "product")):
    print("project", record["id"], record["canonical_locator"], "->", os.path.realpath(os.path.join(work, "product")))
record["canonical_locator"] = os.path.realpath(os.path.join(work, "product"))
open(path, "w").write(json.dumps(record, indent=2) + "\n")
PY

echo "isolated environment ready at $work"
