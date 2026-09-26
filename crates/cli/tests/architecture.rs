//! Guards the instruction-ownership boundary: most skills dispatch, Build never launches the CLI.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn skill_dirs(root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    for entry in fs::read_dir(root.join("skills")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    names
}

fn split_skill(text: &str) -> (String, String) {
    let rest = text
        .strip_prefix("---\n")
        .expect("SKILL.md must open with frontmatter");
    let (frontmatter, body) = rest
        .split_once("\n---\n")
        .expect("SKILL.md frontmatter must close");
    (frontmatter.to_owned(), body.trim().to_owned())
}

fn guide_commands() -> Vec<&'static str> {
    orchestrate_guides::ACTIONS
        .iter()
        .chain(orchestrate_guides::INTERNAL)
        .map(|(name, _)| *name)
        .collect()
}

#[test]
fn every_installed_skill_has_exactly_one_public_guide() {
    let skills = skill_dirs(&repo_root());
    let actions: Vec<_> = orchestrate_guides::ACTIONS
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect();
    assert_eq!(skills, actions);
}

#[test]
fn skills_are_dispatchers_only() {
    let root = repo_root();
    for name in skill_dirs(&root) {
        let text = fs::read_to_string(root.join("skills").join(&name).join("SKILL.md")).unwrap();
        let (frontmatter, body) = split_skill(&text);
        let entries = flat_frontmatter(&frontmatter, &name);
        let keys: Vec<_> = entries.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(
            keys,
            ["name", "description", "disable-model-invocation"],
            "{name} frontmatter may only carry dispatcher metadata"
        );
        assert_eq!(entries[0].1, name);
        assert_dispatcher_description(&name, &entries[1].1);
        assert_eq!(entries[2].1, "true");
        if name == "build" {
            assert!(body.contains("`$build` is not authorization"));
            assert!(body.contains("Do not execute `orchestrate build guide`"));
            assert!(body.contains("Never infer launch authorization from"));
            assert!(!body.contains("You must run `orchestrate"));
        } else {
            assert_eq!(
                body,
                format!("You must run `orchestrate {name} guide` for instructions.")
            );
        }
    }
}

fn flat_frontmatter(frontmatter: &str, name: &str) -> Vec<(String, String)> {
    frontmatter
        .lines()
        .map(|line| {
            assert!(
                !line.is_empty() && !line.starts_with([' ', '-', '#']),
                "{name} frontmatter must stay flat key/value metadata"
            );
            let (key, value) = line.split_once(": ").unwrap_or_else(|| {
                panic!("{name} frontmatter line must be `key: value`, got {line:?}")
            });
            assert!(
                !value.contains(':'),
                "{name} frontmatter value must not carry nested guidance"
            );
            (key.to_owned(), value.to_owned())
        })
        .collect()
}

fn assert_dispatcher_description(name: &str, description: &str) {
    let valid = if name == "build" {
        description == "Prepare or explain an Orchestrate Build without starting its CLI driver."
    } else {
        description.starts_with("Load the current Orchestrate ")
            && description.ends_with(" instructions.")
    };
    assert!(valid, "{name} description must stay a dispatcher label");
    assert!(
        description.len() <= 72,
        "{name} description is long enough to carry procedure"
    );
    assert!(
        !description.contains('`') && !description.contains("orchestrate "),
        "{name} description must not embed commands"
    );
}

#[test]
fn skill_directories_hold_no_operational_assets() {
    let root = repo_root();
    for name in skill_dirs(&root) {
        let mut files = Vec::new();
        visit(&root.join("skills").join(&name), &mut files);
        for file in files {
            let relative = file
                .strip_prefix(root.join("skills").join(&name))
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            assert!(
                relative == "SKILL.md" || relative == "agents/openai.yaml",
                "{name} contains operational asset {relative}"
            );
            if relative == "agents/openai.yaml" {
                assert_eq!(
                    fs::read_to_string(&file).unwrap(),
                    "policy:\n  allow_implicit_invocation: false\n",
                    "{name} host metadata must stay an invocation policy"
                );
            }
        }
    }
}

fn visit(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            visit(&path, files);
        } else {
            files.push(path);
        }
    }
}

#[test]
fn guide_commands_are_bootstrap_safe_and_canonical() {
    let temp = std::env::temp_dir().join(format!(
        "orchestrate-guide-bootstrap-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).unwrap();
    for action in guide_commands() {
        let output = Command::new(env!("CARGO_BIN_EXE_orchestrate"))
            .args([action, "guide"])
            .current_dir(&temp)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{action} guide failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = orchestrate_guides::guide(action).unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            expected,
            "{action} guide output drifted from the embedded resource"
        );
        assert!(!expected.is_empty());
    }
    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn codex_install_docs_use_the_current_global_skill_directory() {
    let root = repo_root();
    let install = fs::read_to_string(root.join("docs/guides/agent-installation.md")).unwrap();
    assert!(install.contains("~/.agents/skills"));
    assert!(!install.contains("~/.codex/skills"));
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(agents.contains("~/.agents/skills/"));
}

#[test]
fn skill_installer_replaces_dispatchers_without_backups_and_preserves_unrelated_skills() {
    let root = repo_root();
    let destination =
        std::env::temp_dir().join(format!("orchestrate-skill-install-{}", std::process::id()));
    let _ = fs::remove_dir_all(&destination);
    fs::create_dir_all(destination.join("discovery")).unwrap();
    fs::write(destination.join("discovery/old.txt"), "old").unwrap();
    fs::create_dir_all(destination.join("unrelated")).unwrap();
    fs::write(destination.join("unrelated/keep.txt"), "keep").unwrap();
    let status = Command::new(root.join("scripts/install-skills.sh"))
        .arg(&destination)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(destination.join("unrelated/keep.txt").is_file());
    assert!(!destination.join("discovery/old.txt").exists());
    for name in skill_dirs(&root) {
        assert!(destination.join(name).join("SKILL.md").is_file());
    }
    assert!(fs::read_dir(&destination).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".bak")
    }));
    let _ = fs::remove_dir_all(&destination);
}

#[test]
fn the_old_guide_subcommand_is_gone() {
    let output = Command::new(env!("CARGO_BIN_EXE_orchestrate"))
        .args(["guide", "discovery"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn reconcile_guide_is_closed_world_convergent_and_stops_before_build() {
    let guide = orchestrate_guides::RECONCILE;
    assert!(guide.contains("Reconcile is synthesis only"));
    assert!(guide.contains("Do not inspect the target repository"));
    assert!(guide.contains("Do not run tests or experiments"));
    assert!(guide.contains("vendor documentation, or the web"));
    assert!(guide.contains("Produce one leading direction"));
    assert!(guide.contains("experiment` is not an automatic winner"));
    assert!(guide.contains("Then stop"));
    assert!(!guide.contains("reconcile adopt"));
    assert!(!guide.contains("approve Build"));

    let rust = fs::read_to_string(repo_root().join("crates/reconcile/src/lib.rs")).unwrap();
    assert!(!rust.contains("Verification::Experiment"));
    assert!(!rust.contains("Verification::Inspection"));
}

#[test]
fn embedded_guides_preserve_high_value_instruction_boundaries() {
    assert!(orchestrate_guides::BUILD.contains("## Human-controlled CLI boundary"));
    assert!(orchestrate_guides::BUILD.contains("## Authority and preparation"));
    assert!(orchestrate_guides::BUILD.contains("## Launch and durable state"));
    assert!(orchestrate_guides::BUILD.contains("explicitly launch `build --effort <id>`"));
    assert!(orchestrate_guides::BUILD.contains("Continuation never resets"));
    assert!(
        orchestrate_guides::WORK.contains("complete Reconciled Discovery is binding authority")
    );
    assert!(orchestrate_guides::REVIEW.contains("detailed plan"));
    assert!(
        orchestrate_guides::FINAL_AUDIT.contains("Assess only the exact implementation artifact")
    );
    assert!(orchestrate_guides::UNBLOCK.contains("Unblock is sessionless and diagnosis-only"));
    assert!(orchestrate_guides::UNBLOCK.contains("do not modify product files"));
    for guide in [
        orchestrate_guides::PREP_DISCOVERY_TICKET,
        orchestrate_guides::PREP_DISCOVERY_FREEFORM,
    ] {
        assert!(guide.contains("expand the user's home directory"));
        assert!(!guide.contains("absolute `~/.orchestration` path"));
    }
}

#[test]
fn prepare_guides_challenge_intent_without_preempting_discovery() {
    for guide in [
        orchestrate_guides::PREP_DISCOVERY_TICKET,
        orchestrate_guides::PREP_DISCOVERY_FREEFORM,
    ] {
        assert!(guide.contains("lightweight intent challenge"));
        assert!(guide.contains("user-owned intent"));
        assert!(guide.contains("zero questions is valid"));
        assert!(guide.contains("not a completeness checklist or fixed questionnaire"));
        assert!(guide.contains("Do not inspect the target repository"));
        assert!(guide.contains("Do not diagnose the problem"));
        assert!(guide.contains("direct and unmistakable user requirement"));
        assert!(guide.contains("Do not infer frozen constraints"));
        assert!(guide.contains("genuine uncertainty remains an explicit unknown"));
        assert!(guide.contains("deliberate delegation"));
        assert!(guide.contains("review/edit it before Discovery"));
    }

    let ticket = orchestrate_guides::PREP_DISCOVERY_TICKET;
    assert!(ticket.contains("Read the user-selected original ticket"));
    assert!(ticket.contains("reproduce the original ticket verbatim"));
    assert!(!ticket.contains("<!--"));
    assert!(!ticket.contains("placeholder"));
}

#[test]
fn host_files_are_install_pointers_only() {
    let root = repo_root();
    assert_install_pointer(
        &fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        "AGENTS.md",
        "~/.agents/skills/",
        "ordinary repository work.",
    );
    assert_install_pointer(
        &fs::read_to_string(root.join("CLAUDE.md")).unwrap(),
        "CLAUDE.md",
        "~/.claude/skills/",
        "ordinary repository work.",
    );
    let cursor = fs::read_to_string(root.join(".cursor/rules/orchestration.mdc")).unwrap();
    assert!(
        cursor.len() <= 600,
        "orchestration.mdc is too large to stay install-only"
    );
    let (frontmatter, body) = split_skill(&cursor);
    let entries = flat_frontmatter(&frontmatter, "orchestration.mdc");
    assert_eq!(
        entries
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        ["description", "alwaysApply"]
    );
    assert!(entries[0].1.len() <= 120);
    assert!(entries[0].1.to_ascii_lowercase().contains("install"));
    assert_eq!(entries[1].1, "false");
    assert_install_body(
        &body,
        "orchestration.mdc",
        "~/.cursor/skills/",
        "ordinary code-editing requests.",
    );
}

fn assert_install_pointer(text: &str, label: &str, skill_dir: &str, scope: &str) {
    assert!(
        text.starts_with("# "),
        "{label} must open as a single install pointer"
    );
    assert_eq!(
        text.lines().filter(|line| line.starts_with('#')).count(),
        1,
        "{label} must not grow workflow sections"
    );
    assert_install_body(text, label, skill_dir, scope);
}

fn assert_install_body(text: &str, label: &str, skill_dir: &str, scope: &str) {
    assert!(
        text.len() <= 600,
        "{label} is too large to stay install-only"
    );
    assert!(
        !text.contains("```"),
        "{label} must not carry command blocks"
    );
    assert!(
        !text.contains("~/.codex/skills"),
        "{label} must use the current Codex skill location"
    );
    let paragraphs: Vec<_> = text
        .trim()
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .collect();
    assert!(
        paragraphs.len() <= 3,
        "{label} has {} paragraphs; keep it an install pointer",
        paragraphs.len()
    );
    for paragraph in &paragraphs {
        assert!(
            paragraph.len() <= 360,
            "{label} paragraph is {} bytes; keep it an install pointer",
            paragraph.len()
        );
        for line in paragraph.lines() {
            let trimmed = line.trim_start();
            assert!(
                !trimmed.starts_with(['-', '*'])
                    && !trimmed.starts_with(|c: char| c.is_ascii_digit()),
                "{label} must not grow a procedure list"
            );
        }
    }
    assert!(
        text.contains("docs/guides/agent-installation.md"),
        "{label} must point at the shared installation guide"
    );
    assert!(
        text.contains("install or update"),
        "{label} must stay limited to installation"
    );
    assert!(text.contains(skill_dir), "{label} skill location drifted");
    assert!(
        text.contains("Do not apply") && text.contains(scope),
        "{label} must exclude ordinary work"
    );
}
