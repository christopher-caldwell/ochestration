use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use orchestrate_contracts::{
    ArtifactRef, AuditAssessment, ConsensusProposal, EvidenceKind, EvidenceStatus,
    ImplementationStatus, Provenance, RequestKind,
};
use orchestrate_core::Store;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

mod prepared_request;

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
    project: Option<PathBuf>,
    #[arg(long)]
    effort: Option<String>,
    #[arg(long)]
    from_file: Option<PathBuf>,
    #[arg(long, conflicts_with = "request_file")]
    request: Option<String>,
    #[arg(long, conflicts_with = "request")]
    request_file: Option<PathBuf>,
    #[arg(long, value_parser = parse_request_kind)]
    request_kind: Option<RequestKind>,
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
        #[command(flatten)]
        provenance: ProvenanceArgs,
    },
    Validate {
        #[arg(long)]
        effort: String,
        #[arg(long)]
        run: String,
    },
    Finalize {
        #[arg(long)]
        effort: String,
        #[arg(long)]
        run: String,
    },
}
#[derive(Subcommand)]
enum Consensus {
    Inputs {
        #[arg(long)]
        effort: String,
        #[arg(
            long = "opinion",
            help = "Discovery artifact selector; omit to resolve the single eligible artifact per slot"
        )]
        opinions: Vec<String>,
    },
    Finalize {
        #[arg(long)]
        effort: String,
        #[arg(
            long = "opinion",
            help = "Discovery artifact selector; omit to infer the single eligible artifact per slot"
        )]
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
        #[arg(
            long,
            help = "Agreement artifact selector; inferred when exactly one eligible Agreement exists"
        )]
        agreement: Option<String>,
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
        #[arg(
            long,
            help = "Adoption receipt selector; inferred when exactly one eligible Adoption exists"
        )]
        adoption: Option<String>,
        #[arg(long, default_value = "HEAD", help = "Git revision to register")]
        commit: String,
        #[arg(long, default_value = "external implementation")]
        declaration: String,
        #[arg(long, default_value = "submitted")]
        status: String,
        #[command(flatten)]
        provenance: ProvenanceArgs,
    },
}
#[derive(Subcommand)]
enum AuditCommand {
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
    #[arg(long)]
    provider: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long = "model-effort")]
    model_effort: Option<String>,
    #[arg(long,default_value="input_excluded",value_parser=parse_independence)]
    independence: orchestrate_contracts::Independence,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("orchestrate: {error:#}");
        std::process::exit(2)
    }
}
fn run() -> Result<()> {
    let Cli { root, command } = Cli::parse();
    match command {
        Command::Guide { phase } => guide(phase),
        Command::Skills { command } => skills(command),
        Command::Init(args) => {
            let input = resolve_init_input(root, args)?;
            let store = Store::open(&input.root)?;
            initialize(&store, input)
        }
        command => execute(store(root)?, command),
    }
}
struct InitInput {
    root: PathBuf,
    project: PathBuf,
    effort: String,
    request_kind: RequestKind,
    request: String,
    constraints: Vec<String>,
}
fn resolve_init_input(root: Option<PathBuf>, args: Init) -> Result<InitInput> {
    if let Some(path) = args.from_file {
        ensure!(
            root.is_none()
                && args.project.is_none()
                && args.effort.is_none()
                && args.request.is_none()
                && args.request_file.is_none()
                && args.request_kind.is_none()
                && args.constraints.is_empty(),
            "--from-file cannot be combined with --root, --project, --effort, --request, --request-file, --request-kind, or --constraint"
        );
        let prepared = prepared_request::read(&path)?;
        return Ok(InitInput {
            root: prepared.root,
            project: prepared.project,
            effort: prepared.effort,
            request_kind: prepared.request_kind,
            request: prepared.body,
            constraints: prepared.constraints,
        });
    }

    let project = args
        .project
        .context("--project is required unless --from-file is used")?;
    let effort = args
        .effort
        .context("--effort is required unless --from-file is used")?;
    let (request_kind, request) = match (args.request, args.request_file) {
        (Some(request), None) => (args.request_kind.unwrap_or(RequestKind::Freeform), request),
        (None, Some(path)) => {
            let request_kind = args
                .request_kind
                .context("--request-file requires --request-kind")?;
            let bytes = fs::read(&path)
                .with_context(|| format!("cannot read request file {}", path.display()))?;
            let request = String::from_utf8(bytes)
                .with_context(|| format!("request file {} is not valid UTF-8", path.display()))?;
            (request_kind, request)
        }
        (None, None) => bail!("provide exactly one of --request or --request-file"),
        (Some(_), Some(_)) => unreachable!("Clap enforces mutually exclusive request inputs"),
    };
    Ok(InitInput {
        root: root.unwrap_or_else(default_root),
        project,
        effort,
        request_kind,
        request,
        constraints: args.constraints,
    })
}
fn initialize(store: &Store, input: InitInput) -> Result<()> {
    let effort = store.init_effort(
        &input.project,
        &input.effort,
        input.request_kind,
        input.request,
        input.constraints,
    )?;
    output(
        "SUCCESS",
        "COHORT_CREATED",
        serde_json::json!({"effort":effort.id,"cohort":effort.cohort.id,"baseline":effort.cohort.baseline_commit}),
    );
    Ok(())
}
fn execute(store: Store, command: Command) -> Result<()> {
    match command {
        Command::Init(_) => unreachable!("init is resolved before the store is opened"),
        Command::Discovery {
            command:
                Discovery::Prepare {
                    effort,
                    slot,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let (run, workspace) = orchestrate_discovery::prepare(
                &store,
                &effort,
                &slot,
                provenance.for_guide(DISCOVERY_GUIDE),
            )?;
            output(
                "SUCCESS",
                "PREPARED",
                serde_json::json!({"run":run,"workspace":workspace,"frozen_source":workspace.join("source")}),
            );
        }
        Command::Discovery {
            command: Discovery::Validate { effort, run },
        } => {
            let effort = store.load_effort(&effort)?;
            let result = orchestrate_discovery::validate(&store, &effort, &run)?;
            let details = if result.summary.outcome == "BLOCKED" {
                let questions: Vec<_> = result
                    .nodes
                    .iter()
                    .filter(|node| {
                        node.kind == EvidenceKind::Question
                            && node.status == EvidenceStatus::Blocked
                    })
                    .map(|node| {
                        serde_json::json!({"id":node.id,"title":node.title,"body":node.body})
                    })
                    .collect();
                serde_json::json!({"run":run,"questions":questions,"summary":result.summary,"nodes":result.nodes.len()})
            } else {
                serde_json::json!({"run":run,"summary":result.summary,"nodes":result.nodes.len()})
            };
            output("SUCCESS", &result.summary.outcome, details);
        }
        Command::Discovery {
            command: Discovery::Finalize { effort, run },
        } => {
            let effort = store.load_effort(&effort)?;
            let reference = orchestrate_discovery::finalize(&store, &effort, &run)?;
            let outcome = store.load_envelope(&effort, &reference)?.outcome;
            output(
                "SUCCESS",
                "FINALIZED",
                serde_json::json!({"artifact":reference,"outcome":outcome}),
            );
        }
        Command::Consensus {
            command: Consensus::Inputs { effort, opinions },
        } => {
            let effort = store.load_effort(&effort)?;
            let opinions = consensus_opinions(&store, &effort, opinions)?;
            output(
                "SUCCESS",
                "READ_ONLY",
                serde_json::json!({"opinions":opinions}),
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
            let refs = consensus_opinions(&store, &effort, opinions)?;
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
            let agreement = match agreement {
                Some(artifact_id) => store.find_artifact_ref(&effort, &artifact_id)?,
                None => orchestrate_audit::select_agreement(&store, &effort)?,
            };
            let reference = orchestrate_audit::adopt(
                &store,
                &effort,
                agreement,
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
                    commit,
                    declaration,
                    status,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let adoption = match adoption {
                Some(artifact_id) => store.find_artifact_ref(&effort, &artifact_id)?,
                None => orchestrate_audit::select_adoption(&store, &effort)?,
            };
            let status = parse_status(&status)?;
            let reference = orchestrate_audit::register_implementation(
                &store,
                &effort,
                adoption,
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
            let verdict = store.load_envelope(&effort, &reference)?.outcome;
            output(
                "SUCCESS",
                "ASSESSMENT_FINALIZED",
                serde_json::json!({"audit":reference,"verdict":verdict}),
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
fn consensus_opinions(
    store: &Store,
    effort: &orchestrate_core::Effort,
    opinions: Vec<String>,
) -> Result<BTreeMap<String, ArtifactRef>> {
    if opinions.is_empty() {
        return orchestrate_consensus::select_opinions(store, effort);
    }
    orchestrate_consensus::bind_opinions(store, effort, &opinions)
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
            provider: self.provider,
            model: self.model,
            model_effort: self.model_effort,
            guide_digest: orchestrate_contracts::digest_bytes(guide.as_bytes()),
            independence: self.independence,
        }
    }
}
fn parse_request_kind(value: &str) -> std::result::Result<RequestKind, String> {
    match value {
        "ticket" => Ok(RequestKind::Ticket),
        "freeform" => Ok(RequestKind::Freeform),
        _ => Err("request kind must be ticket or freeform".into()),
    }
}
fn parse_independence(
    value: &str,
) -> std::result::Result<orchestrate_contracts::Independence, String> {
    match value {
        "input_excluded" => Ok(orchestrate_contracts::Independence::InputExcluded),
        "compromised" => Ok(orchestrate_contracts::Independence::Compromised),
        "unknown" => Ok(orchestrate_contracts::Independence::Unknown),
        _ => Err("expected input_excluded, compromised, or unknown".into()),
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
