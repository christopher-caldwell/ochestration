#!/bin/sh
set -eu
packet_dir=$(mktemp -d "${TMPDIR:-/tmp}/orchestrate-codex-audit.XXXXXX")
trap 'rm -rf "$packet_dir"' EXIT HUP INT TERM
packet="$packet_dir/packet.json"
result="$packet_dir/result.json"
cat > "$packet"
target_source=$(jq -r '.target_source' "$packet")
if [ -z "$target_source" ] || [ "$target_source" = "null" ]; then
  echo "packet lacks target_source" >&2
  exit 64
fi
codex exec --ephemeral --skip-git-repo-check --sandbox read-only --model gpt-5.6-luna \
  --output-schema "$(dirname "$0")/audit-schema.json" --output-last-message "$result" \
  --cd "$target_source" \
  'You are a fresh independent Audit assessor. Read the supplied JSON packet and guide. Audit its exact Agreement/implementation pair without editing source or authority. Return exactly one coverage row per Agreement requirement. Use pass only for attributable evidence, fail for demonstrated deviations, unknown for missing verification, and justified not_applicable only when the requirement does not apply. Put a bounded correction in the row when one is needed and use an empty correction otherwise. Do not invent requirements.' \
  < "$packet" >/dev/null
cat "$result"
