//! Guards the instruction-ownership boundary: skills dispatch, the CLI instructs.

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
        assert!(
            frontmatter
                .lines()
                .any(|line| line == format!("name: {name}")),
            "{name} frontmatter name must match its directory"
        );
        assert!(
            frontmatter.contains("disable-model-invocation: true"),
            "{name} must stay explicit-only for hosts that read this field"
        );
        assert_eq!(
            body,
            format!("Run `orchestrate {name} guide` and follow the returned instructions.")
        );
    }
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
fn the_old_guide_subcommand_is_gone() {
    let output = Command::new(env!("CARGO_BIN_EXE_orchestrate"))
        .args(["guide", "discovery"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn host_files_do_not_carry_runtime_commands() {
    let root = repo_root();
    let forbidden = [
        "discovery prepare",
        "discovery finalize",
        "reconcile inputs",
        "reconcile finalize",
        "reconcile adopt",
        "audit finalize",
        "orchestrate init",
        "$discovery",
        "$reconcile",
        "$build",
        "$audit",
        "prep-discovery-ticket guide",
    ];
    for relative in ["AGENTS.md", "CLAUDE.md", ".cursor/rules/orchestration.mdc"] {
        let text = fs::read_to_string(root.join(relative)).unwrap();
        for phrase in forbidden {
            assert!(
                !text.contains(phrase),
                "{relative} contains runtime instruction {phrase:?}"
            );
        }
    }
}
