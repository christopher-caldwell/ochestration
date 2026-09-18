#!/bin/sh
set -eu
packet_dir=$(mktemp -d "${TMPDIR:-/tmp}/orchestrate-codex-consensus.XXXXXX")
trap 'rm -rf "$packet_dir"' EXIT HUP INT TERM
packet="$packet_dir/packet.json"
result="$packet_dir/result.json"
cat > "$packet"
codex exec --ephemeral --skip-git-repo-check --sandbox read-only --model gpt-5.6-luna \
  --output-schema "$(dirname "$0")/proposal-schema.json" --output-last-message "$result" \
  'You are the Consensus reconciler. Read the supplied JSON packet and guide only. Do not inspect a source repository or private run state. Return a JSON ConsensusProposal with comparison markdown and per-requirement supporter maps. Preserve governing constraints, distinguish support from silence, retain dissent, and adopt only a coherent package with one common strict majority.' \
  < "$packet" >/dev/null
cat "$result"
