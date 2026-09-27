set positional-arguments

skill_dir := env('ORCHESTRATE_SKILLS_DIR', env('HOME') + '/.agents/skills')

# Install or refresh only the CLI from this checkout, including same-version changes.
install-only-cli:
    cargo install --path crates/cli --locked --force

# Refresh only the CLI from this checkout.
update-only-cli: install-only-cli

# Install the six checked-in skills (defaults to Codex's personal skill directory).
install-skills destination=skill_dir:
    ./scripts/install-skills.sh "$1"

# Refresh the six checked-in skills.
update-skills destination=skill_dir: (install-skills destination)

# Install the CLI and six skills.
install-cli: install-only-cli install-skills

# Refresh the CLI and six skills.
update-cli: update-only-cli update-skills
