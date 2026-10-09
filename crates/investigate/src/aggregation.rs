use crate::{config::Config, evidence};
use anyhow::{Context, Result, ensure};
use orchestrate_contracts::investigation::*;
use std::collections::{HashMap, HashSet};

pub fn validate(
    reconciliation: &Reconciliation,
    input_digest: &str,
    graphs: &[LaneGraph],
) -> Result<()> {
    ensure!(
        reconciliation.schema_version == 1
            && reconciliation.input_digest == input_digest
            && !reconciliation.answer.trim().is_empty(),
        "reconciliation version/input/answer invalid"
    );
    let findings: HashMap<_, _> = graphs
        .iter()
        .flat_map(|g| {
            g.findings
                .iter()
                .map(move |f| (format!("{}/{}", g.lane_id, f.id), (g, f)))
        })
        .collect();
    let observations: HashMap<_, _> = graphs
        .iter()
        .flat_map(|g| {
            g.observations
                .iter()
                .map(move |o| (format!("{}/{}", g.lane_id, o.id), (g, o)))
        })
        .collect();
    let challenges: HashMap<_, _> = graphs
        .iter()
        .flat_map(|g| {
            g.challenges
                .iter()
                .map(move |c| (format!("{}/{}", g.lane_id, c.id), c))
        })
        .collect();
    let mut ids = HashSet::new();
    let mut origins = HashSet::new();
    for canonical in &reconciliation.findings {
        ensure!(
            !canonical.id.trim().is_empty() && ids.insert(canonical.id.as_str()),
            "missing or duplicate canonical finding ID"
        );
        ensure!(
            !canonical.proposition.trim().is_empty()
                && !canonical.scope.trim().is_empty()
                && !canonical.rationale.trim().is_empty()
                && !canonical.origins.is_empty(),
            "canonical finding requires scoped proposition, origins and rationale"
        );
        let mut material = false;
        let mut origin_observations = HashSet::new();
        for origin in &canonical.origins {
            let (graph, finding) = findings
                .get(origin)
                .context("unattributed canonical finding")?;
            ensure!(
                origins.insert(origin),
                "finding origin assigned to multiple canonical claims"
            );
            ensure!(
                finding.negative == canonical.negative,
                "contradictory propositions must retain distinct canonical findings"
            );
            material |= finding.material;
            let dependencies = evidence::dependencies(graph);
            for observation in &graph.observations {
                if reachable(&finding.id, &observation.id, &dependencies) {
                    origin_observations.insert(format!("{}/{}", graph.lane_id, observation.id));
                }
            }
        }
        ensure!(
            !material || canonical.material,
            "reconciliation cannot silently erase originating materiality"
        );
        let mut seen = HashSet::new();
        for reference in canonical
            .supporting_observations
            .iter()
            .chain(&canonical.contradicting_observations)
        {
            ensure!(
                observations.contains_key(reference) && seen.insert(reference),
                "missing or duplicate reconciliation observation {reference}"
            );
        }
        let primary = |reference: &String| {
            observations
                .get(reference)
                .is_some_and(|(graph, observation)| {
                    evidence::primary_observation(graph, &observation.id)
                })
        };
        if canonical.disposition == Disposition::Supported {
            ensure!(
                canonical
                    .supporting_observations
                    .iter()
                    .any(|r| origin_observations.contains(r) && primary(r)),
                "supported claim lacks primary evidence from its originating finding"
            );
        }
        if canonical.disposition == Disposition::Rejected {
            ensure!(
                canonical
                    .supporting_observations
                    .iter()
                    .chain(&canonical.contradicting_observations)
                    .any(primary),
                "rejected finding requires a primary rejection basis"
            );
        }
        if canonical.demonstrated {
            ensure!(
                canonical.disposition == Disposition::Supported
                    && canonical
                        .supporting_observations
                        .iter()
                        .any(|r| origin_observations.contains(r) && primary(r)),
                "demonstration lacks a supported primary evidence path"
            );
        }
    }
    ensure!(
        origins.len() == findings.len(),
        "reconciliation omitted original findings"
    );
    let mut handled = HashSet::new();
    for disposition in &reconciliation.challenges {
        let challenge = challenges
            .get(&disposition.challenge)
            .context("unknown reconciled challenge")?;
        ensure!(
            handled.insert(&disposition.challenge) && !disposition.rationale.trim().is_empty(),
            "duplicate challenge or missing rationale"
        );
        for reference in &disposition.observations {
            ensure!(
                observations.contains_key(reference),
                "challenge disposition cites missing evidence"
            );
        }
        if challenge.material
            && matches!(
                disposition.disposition,
                Disposition::Supported | Disposition::Rejected
            )
        {
            ensure!(
                disposition.observations.iter().any(|r| observations
                    .get(r)
                    .is_some_and(|(g, o)| evidence::primary_observation(g, &o.id))),
                "material challenge disposition lacks primary evidence"
            );
        }
    }
    ensure!(
        handled.len() == challenges.len(),
        "reconciliation omitted challenges"
    );
    let mut essential = HashSet::new();
    for reference in &reconciliation.essential_findings {
        ensure!(
            ids.contains(reference.as_str()) && essential.insert(reference),
            "missing or duplicate essential finding"
        );
    }
    Ok(())
}
fn reachable(from: &str, target: &str, deps: &HashMap<&str, &[String]>) -> bool {
    from == target
        || deps
            .get(from)
            .is_some_and(|refs| refs.iter().any(|r| reachable(r, target, deps)))
}

pub fn result(
    config: &Config,
    run_id: &str,
    input_digest: &str,
    lanes: Vec<LaneRecord>,
    reconciliation: Option<Reconciliation>,
    mut limitations: Vec<String>,
    graphs: &[LaneGraph],
) -> InvestigationResult {
    let completed = lanes
        .iter()
        .filter(|l| l.state == LaneState::Completed)
        .count();
    let invalid = lanes
        .iter()
        .filter(|l| l.state == LaneState::Invalid)
        .count();
    let unavailable = lanes
        .iter()
        .filter(|l| l.state == LaneState::Unavailable)
        .count();
    let failed = lanes
        .iter()
        .filter(|l| l.state == LaneState::Failed)
        .count();
    let completion = if completed >= config.min_completed() && reconciliation.is_some() {
        Completion::Complete
    } else {
        Completion::Incomplete
    };
    let votes = if config.mode == Mode::Consensus {
        let min_go_votes = config
            .consensus
            .as_ref()
            .expect("validated consensus config")
            .min_go_votes;
        let go = lanes
            .iter()
            .filter(|l| l.state == LaneState::Completed && l.position == Some(Position::Go))
            .count();
        let no_go = lanes
            .iter()
            .filter(|l| l.state == LaneState::Completed && l.position == Some(Position::NoGo))
            .count();
        let unknown = completed - go - no_go;
        Some(VoteResult {
            go,
            no_go,
            unknown,
            min_go_votes,
            threshold_met: go >= min_go_votes,
        })
    } else {
        None
    };
    let recommendation = if completion == Completion::Incomplete {
        Some(Recommendation::Inconclusive)
    } else if config.mode == Mode::Wide {
        None
    } else {
        let reconciliation = reconciliation.as_ref().expect("complete reconciliation");
        let demonstrated_defect = reconciliation.findings.iter().any(|f| {
            f.negative && f.material && f.demonstrated && f.disposition == Disposition::Supported
        });
        let material_challenges: HashSet<_> = graphs
            .iter()
            .flat_map(|g| {
                g.challenges
                    .iter()
                    .filter(|c| c.material)
                    .map(move |c| format!("{}/{}", g.lane_id, c.id))
            })
            .collect();
        let unresolved = reconciliation.findings.iter().any(|f| {
            f.material
                && ((f.negative && f.disposition != Disposition::Rejected)
                    || matches!(
                        f.disposition,
                        Disposition::Contested | Disposition::InsufficientEvidence
                    ))
        }) || reconciliation.challenges.iter().any(|c| {
            material_challenges.contains(&c.challenge) && c.disposition != Disposition::Rejected
        });
        let supported_answer = !reconciliation.essential_findings.is_empty()
            && reconciliation.essential_findings.iter().all(|id| {
                reconciliation
                    .findings
                    .iter()
                    .any(|f| f.id == *id && !f.negative && f.disposition == Disposition::Supported)
            });
        Some(if demonstrated_defect {
            if config.audit.is_some() {
                Recommendation::ChangesRequired
            } else {
                Recommendation::NoGo
            }
        } else if unresolved
            || !supported_answer
            || !votes.as_ref().expect("consensus votes").threshold_met
        {
            Recommendation::ReviewRequired
        } else {
            Recommendation::Go
        })
    };
    if completed < config.cohort_size() {
        limitations.push(format!("Only {completed}/{} configured lanes supplied eligible graphs; missing lanes are not affirmative votes or clean reviews.", config.cohort_size()));
    }
    limitations.push("Graph validation establishes integrity and completeness, not semantic truth or statistical independence. Reconciliation classifications are agent judgments over retained evidence.".into());
    InvestigationResult {
        schema_version: 1,
        run_id: run_id.into(),
        input_digest: input_digest.into(),
        mode: config.mode.clone(),
        completion,
        requested: config.cohort_size(),
        completed,
        invalid,
        unavailable,
        failed,
        min_completed: config.min_completed(),
        lanes,
        votes,
        recommendation,
        reconciliation,
        limitations,
    }
}

pub fn render(result: &InvestigationResult) -> String {
    let mut report = format!(
        "# Investigation {}\n\nMode: {:?}. Completion: {}.\n\nLanes: {} requested, {} completed, {} invalid, {} unavailable, {} failed; minimum completed: {}.\n\nInput digest: `{}`.\n",
        result.run_id,
        result.mode,
        serde_json::to_value(&result.completion)
            .expect("enum")
            .as_str()
            .expect("string"),
        result.requested,
        result.completed,
        result.invalid,
        result.unavailable,
        result.failed,
        result.min_completed,
        result.input_digest
    );
    if let Some(vote) = &result.votes {
        report.push_str(&format!(
            "\nGO: {}; NO_GO: {}; UNKNOWN: {}; threshold: {}/{}; threshold_met: {}.\n",
            vote.go,
            vote.no_go,
            vote.unknown,
            vote.min_go_votes,
            result.requested,
            vote.threshold_met
        ));
    }
    if let Some(recommendation) = &result.recommendation {
        report.push_str(&format!(
            "\nEvidence-based recommendation: {}.\n",
            serde_json::to_value(recommendation)
                .expect("enum")
                .as_str()
                .expect("string")
        ));
    }
    if let Some(reconciliation) = &result.reconciliation {
        report.push_str("\n## Findings\n");
        for finding in &reconciliation.findings {
            report.push_str(&format!("\n### {}: {:?}\n\n{}\n\nScope: {}. Material: {}. Demonstrated: {}.\n\nOrigins: {}.\n\nSupporting observations: {}. Contradicting observations: {}.\n\n{}\n",finding.id,finding.disposition,finding.proposition,finding.scope,finding.material,finding.demonstrated,finding.origins.join(", "),finding.supporting_observations.join(", "),finding.contradicting_observations.join(", "),finding.rationale));
            for limit in &finding.limitations {
                report.push_str(&format!("\n- {limit}\n"));
            }
        }
        report.push_str("\n## Challenge dispositions\n");
        for challenge in &reconciliation.challenges {
            report.push_str(&format!(
                "\n- {}: {:?}; {} Evidence: {}.\n",
                challenge.challenge,
                challenge.disposition,
                challenge.rationale,
                challenge.observations.join(", ")
            ));
        }
        report.push_str(&format!(
            "\nAttributed answer:\n\n> {}\n",
            reconciliation.answer.replace('\n', "\n> ")
        ));
        report.push_str("\n## Reconciler assessment\n\nThe retained reconciliation.json contains the agent's attributed answer and essential claims. The recommendation and vote totals above are calculated by Rust.\n");
        for limit in &reconciliation.limitations {
            report.push_str(&format!("\n- {limit}\n"));
        }
    }
    report.push_str("\n## Lanes\n");
    for lane in &result.lanes {
        report.push_str(&format!(
            "\n- {}: {:?}; {} Manifest: `{}`.\n",
            lane.lane_id, lane.state, lane.detail, lane.manifest_digest
        ));
    }
    report.push_str("\n## Limitations\n");
    for limit in &result.limitations {
        report.push_str(&format!("\n- {limit}\n"));
    }
    report
}
