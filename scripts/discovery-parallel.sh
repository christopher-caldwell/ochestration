#!/usr/bin/env bash
set -euo pipefail

launch_dir="${1:?usage: discovery-parallel.sh <launch-dir>}"
orchestrate_bin="${ORCHESTRATE_BIN:-orchestrate}"

launch_json="$launch_dir/launch.json"
[ -f "$launch_json" ] || {
    printf 'launch.json not found at %s\n' "$launch_json" >&2
    exit 1
}

effort="$(jq -r '.effort' "$launch_json")"
root="$(jq -r '.root' "$launch_json")"
count="$(jq '.providers | length' "$launch_json")"

pids=()
for i in $(seq 0 $((count - 1))); do
    slot="$(jq -r ".providers[$i].slot" "$launch_json")"
    interactive="$(jq -r ".providers[$i].interactive" "$launch_json")"
    workspace="$(jq -r ".providers[$i].workspace" "$launch_json")"
    log_dir="$(jq -r ".providers[$i].log_dir" "$launch_json")"
    prompt="$(jq -r ".providers[$i].prompt" "$launch_json")"
    run="$(jq -r ".providers[$i].run" "$launch_json")"
    mkdir -p "$log_dir"

    if [ "$interactive" = "true" ]; then
        cmd="$(jq -r ".providers[$i].command | join(\" \")" "$launch_json")"
        printf '\n[interactive] slot=%s\n' "$slot"
        printf '  workspace: %s\n' "$workspace"
        printf '  run:       %s\n' "$run"
        printf '  command:   %s\n' "$cmd"
        printf '  prompt:    %s\n' "$prompt"
        printf '  Open the provider in the workspace, follow the prompt, then finalize manually.\n'
        continue
    fi

    cmd_args=()
    while IFS= read -r arg; do
        cmd_args+=("$arg")
    done < <(jq -r ".providers[$i].command[]" "$launch_json")

    (
        cd "$workspace"
        ORCHESTRATE_SLOT="$slot" \
        ORCHESTRATE_EFFORT="$effort" \
        ORCHESTRATE_RUN="$run" \
        ORCHESTRATE_WORKSPACE="$workspace" \
            "${cmd_args[@]}" >"$log_dir/stdout.log" 2>"$log_dir/stderr.log"
        printf '%s\n' "$?" >"$log_dir/exit"
    ) &
    pids+=("$!")
done

for pid in "${pids[@]}"; do
    wait "$pid" || true
done

printf '\nAll headless jobs finished. Current effort status:\n'
"$orchestrate_bin" --root "$root" status --effort "$effort"
