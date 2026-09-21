#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: $0 /absolute/path/to/host/skills" >&2
  exit 2
fi

destination=$1
case "$destination" in
  /*) ;;
  *) echo "skill destination must be absolute" >&2; exit 2 ;;
esac
if [ "$destination" = "/" ]; then
  echo "refusing to use / as a skill destination" >&2
  exit 2
fi

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository=$(CDPATH= cd -- "$script_dir/.." && pwd)
mkdir -p "$destination"

for skill in prep-discovery-ticket prep-discovery-freeform discovery reconcile build audit; do
  target="$destination/$skill"
  rm -rf -- "$target"
  cp -R "$repository/skills/$skill" "$target"
done

for skill in prep-discovery-ticket prep-discovery-freeform discovery reconcile build audit; do
  test -f "$destination/$skill/SKILL.md"
done

echo "Installed six Orchestrate dispatchers in $destination"
