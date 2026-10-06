use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

use crate::command_display::{command_prefix, shell_quote};

pub(crate) fn render(result: &Value, editor: Option<&str>, root: Option<&Path>) -> Result<String> {
    let effort = result["effort"]
        .as_str()
        .context("import result does not include its effort ID")?;
    let build_dir = result["build_dir"]
        .as_str()
        .map(std::path::PathBuf::from)
        .context("import result does not include its Build directory")?;
    let config = build_dir.join("config.toml");
    let config = config
        .canonicalize()
        .with_context(|| format!("cannot resolve imported agent config {}", config.display()))?;
    let plan: Value = serde_json::from_slice(
        &std::fs::read(build_dir.join("plan.json")).context("cannot read imported Build plan")?,
    )
    .context("cannot parse imported Build plan")?;
    let phases = plan["phases"]
        .as_array()
        .context("Build plan has no phase list")?;
    let command = command_prefix(root);
    let build = format!("{command} build --effort {}", shell_quote(effort));
    let edit = match editor {
        Some(editor) if !editor.trim().is_empty() => {
            format!("{editor} {}", shell_quote(&config.display().to_string()))
        }
        _ => shell_quote(&config.display().to_string()),
    };

    let mut output =
        format!("Successfully imported {effort}.\n\nTo edit the agent config:\n-> {edit}\n\n");
    if phases.is_empty() {
        output.push_str(
            "This import has no Build phases yet. Add phases to plan.json before launching Build.\n\n",
        );
    }
    output.push_str(&format!("To start the build:\n-> {build}\n"));
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};

    fn fixture(phases: &[&str]) -> (std::path::PathBuf, Value) {
        let nonce = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("orchestrate-import-display-{nonce} user's"));
        let build_dir = root.join("Build With Space");
        fs::create_dir_all(&build_dir).unwrap();
        fs::write(build_dir.join("config.toml"), "config\n").unwrap();
        fs::write(
            build_dir.join("plan.json"),
            serde_json::json!({"phases": phases}).to_string(),
        )
        .unwrap();
        let result = serde_json::json!({
            "effort": "effort-123",
            "build_dir": build_dir,
        });
        (root, result)
    }

    #[test]
    fn import_output_uses_the_config_path_quotes_spaces_and_reports_missing_phases() {
        let (root, result) = fixture(&[]);
        let output = render(&result, None, None).unwrap();
        assert!(output.contains("Successfully imported effort-123."));
        assert!(output.contains("To edit the agent config:\n-> '"));
        assert!(output.contains("Build With Space/config.toml'"));
        assert!(output.contains("'\\''s/Build With Space/config.toml'"));
        assert!(output.contains("Add phases to plan.json before launching Build."));
        assert!(output.contains("orchestrate build --effort effort-123"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn import_output_preserves_editor_arguments_and_includes_an_explicit_store_root() {
        let (root, result) = fixture(&["phase_01_delivery"]);
        let store_root = root.join("store's data");
        let output = render(&result, Some("code --wait"), Some(&store_root)).unwrap();
        assert!(output.contains("code --wait '"));
        assert!(output.contains("orchestrate --root '"));
        assert!(output.contains("build --effort effort-123"));
        assert!(!output.contains("no Build phases"));
        fs::remove_dir_all(root).unwrap();
    }
}
