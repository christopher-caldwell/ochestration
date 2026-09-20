default:
    @just --list

# Run the prepared Discovery launch manifest: `just discovery-parallel <launch-dir>`
discovery-parallel launch_dir:
    @bash scripts/discovery-parallel.sh {{launch_dir}}
