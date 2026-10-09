use crate::{config::AuditInputs, storage};
use anyhow::{Context, Result, ensure};
use orchestrate_contracts::{
    CoverageState, Implementation, ReconciledDiscovery, Verdict, derive_verdict, investigation::*,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Native transport is the execution witness. Missing exit/completion facts never become passing receipts.
pub fn receipts(transport: &str) -> Vec<CommandReceipt> {
    let mut receipts = Vec::new();
    let mut tools: HashMap<String, (String, Option<String>)> = HashMap::new();
    for (line, raw) in transport.lines().enumerate() {
        let Ok(event) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        if event["type"] == "item.completed" && event["item"]["type"] == "command_execution" {
            let item = &event["item"];
            if let Some(command) = item["command"].as_str() {
                receipts.push(CommandReceipt {
                    id: format!("receipt-{}", line + 1),
                    transport_line: line + 1,
                    command: command.into(),
                    cwd: item["cwd"].as_str().map(str::to_owned),
                    exit_code: item["exit_code"]
                        .as_i64()
                        .and_then(|v| i32::try_from(v).ok()),
                    output: item["aggregated_output"].as_str().unwrap_or("").into(),
                    completed: matches!(item["status"].as_str(), Some("completed" | "failed")),
                });
            }
        }
        // Cursor's terminal shell event carries both native arguments and execution facts.
        if event["type"] == "tool_call" && event["subtype"] == "completed" {
            let shell = &event["tool_call"]["shellToolCall"];
            if let Some(command) = shell["args"]["command"].as_str() {
                let result = &shell["result"];
                let facts = result.get("success").or_else(|| result.get("failure"));
                let exit_code = facts
                    .and_then(|f| f["exitCode"].as_i64())
                    .and_then(|v| i32::try_from(v).ok());
                receipts.push(CommandReceipt {
                    id: format!("receipt-{}", line + 1),
                    transport_line: line + 1,
                    command: command.into(),
                    cwd: shell["args"]["workingDirectory"]
                        .as_str()
                        .map(str::to_owned),
                    exit_code,
                    output: facts
                        .map(|f| {
                            f["interleavedOutput"]
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| {
                                    format!(
                                        "{}{}",
                                        f["stdout"].as_str().unwrap_or(""),
                                        f["stderr"].as_str().unwrap_or("")
                                    )
                                })
                        })
                        .unwrap_or_default(),
                    completed: exit_code.is_some()
                        && result["isBackground"] != true
                        && facts.is_some_and(|f| f["aborted"] != true),
                });
            }
        }
        // Claude streams tool uses separately from their results.
        if let Some(content) = event["message"]["content"].as_array() {
            for block in content {
                if block["type"] == "tool_use"
                    && matches!(block["name"].as_str(), Some("Bash" | "Shell" | "shell"))
                    && let (Some(id), Some(command)) =
                        (block["id"].as_str(), block["input"]["command"].as_str())
                {
                    tools.insert(
                        id.into(),
                        (
                            command.into(),
                            block["input"]["cwd"].as_str().map(str::to_owned),
                        ),
                    );
                }
                if block["type"] == "tool_result" {
                    let Some((command, cwd)) = block["tool_use_id"]
                        .as_str()
                        .and_then(|id| tools.remove(id))
                    else {
                        continue;
                    };
                    let facts = &event["tool_use_result"];
                    let exit_code = facts["exit_code"]
                        .as_i64()
                        .or_else(|| facts["exitCode"].as_i64())
                        .and_then(|v| i32::try_from(v).ok());
                    let output = if facts["stdout"].is_string() || facts["stderr"].is_string() {
                        format!(
                            "{}{}",
                            facts["stdout"].as_str().unwrap_or(""),
                            facts["stderr"].as_str().unwrap_or("")
                        )
                    } else {
                        block["content"].as_str().unwrap_or("").into()
                    };
                    receipts.push(CommandReceipt {
                        id: format!("receipt-{}", line + 1),
                        transport_line: line + 1,
                        command,
                        cwd,
                        exit_code,
                        output,
                        completed: exit_code.is_some() && facts["interrupted"] != true,
                    });
                }
            }
        }
    }
    receipts
}

pub struct AuditContext<'a> {
    pub refs: &'a AuditInputs,
    pub reconciled: &'a ReconciledDiscovery,
    pub implementation: &'a Implementation,
}

pub fn validate_graph(
    graph: &mut LaneGraph,
    lane_id: &str,
    input_digest: &str,
    kind: &QuestionKind,
    sources: &BTreeMap<String, (std::path::PathBuf, BTreeMap<String, String>)>,
    receipts: &[CommandReceipt],
    audit: Option<AuditContext<'_>>,
) -> Result<Vec<String>> {
    ensure!(
        graph.schema_version == 1 && graph.lane_id == lane_id && graph.input_digest == input_digest,
        "graph version/lane/input identity mismatch"
    );
    let mut deps: HashMap<&str, &[String]> = HashMap::new();
    for (id, references) in graph
        .observations
        .iter()
        .map(|n| (n.id.as_str(), n.depends_on.as_slice()))
        .chain(
            graph
                .findings
                .iter()
                .map(|n| (n.id.as_str(), n.depends_on.as_slice())),
        )
        .chain(
            graph
                .challenges
                .iter()
                .map(|n| (n.id.as_str(), n.depends_on.as_slice())),
        )
        .chain(std::iter::once((
            graph.conclusion.id.as_str(),
            graph.conclusion.depends_on.as_slice(),
        )))
    {
        ensure!(
            !id.trim().is_empty()
                && !id.contains(['/', ':'])
                && deps.insert(id, references).is_none(),
            "missing or duplicate graph ID {id}"
        );
    }
    for refs in deps.values() {
        let mut seen = HashSet::new();
        for reference in *refs {
            ensure!(
                deps.contains_key(reference.as_str()) && seen.insert(reference),
                "missing or duplicate dependency {reference}"
            );
        }
    }
    for observation in &graph.observations {
        ensure!(
            observation
                .depends_on
                .iter()
                .all(|id| graph.observations.iter().any(|o| o.id == *id)),
            "observations cannot depend on derived findings or conclusions"
        );
    }
    for node in graph
        .findings
        .iter()
        .map(|f| &f.depends_on)
        .chain(graph.challenges.iter().map(|c| &c.depends_on))
    {
        ensure!(
            !node.contains(&graph.conclusion.id),
            "findings/challenges cannot use the final conclusion as evidence"
        );
    }
    let mut done = HashSet::new();
    for id in deps.keys() {
        visit(id, &deps, &mut HashSet::new(), &mut done)?;
    }
    let finding_ids: HashSet<_> = graph.findings.iter().map(|f| f.id.as_str()).collect();
    for challenge in &graph.challenges {
        ensure!(
            !challenge.reason.trim().is_empty() && !challenge.targets.is_empty(),
            "challenge requires reason and targets"
        );
        let mut targets = HashSet::new();
        for target in &challenge.targets {
            ensure!(targets.insert(target), "duplicate challenge target");
            ensure!(
                finding_ids.contains(target.as_str()) || *target == graph.conclusion.id,
                "challenge target must be a finding or conclusion"
            );
        }
        ensure!(
            has_observation(&challenge.id, &deps, graph),
            "challenge needs an observation path"
        );
    }
    ensure!(
        !graph.conclusion.answer.trim().is_empty(),
        "conclusion needs an answer"
    );
    ensure!(
        !graph.conclusion.depends_on.is_empty()
            && graph
                .conclusion
                .depends_on
                .iter()
                .all(|r| finding_ids.contains(r.as_str())
                    || graph.challenges.iter().any(|c| c.id == *r)),
        "conclusion must reference findings/challenges"
    );
    match kind {
        QuestionKind::Binary => ensure!(
            graph.conclusion.position.is_some(),
            "binary request requires GO/NO_GO/UNKNOWN"
        ),
        QuestionKind::OpenEnded => ensure!(
            graph.conclusion.position.is_none(),
            "open-ended request must not invent a vote"
        ),
    }
    let mut limitations = Vec::new();
    // Drop immutable borrows before resolving execution references in the normalized graph.
    drop(deps);
    for observation in &mut graph.observations {
        ensure!(
            !observation.observed.trim().is_empty() && !observation.environment.trim().is_empty(),
            "observation needs bounded facts and environment"
        );
        if let Some(location) = &observation.source {
            let (root, files) = sources
                .get(&location.source_id)
                .context("unknown source identity")?;
            let expected = files
                .get(&location.path)
                .context("observation references nonexistent frozen evidence")?;
            let bytes = storage::read(root, &location.path)?;
            ensure!(
                orchestrate_contracts::digest_bytes(&bytes) == *expected,
                "observation source digest mismatch"
            );
            if let Some(line) = location.line {
                ensure!(
                    line > 0 && line <= String::from_utf8_lossy(&bytes).lines().count(),
                    "nonexistent source line"
                );
            }
        }
        match observation.kind {
            ObservationKind::Inspection => {
                ensure!(
                    observation.source.is_some()
                        && observation.command.is_none()
                        && observation.receipt.is_none(),
                    "static inspection needs a source and cannot claim executed verification"
                );
            }
            ObservationKind::Execution => {
                let command = observation
                    .command
                    .as_deref()
                    .context("execution observation needs the actual command")?;
                let matches: Vec<_> = receipts
                    .iter()
                    .filter(|r| {
                        same_command(&r.command, command)
                            && observation.receipt.as_ref().is_none_or(|id| r.id == *id)
                    })
                    .collect();
                if observation.receipt.is_some() {
                    ensure!(
                        matches.len() == 1,
                        "forged or ambiguous execution receipt reference"
                    );
                }
                if matches.len() == 1 && matches[0].completed && matches[0].exit_code.is_some() {
                    observation.receipt = Some(matches[0].id.clone());
                } else {
                    observation.kind = ObservationKind::Testimony;
                    observation.receipt = None;
                    limitations.push(format!(
                        "{}: execution not established by a complete native transport receipt",
                        observation.id
                    ));
                }
            }
            ObservationKind::Testimony => ensure!(
                observation.receipt.is_none(),
                "testimony cannot claim an execution receipt"
            ),
        }
    }
    let deps = dependencies(graph);
    for finding in &graph.findings {
        ensure!(
            !finding.claim.trim().is_empty()
                && !finding.scope.trim().is_empty()
                && !finding.applicability.trim().is_empty()
                && !finding.impact.trim().is_empty(),
            "finding requires a scoped claim, applicability and impact"
        );
        ensure!(
            has_observation(&finding.id, &deps, graph),
            "finding lacks an inspectable evidence path"
        );
    }
    if graph.conclusion.position == Some(Position::Go) {
        ensure!(
            graph
                .conclusion
                .depends_on
                .iter()
                .any(|id| graph.findings.iter().any(|f| f.id == *id && !f.negative)),
            "GO conclusion requires supporting findings"
        );
        for id in &graph.conclusion.depends_on {
            if graph.findings.iter().any(|f| f.id == *id) {
                ensure!(
                    has_primary(id, &deps, graph),
                    "GO relies on unverified testimony"
                );
            }
        }
    }
    if let Some(audit) = audit {
        let assessment = graph
            .assessment
            .as_ref()
            .context("conformance graph lacks assessment")?;
        ensure!(
            assessment.reconciled == audit.refs.reconciled
                && assessment.adoption == audit.refs.adoption
                && assessment.implementation == audit.refs.implementation,
            "wrong conformance authority references"
        );
        ensure!(
            assessment.coverage.len() == audit.reconciled.requirements.len(),
            "incomplete binding requirement coverage"
        );
        for row in &assessment.coverage {
            for id in &row.evidence {
                ensure!(
                    deps.contains_key(id.as_str()) && has_observation(id, &deps, graph),
                    "coverage references missing graph evidence"
                );
            }
            if matches!(
                row.state,
                CoverageState::Pass | CoverageState::Fail | CoverageState::NotApplicable
            ) {
                ensure!(
                    row.evidence.iter().any(|id| has_primary(id, &deps, graph)),
                    "conformance disposition relies solely on unverified testimony"
                );
            }
        }
        let position =
            match derive_verdict(audit.reconciled, assessment, &audit.implementation.status)? {
                Verdict::Pass => Position::Go,
                Verdict::ChangesRequired => Position::NoGo,
                Verdict::Blocked => Position::Unknown,
            };
        ensure!(
            graph.conclusion.position == Some(position),
            "lane position contradicts coverage-derived Audit verdict"
        );
    } else {
        ensure!(
            graph.assessment.is_none(),
            "generic investigation cannot supply conformance assessment"
        );
    }
    Ok(limitations)
}
pub fn dependencies(graph: &LaneGraph) -> HashMap<&str, &[String]> {
    graph
        .observations
        .iter()
        .map(|n| (n.id.as_str(), n.depends_on.as_slice()))
        .chain(
            graph
                .findings
                .iter()
                .map(|n| (n.id.as_str(), n.depends_on.as_slice())),
        )
        .chain(
            graph
                .challenges
                .iter()
                .map(|n| (n.id.as_str(), n.depends_on.as_slice())),
        )
        .chain(std::iter::once((
            graph.conclusion.id.as_str(),
            graph.conclusion.depends_on.as_slice(),
        )))
        .collect()
}
fn visit<'a>(
    id: &'a str,
    map: &HashMap<&'a str, &'a [String]>,
    visiting: &mut HashSet<&'a str>,
    done: &mut HashSet<&'a str>,
) -> Result<()> {
    if done.contains(id) {
        return Ok(());
    }
    ensure!(visiting.insert(id), "cyclic evidence dependency at {id}");
    for reference in map[id] {
        visit(reference, map, visiting, done)?;
    }
    visiting.remove(id);
    done.insert(id);
    Ok(())
}
fn reaches(id: &str, map: &HashMap<&str, &[String]>, predicate: &impl Fn(&str) -> bool) -> bool {
    predicate(id)
        || map
            .get(id)
            .is_some_and(|refs| refs.iter().any(|r| reaches(r, map, predicate)))
}
fn has_observation(id: &str, map: &HashMap<&str, &[String]>, graph: &LaneGraph) -> bool {
    reaches(id, map, &|id| graph.observations.iter().any(|o| o.id == id))
}
pub fn has_primary(id: &str, map: &HashMap<&str, &[String]>, graph: &LaneGraph) -> bool {
    reaches(id, map, &|id| {
        graph.observations.iter().any(|o| {
            o.id == id
                && matches!(
                    o.kind,
                    ObservationKind::Inspection | ObservationKind::Execution
                )
        })
    })
}
pub fn primary_observation(graph: &LaneGraph, id: &str) -> bool {
    graph.observations.iter().any(|o| {
        o.id == id
            && matches!(
                o.kind,
                ObservationKind::Inspection | ObservationKind::Execution
            )
    })
}

/// Reconciliation has its entire bounded evidence in the prompt and must not initiate tools.
pub fn has_tool_activity(transport: &str) -> bool {
    transport
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .any(|event| {
            matches!(
                event["item"]["type"].as_str(),
                Some(
                    "command_execution"
                        | "web_search"
                        | "mcp_tool_call"
                        | "file_change"
                        | "tool_call"
                )
            ) || event["type"] == "tool_call"
                || event["message"]["content"]
                    .as_array()
                    .is_some_and(|blocks| {
                        blocks.iter().any(|b| {
                            matches!(b["type"].as_str(), Some("tool_use" | "server_tool_use"))
                        })
                    })
        })
}

fn same_command(captured: &str, claimed: &str) -> bool {
    if captured == claimed {
        return true;
    }
    // Native Codex receipts render the tool command inside the provider's shell wrapper.
    // Unwrap only an exact shell -c/-lc invocation, never rewrite the command itself.
    shlex::split(captured).is_some_and(|parts| {
        parts.len() == 3
            && matches!(
                std::path::Path::new(&parts[0])
                    .file_name()
                    .and_then(|n| n.to_str()),
                Some("zsh" | "bash" | "sh")
            )
            && matches!(parts[1].as_str(), "-c" | "-lc")
            && parts[2] == claimed
    })
}

pub fn provider_metadata(outcome: &orchestrate_core::provider::InvocationOutcome) -> Value {
    let mut models = std::collections::BTreeSet::new();
    let mut versions = std::collections::BTreeSet::new();
    for event in outcome
        .stdout
        .lines()
        .filter_map(|s| serde_json::from_str::<Value>(s).ok())
    {
        if matches!(
            event["type"].as_str(),
            Some("system" | "thread.started" | "turn.started" | "result")
        ) {
            if let Some(model) = event["model"].as_str() {
                models.insert(model.to_owned());
            }
            for field in ["modelUsage", "model_usage"] {
                if let Some(usage) = event[field].as_object() {
                    models.extend(usage.keys().cloned());
                }
            }
            if let Some(version) = event["cli_version"].as_str() {
                versions.insert(version.to_owned());
            }
        }
    }
    serde_json::json!({"observed_session":outcome.observed_session,"observed_models":models,"observed_cli_versions":versions,"unknown_values_are_not_inferred":true})
}
