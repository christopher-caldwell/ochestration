use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use orchestrate_contracts::{
    ArtifactRef, AuditAssessment, ConsensusProposal, ImplementationStatus, Provenance,
};
use orchestrate_core::Store;
use std::{
    fs,
    path::{Path, PathBuf},
};

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
    Journal(SelectEffort),
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
#[derive(Subcommand)]
enum Discovery {
    Prepare {
        #[arg(long)]
        effort: String,
        #[arg(long)]
        slot: String,
    },
    Validate {
        #[arg(long)]
        effort: String,
        #[arg(long)]
        run: String,
        #[arg(long)]
        outcome: Option<String>,
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
        run: String,
        #[arg(long)]
        outcome: Option<String>,
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
        bundle: PathBuf,
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
        authorization_label: String,
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
        bundle: PathBuf,
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
    #[arg(long,default_value="input_excluded_cooperative",value_parser=parse_independence)]
    independence: orchestrate_contracts::Independence,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("orchestrate: {error:#}");
        std::process::exit(2)
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
                serde_json::json!({"effort":effort.id,"cohort":effort.cohort.id,"baseline":effort.cohort.baseline_commit}),
            );
        }
        Command::Discovery {
            command: Discovery::Prepare { effort, slot },
        } => {
            let effort = store.load_effort(&effort)?;
            let (run, workspace) = orchestrate_discovery::prepare(&store, &effort, &slot)?;
            output(
                "SUCCESS",
                "PREPARED",
                serde_json::json!({"run":run,"workspace":workspace,"frozen_source":workspace.join("source")}),
            );
        }
        Command::Discovery {
            command:
                Discovery::Validate {
                    effort,
                    run,
                    outcome,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let result =
                orchestrate_discovery::validate(&store, &effort, &run, outcome.as_deref())?;
            output(
                "SUCCESS",
                "VALID",
                serde_json::json!({"summary":result.summary,"nodes":result.nodes.len()}),
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
                DISCOVERY_GUIDE,
                provenance.for_guide(DISCOVERY_GUIDE),
            )?;
            output(
                "SUCCESS",
                "FINALIZED",
                serde_json::json!({"artifact":reference}),
            );
        }
        Command::Discovery {
            command:
                Discovery::Finalize {
                    effort,
                    run,
                    outcome,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let reference = orchestrate_discovery::finalize(
                &store,
                &effort,
                &run,
                outcome.as_deref(),
                provenance.for_guide(DISCOVERY_GUIDE),
            )?;
            output(
                "SUCCESS",
                "FINALIZED",
                serde_json::json!({"artifact":reference}),
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
            let effort = store.load_effort(&effort)?;
            let refs = three_refs(&store, &effort, opinions)?;
            let result = orchestrate_consensus::run_provider(
                &store,
                &effort,
                refs,
                &provider,
                CONSENSUS_GUIDE,
                provenance.for_guide(CONSENSUS_GUIDE),
            )?;
            output(
                "SUCCESS",
                if result.agreement.is_some() {
                    "ELIGIBLE_CANDIDATE"
                } else {
                    "NO_CONSENSUS"
                },
                serde_json::json!({"comparison":result.comparison,"agreement":result.agreement}),
            );
        }
        Command::Consensus {
            command:
                Consensus::Finalize {
                    effort,
                    opinions,
                    bundle,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let refs = three_refs(&store, &effort, opinions)?;
            let proposal: ConsensusProposal = json_file(&bundle_file(&bundle, "proposal.json"))?;
            let result = orchestrate_consensus::finalize(
                &store,
                &effort,
                refs,
                proposal,
                provenance.for_guide(CONSENSUS_GUIDE),
            )?;
            output(
                "SUCCESS",
                if result.agreement.is_some() {
                    "ELIGIBLE_CANDIDATE"
                } else {
                    "NO_CONSENSUS"
                },
                serde_json::json!({"comparison":result.comparison,"agreement":result.agreement}),
            );
        }
        Command::Agreement {
            command:
                AgreementCommand::Adopt {
                    effort,
                    agreement,
                    authorization_label,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let reference = orchestrate_audit::adopt(
                &store,
                &effort,
                store.find_artifact_ref(&effort, &agreement)?,
                authorization_label,
                provenance.for_guide(CONSENSUS_GUIDE),
            )?;
            output(
                "SUCCESS",
                "ADOPTED",
                serde_json::json!({"adoption":reference}),
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
            let status = parse_status(&status)?;
            let reference = orchestrate_audit::register_implementation(
                &store,
                &effort,
                store.find_artifact_ref(&effort, &adoption)?,
                &project,
                &commit,
                declaration,
                status,
                provenance.for_guide(CONSENSUS_GUIDE),
            )?;
            output(
                "SUCCESS",
                "REGISTERED_EXTERNAL",
                serde_json::json!({"implementation":reference}),
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
                AUDIT_GUIDE,
                provenance.for_guide(AUDIT_GUIDE),
            )?;
            output(
                "SUCCESS",
                "ASSESSMENT_FINALIZED",
                serde_json::json!({"audit":reference}),
            );
        }
        Command::Audit {
            command:
                AuditCommand::Finalize {
                    effort,
                    bundle,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let assessment: AuditAssessment = json_file(&bundle_file(&bundle, "assessment.json"))?;
            let reference = orchestrate_audit::finalize_audit(
                &store,
                &effort,
                assessment,
                provenance.for_guide(AUDIT_GUIDE),
            )?;
            output(
                "SUCCESS",
                "ASSESSMENT_FINALIZED",
                serde_json::json!({"audit":reference}),
            );
        }
        Command::Journal(args) => {
            let effort = store.load_effort(&args.effort)?;
            output(
                "SUCCESS",
                "READ_ONLY",
                serde_json::to_value(store.read_journal(&effort)?)?,
            );
        }
        Command::Status(args) => {
            let effort = store.load_effort(&args.effort)?;
            output(
                "SUCCESS",
                "READ_ONLY",
                serde_json::json!({"effort":effort,"artifacts":store.list_artifacts(&effort)?}),
            );
        }
        Command::Inspect(args) => {
            let effort = store.load_effort(&args.effort)?;
            let reference = store.find_artifact_ref(&effort, &args.artifact)?;
            let (envelope, files) = store.load_bundle(&effort, &reference)?;
            output(
                "SUCCESS",
                "READ_ONLY",
                serde_json::json!({"manifest":envelope,"files":files.keys().collect::<Vec<_>>() }),
            );
        }
        Command::Lineage(args) => {
            let effort = store.load_effort(&args.effort)?;
            let reference = store.find_artifact_ref(&effort, &args.artifact)?;
            output(
                "SUCCESS",
                "READ_ONLY",
                serde_json::json!({"artifact":reference,"nodes":store.reconstruct_lineage(&effort,&reference)?}),
            );
        }
        Command::Guide { .. } | Command::Skills { .. } => unreachable!(),
    }
    Ok(())
}
fn three_refs(
    store: &Store,
    effort: &orchestrate_core::Effort,
    opinions: Vec<String>,
) -> Result<[ArtifactRef; 3]> {
    ensure!(
        opinions.len() == 3,
        "exactly three --opinion selectors are required"
    );
    opinions
        .iter()
        .map(|id| store.find_artifact_ref(effort, id))
        .collect::<Result<Vec<_>>>()?
        .try_into()
        .map_err(|_| anyhow::anyhow!("three opinions required"))
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
fn bundle_file(bundle: &Path, filename: &str) -> PathBuf {
    if bundle.is_dir() {
        bundle.join(filename)
    } else {
        bundle.to_owned()
    }
}
fn output(operation_status: &str, semantic_outcome: &str, details: serde_json::Value) {
    println!(
        "{}",
        serde_json::json!({"operation_status":operation_status,"semantic_outcome":semantic_outcome,"details":details})
    )
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
                fs::write(
                    dir.join("SKILL.md"),
                    format!(
                        "---\nname: {name}\ndisable-model-invocation: true\n---\n\nRun `orchestrate guide {phase}` and follow that phase boundary.\n"
                    ),
                )?;
            }
            output(
                "SUCCESS",
                "SKILLS_INSTALLED",
                serde_json::json!({"prefix":prefix}),
            );
        }
    }
    Ok(())
}
impl ProvenanceArgs {
    fn for_guide(self, guide: &str) -> Provenance {
        Provenance {
            host: self.host,
            execution_provider: self.execution_provider,
            requested_model: self.requested_model,
            observed_model: self.observed_model,
            requested_effort: self.requested_effort,
            observed_effort: self.observed_effort,
            guide_digest: orchestrate_contracts::digest_bytes(guide.as_bytes()),
            independence: self.independence,
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
            "expected input_excluded_cooperative, access_enforced, compromised, or unknown".into(),
        ),
    }
}
fn parse_status(value: &str) -> Result<ImplementationStatus> {
    match value {
        "submitted" => Ok(ImplementationStatus::Submitted),
        "partial" => Ok(ImplementationStatus::Partial),
        "blocked" => Ok(ImplementationStatus::Blocked),
        _ => bail!("implementation status must be submitted, partial, or blocked"),
    }
}
