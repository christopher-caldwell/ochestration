use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

pub const PLACEHOLDERS: [&str; 8] = [
    "slot",
    "effort",
    "run",
    "root",
    "workspace",
    "source",
    "prompt",
    "log",
];

#[derive(Debug, Deserialize)]
pub struct ProviderPlan {
    #[serde(default)]
    pub quorum: Option<QuorumValue>,
    #[serde(default)]
    pub provider: Vec<ProviderSpec>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum QuorumValue {
    Text(String),
    Count(usize),
}

#[derive(Debug, Deserialize)]
pub struct ProviderSpec {
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_effort: Option<String>,
    #[serde(default)]
    pub interactive: bool,
    pub command: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LaunchEntry {
    pub slot: String,
    pub run: String,
    pub workspace: String,
    pub source: String,
    pub prompt: String,
    pub log_dir: String,
    pub command: Vec<String>,
    pub interactive: bool,
}

#[derive(Debug, Serialize)]
pub struct LaunchManifest {
    pub effort: String,
    pub root: String,
    pub slots: Vec<String>,
    pub quorum: usize,
    pub providers: Vec<LaunchEntry>,
}

impl ProviderPlan {
    pub fn read(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read provider plan {}", path.display()))?;
        let plan: ProviderPlan = toml::from_str(&text)
            .with_context(|| format!("invalid provider plan {}", path.display()))?;
        plan.validate()?;
        Ok(plan)
    }

    pub fn slots(&self) -> Vec<String> {
        self.provider.iter().map(|spec| spec.name.clone()).collect()
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            !self.provider.is_empty(),
            "provider plan must declare at least two providers"
        );
        ensure!(
            self.provider.len() >= 2,
            "provider plan must declare at least two providers"
        );
        let mut seen = HashMap::new();
        for spec in &self.provider {
            ensure!(
                valid_label(&spec.name),
                "invalid provider slot name {}",
                spec.name
            );
            ensure!(
                seen.insert(spec.name.as_str(), ()).is_none(),
                "duplicate provider slot {}",
                spec.name
            );
            ensure!(
                !spec.host.trim().is_empty(),
                "provider {} needs a host",
                spec.name
            );
            ensure!(
                !spec.command.is_empty(),
                "provider {} needs a command",
                spec.name
            );
            for arg in &spec.command {
                validate_placeholders(arg)?;
            }
        }
        Ok(())
    }
}

pub fn expand_command(template: &[String], vars: &HashMap<String, String>) -> Result<Vec<String>> {
    let mut expanded = Vec::with_capacity(template.len());
    for arg in template {
        let mut value = arg.clone();
        for key in PLACEHOLDERS {
            let token = format!("{{{key}}}");
            if let Some(replacement) = vars.get(key) {
                value = value.replace(&token, replacement);
            }
        }
        ensure!(
            !value.contains('{') && !value.contains('}'),
            "unresolved placeholder in command argument {arg:?}"
        );
        expanded.push(value);
    }
    Ok(expanded)
}

fn validate_placeholders(arg: &str) -> Result<()> {
    let mut rest = arg;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let close = after
            .find('}')
            .context("unterminated placeholder in command")?;
        let name = &after[..close];
        ensure!(
            PLACEHOLDERS.contains(&name),
            "unknown command placeholder {{{name}}}"
        );
        rest = &after[close + 1..];
    }
    Ok(())
}

fn valid_label(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}
