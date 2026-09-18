use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use orchestrate_contracts::{
    ArtifactKind, ArtifactRef, AuditAssessment, ConsensusProposal, Invalidation, Opinion,
    Provenance,
};
use orchestrate_core::Store;

const DISCOVERY_GUIDE: &str = include_str!("../resources/guides/discovery.md");
const CONSENSUS_GUIDE: &str = include_str!("../resources/guides/consensus.md");
const AUDIT_GUIDE: &str = include_str!("../resources/guides/audit.md");

#[derive(Parser)]
#[command(
    name = "orchestrate",
    version,
    about = "Inspectable Discovery → Consensus → external Build → Audit"
)]
struct Cli {
    /// External orchestration root. Defaults to ~/.orchestration.
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Init(Init),
    Discovery {
        #[command(subcommand)]
        command: Discovery,
    },
    Consensus {
        #[command(subcommand)]
        command: Consensus,
    },
    Agreement {
        #[command(subcommand)]
        command: AgreementCommand,
    },
    Implementation {
        #[command(subcommand)]
        command: ImplementationCommand,
    },
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    Invalidate(Invalidate),
    Status(SelectEffort),
    Inspect(Inspect),
    Lineage(Inspect),
    Guide {
        phase: Option<String>,
    },
    Skills {
        #[command(subcommand)]
        command: Skills,
    },
}
#[derive(Args)]
struct Init {
    #[arg(long)]
    project: PathBuf,
    #[arg(long)]
    effort: String,
    #[arg(long)]
    request: String,
    #[arg(long = "constraint")]
    constraints: Vec<String>,
}
#[derive(Args)]
struct SelectEffort {
    #[arg(long)]
    effort: String,
}
#[derive(Args)]
struct Inspect {
    #[arg(long)]
    effort: String,
    #[arg(long)]
    artifact: String,
}
#[derive(Args)]
struct Invalidate {
    #[arg(long)]
    effort: String,
    #[arg(long)]
    artifact: String,
    #[arg(long)]
    reason: String,
    #[arg(long)]
    declared_by: String,
    #[command(flatten)]
    provenance: ProvenanceArgs,
}
#[derive(Subcommand)]
enum Discovery {
    Prepare {
        #[arg(long)]
        effort: String,
        #[arg(long)]
        slot: String,
    },
    Run {
        #[arg(long)]
        effort: String,
        #[arg(long)]
        slot: String,
        #[arg(long)]
        provider: PathBuf,
        #[command(flatten)]
        provenance: ProvenanceArgs,
    },
    Finalize {
        #[arg(long)]
        effort: String,
        #[arg(long)]
        opinion: PathBuf,
        #[command(flatten)]
        provenance: ProvenanceArgs,
    },
}
#[derive(Subcommand)]
enum Consensus {
    Run {
        #[arg(long)]
        effort: String,
        #[arg(long = "opinion")]
        opinions: Vec<String>,
        #[arg(long)]
        provider: PathBuf,
        #[command(flatten)]
        provenance: ProvenanceArgs,
    },
    Finalize {
        #[arg(long)]
        effort: String,
        #[arg(long = "opinion")]
        opinions: Vec<String>,
        #[arg(long)]
        proposal: PathBuf,
        #[command(flatten)]
        provenance: ProvenanceArgs,
    },
}
#[derive(Subcommand)]
enum AgreementCommand {
    Adopt {
        #[arg(long)]
        effort: String,
        #[arg(long)]
        agreement: String,
        #[arg(long)]
        authorized_by: String,
        #[arg(long, default_value = "external implementation registration")]
        scope: String,
        #[command(flatten)]
        provenance: ProvenanceArgs,
    },
}
#[derive(Subcommand)]
enum ImplementationCommand {
    Register {
        #[arg(long)]
        effort: String,
        #[arg(long)]
        adoption: String,
        #[arg(long)]
        project: PathBuf,
        #[arg(long, default_value = "HEAD")]
        commit: String,
        #[arg(long)]
        declaration: String,
        #[arg(long, default_value = "submitted")]
        status: String,
        #[command(flatten)]
        provenance: ProvenanceArgs,
    },
}
#[derive(Subcommand)]
enum AuditCommand {
    Run {
        #[arg(long)]
        effort: String,
        #[arg(long)]
        agreement: String,
        #[arg(long)]
        adoption: String,
        #[arg(long)]
        implementation: String,
        #[arg(long)]
        provider: PathBuf,
        #[command(flatten)]
        provenance: ProvenanceArgs,
    },
    Finalize {
        #[arg(long)]
        effort: String,
        #[arg(long)]
        assessment: PathBuf,
        #[command(flatten)]
        provenance: ProvenanceArgs,
    },
}
#[derive(Subcommand)]
enum Skills {
    Install {
        #[arg(long)]
        prefix: PathBuf,
    },
}
#[derive(Args, Clone)]
struct ProvenanceArgs {
    #[arg(long, default_value = "operator")]
    host: String,
    #[arg(long = "execution-provider")]
    execution_provider: Option<String>,
    #[arg(long)]
    requested_model: Option<String>,
    #[arg(long)]
    observed_model: Option<String>,
    #[arg(long)]
    requested_effort: Option<String>,
    #[arg(long)]
    observed_effort: Option<String>,
    #[arg(long, default_value = "input_excluded_cooperative")]
    #[arg(value_parser = parse_independence)]
    independence: orchestrate_contracts::Independence,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("orchestrate: {error:#}");
        std::process::exit(2);
    }
}
fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Guide { phase } => guide(phase),
        Command::Skills { command } => skills(command),
        command => execute(store(cli.root)?, command),
    }
}
fn execute(store: Store, command: Command) -> Result<()> {
    match command {
        Command::Init(args) => {
            let effort =
                store.init_effort(&args.project, &args.effort, args.request, args.constraints)?;
            output(
                "SUCCESS",
                "COHORT_CREATED",
                serde_json::json!({"effort": effort.id, "cohort": effort.cohort.id, "baseline": effort.cohort.baseline_commit}),
            );
        }
        Command::Discovery {
            command: Discovery::Prepare { effort, slot },
        } => {
            let effort = store.load_effort(&effort)?;
            let (run_id, source) = orchestrate_discovery::prepare(&store, &effort, &slot)?;
            output(
                "SUCCESS",
                "PREPARED",
                serde_json::json!({"run": run_id, "frozen_source": source}),
            );
        }
        Command::Discovery {
            command:
                Discovery::Run {
                    effort,
                    slot,
                    provider,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let reference = orchestrate_discovery::run_provider(
                &store,
                &effort,
                &slot,
                &provider,
                provenance.into(),
            )?;
            output(
                "SUCCESS",
                "FINALIZED",
                serde_json::json!({"artifact": reference}),
            );
        }
        Command::Discovery {
            command:
                Discovery::Finalize {
                    effort,
                    opinion,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let opinion: Opinion = json_file(&opinion)?;
            let reference =
                orchestrate_discovery::finalize(&store, &effort, opinion, provenance.into())?;
            output(
                "SUCCESS",
                "FINALIZED",
                serde_json::json!({"artifact": reference}),
            );
        }
        Command::Consensus {
            command:
                Consensus::Run {
                    effort,
                    opinions,
                    provider,
                    provenance,
                },
        } => {
            ensure!(
                opinions.len() == 3,
                "exactly three --opinion selectors are required"
            );
            let effort = store.load_effort(&effort)?;
            let refs: Vec<_> = opinions
                .iter()
                .map(|id| store.find_artifact_ref(&effort, id))
                .collect::<Result<_>>()?;
            let refs: [ArtifactRef; 3] = refs
                .try_into()
                .map_err(|_| anyhow::anyhow!("three opinions required"))?;
            let result = orchestrate_consensus::run_provider(
                &store,
                &effort,
                refs,
                &provider,
                provenance.into(),
            )?;
            output(
                "SUCCESS",
                if result.agreement.is_some() {
                    "ELIGIBLE_CANDIDATE"
                } else {
                    "NO_CONSENSUS"
                },
                serde_json::json!({"comparison": result.comparison, "agreement": result.agreement}),
            );
        }
        Command::Consensus {
            command:
                Consensus::Finalize {
                    effort,
                    opinions,
                    proposal,
                    provenance,
                },
        } => {
            ensure!(
                opinions.len() == 3,
                "exactly three --opinion selectors are required"
            );
            let effort = store.load_effort(&effort)?;
            let refs: Vec<_> = opinions
                .iter()
                .map(|id| store.find_artifact_ref(&effort, id))
                .collect::<Result<_>>()?;
            let refs: [ArtifactRef; 3] = refs
                .try_into()
                .map_err(|_| anyhow::anyhow!("three opinions required"))?;
            let proposal: ConsensusProposal = json_file(&proposal)?;
            let result = orchestrate_consensus::finalize(
                &store,
                &effort,
                refs,
                proposal,
                provenance.into(),
            )?;
            output(
                "SUCCESS",
                if result.agreement.is_some() {
                    "ELIGIBLE_CANDIDATE"
                } else {
                    "NO_CONSENSUS"
                },
                serde_json::json!({"comparison": result.comparison, "agreement": result.agreement}),
            );
        }
        Command::Agreement {
            command:
                AgreementCommand::Adopt {
                    effort,
                    agreement,
                    authorized_by,
                    scope,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let reference = orchestrate_audit::adopt(
                &store,
                &effort,
                store.find_artifact_ref(&effort, &agreement)?,
                authorized_by,
                scope,
                provenance.into(),
            )?;
            output(
                "SUCCESS",
                "ADOPTED",
                serde_json::json!({"adoption": reference}),
            );
        }
        Command::Implementation {
            command:
                ImplementationCommand::Register {
                    effort,
                    adoption,
                    project,
                    commit,
                    declaration,
                    status,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let reference = orchestrate_audit::register_implementation(
                &store,
                &effort,
                store.find_artifact_ref(&effort, &adoption)?,
                &project,
                &commit,
                declaration,
                status,
                provenance.into(),
            )?;
            output(
                "SUCCESS",
                "REGISTERED_EXTERNAL",
                serde_json::json!({"implementation": reference, "build": "DEFERRED_EXTERNAL_ONLY"}),
            );
        }
        Command::Audit {
            command:
                AuditCommand::Run {
                    effort,
                    agreement,
                    adoption,
                    implementation,
                    provider,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let reference = orchestrate_audit::run_provider(
                &store,
                &effort,
                store.find_artifact_ref(&effort, &agreement)?,
                store.find_artifact_ref(&effort, &adoption)?,
                store.find_artifact_ref(&effort, &implementation)?,
                &provider,
                provenance.into(),
            )?;
            output(
                "SUCCESS",
                "ASSESSMENT_FINALIZED",
                serde_json::json!({"audit": reference}),
            );
        }
        Command::Invalidate(args) => {
            let effort = store.load_effort(&args.effort)?;
            let target = store.find_artifact_ref(&effort, &args.artifact)?;
            let notice = Invalidation {
                target: target.clone(),
                reason: args.reason,
                declared_by: args.declared_by,
            };
            let reference = store.publish(
                &effort,
                "invalidation",
                ArtifactKind::Invalidation,
                format!("invalidation-{}", timestamp_suffix()),
                "INVALIDATED".to_owned(),
                vec![target],
                args.provenance.into(),
                &notice,
            )?;
            output(
                "SUCCESS",
                "INVALIDATED",
                serde_json::json!({"notice": reference}),
            );
        }
        Command::Audit {
            command:
                AuditCommand::Finalize {
                    effort,
                    assessment,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let assessment: AuditAssessment = json_file(&assessment)?;
            let reference =
                orchestrate_audit::finalize_audit(&store, &effort, assessment, provenance.into())?;
            output(
                "SUCCESS",
                "ASSESSMENT_FINALIZED",
                serde_json::json!({"audit": reference}),
            );
        }
        Command::Status(args) => {
            let effort = store.load_effort(&args.effort)?;
            output(
                "SUCCESS",
                "READ_ONLY",
                serde_json::json!({"effort": effort, "artifacts": store.list_artifacts(&effort)?}),
            );
        }
        Command::Inspect(args) => {
            let effort = store.load_effort(&args.effort)?;
            let reference = store.find_artifact_ref(&effort, &args.artifact)?;
            output(
                "SUCCESS",
                "READ_ONLY",
                serde_json::to_value(store.load_envelope(&effort, &reference)?)?,
            );
        }
        Command::Lineage(args) => {
            let effort = store.load_effort(&args.effort)?;
            let reference = store.find_artifact_ref(&effort, &args.artifact)?;
            output(
                "SUCCESS",
                "READ_ONLY",
                serde_json::json!({"artifact": reference, "nodes": store.reconstruct_lineage(&effort, &reference)?}),
            );
        }
        Command::Guide { .. } | Command::Skills { .. } => {
            unreachable!("handled before store setup")
        }
    }
    Ok(())
}
fn store(root: Option<PathBuf>) -> Result<Store> {
    Store::open(root.unwrap_or_else(default_root))
}
fn default_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".orchestration")
}
fn json_file<T: for<'a> serde::Deserialize<'a>>(path: &Path) -> Result<T> {
    orchestrate_contracts::decode(
        &fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
    )
}
fn output(operation_status: &str, semantic_outcome: &str, details: serde_json::Value) {
    println!(
        "{}",
        serde_json::json!({"operation_status": operation_status, "semantic_outcome": semantic_outcome, "details": details})
    );
}
fn guide(phase: Option<String>) -> Result<()> {
    match phase.as_deref() {
        None => println!("available guides: discovery, consensus, audit"),
        Some("discovery") => print!("{DISCOVERY_GUIDE}"),
        Some("consensus") => print!("{CONSENSUS_GUIDE}"),
        Some("audit") => print!("{AUDIT_GUIDE}"),
        Some(other) => bail!("unknown guide phase {other}; Build execution is deferred"),
    };
    Ok(())
}
fn skills(command: Skills) -> Result<()> {
    match command {
        Skills::Install { prefix } => {
            ensure!(prefix.is_absolute(), "skill prefix must be absolute");
            for (name, phase) in [
                ("orchestrate-discovery", "discovery"),
                ("orchestrate-consensus", "consensus"),
                ("orchestrate-audit", "audit"),
            ] {
                let dir = prefix.join(name);
                fs::create_dir_all(&dir)?;
                let marker = dir.join(".orchestrate-owned");
                let skill = dir.join("SKILL.md");
                let content = format!(
                    "---\nname: {name}\ndisable-model-invocation: true\n---\n\nRun `orchestrate guide {phase}` and follow only that fixed phase.\n"
                );
                if skill.exists() && fs::read_to_string(&skill)? != content && !marker.exists() {
                    bail!("refusing to overwrite unowned skill {}", skill.display());
                }
                fs::write(&skill, content)?;
                fs::write(marker, "orchestrate 0.1\n")?;
            }
            output(
                "SUCCESS",
                "SKILLS_INSTALLED",
                serde_json::json!({"prefix": prefix}),
            );
        }
    };
    Ok(())
}
impl From<ProvenanceArgs> for Provenance {
    fn from(value: ProvenanceArgs) -> Self {
        Self {
            host: value.host,
            execution_provider: value.execution_provider,
            requested_model: value.requested_model,
            observed_model: value.observed_model,
            requested_effort: value.requested_effort,
            observed_effort: value.observed_effort,
            guide_digest: orchestrate_contracts::digest_bytes(b"orchestrate-guides-v0.1"),
            independence: value.independence,
        }
    }
}

fn parse_independence(
    value: &str,
) -> std::result::Result<orchestrate_contracts::Independence, String> {
    match value {
        "input_excluded_cooperative" => {
            Ok(orchestrate_contracts::Independence::InputExcludedCooperative)
        }
        "access_enforced" => Ok(orchestrate_contracts::Independence::AccessEnforced),
        "compromised" => Ok(orchestrate_contracts::Independence::Compromised),
        "unknown" => Ok(orchestrate_contracts::Independence::Unknown),
        _ => Err(
            "expected input_excluded_cooperative, access_enforced, compromised, or unknown"
                .to_owned(),
        ),
    }
}

fn timestamp_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
        .to_string()
}
