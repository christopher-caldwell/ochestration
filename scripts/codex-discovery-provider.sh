#!/bin/sh
set -eu

# A narrow Codex CLI adapter for the documented Discovery provider protocol. It deliberately
# receives only the supplied packet on stdin and runs from that packet's frozen source path.
packet_dir=$(mktemp -d "${TMPDIR:-/tmp}/orchestrate-codex-provider.XXXXXX")
trap 'rm -rf "$packet_dir"' EXIT HUP INT TERM
packet="$packet_dir/packet.json"
result="$packet_dir/result.json"
cat > "$packet"
source_path=$(jq -r '.frozen_source' "$packet")
if [ -z "$source_path" ] || [ "$source_path" = "null" ]; then
  echo "packet lacks frozen_source" >&2
  exit 64
fi

codex exec --ephemeral --skip-git-repo-check --sandbox read-only --model gpt-5.6-luna \
  --output-schema "$(dirname "$0")/opinion-schema.json" --output-last-message "$result" \
  --cd "$source_path" \
  'You are one independent Discovery investigator. Read the JSON packet and its guide, and inspect only its frozen source workspace. Do not inspect parent directories, user homes, peer records, or unrelated files. Return only a DiscoverySubmission matching the schema: a standalone technical specification and evidence nodes. Record source references and dependencies. Mark a required question blocked only for a genuine product blocker.' \
  < "$packet" >/dev/null
cat "$result"
