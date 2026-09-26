#!/bin/sh
real=/Users/christophercaldwell/.nvm/versions/node/v22.13.1/bin/codex
role=reviewer
prev=
for arg in "$@"; do
  if [ "$prev" = "cwd" ]; then
    case "$arg" in */fixture-worker) role=worker ;; */fixture-unblocker) role=unblocker ;; *) role=reviewer ;; esac
    prev=
    continue
  fi
  if [ "$arg" = "-C" ] || [ "$arg" = "--cd" ]; then prev=cwd; fi
done
sandbox=read-only
if [ "$role" = worker ]; then sandbox=workspace-write; fi
printf '%s\t%s\n' "$role" "$sandbox" >> "$ORCH_VALIDATION_SANDBOX_LOG"
if [ "$1" = exec ]; then
  shift
  exec "$real" exec --sandbox "$sandbox" "$@"
fi
exec "$real" "$@"
