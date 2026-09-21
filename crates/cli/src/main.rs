use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use orchestrate_contracts::{
    AuditAssessment, EvidenceKind, EvidenceStatus, ImplementationStatus, Provenance,
    ReconcileProposal, RequestKind,
};
use orchestrate_core::Store;
use std::{
    fs,
    path::{Path, PathBuf},
};

mod prepared_request;

const DISCOVERY_GUIDE: &str = include_str!("../resources/guides/discovery.md");
const RECONCILE_GUIDE: &str = include_str!("../resources/guides/reconcile.md");
const AUDIT_GUIDE: &str = include_str!("../resources/guides/audit.md");

#[derive(Parser)]
#[command(
    name = "orchestrate",
    version,
    about = "Inspectable Discovery × N → Reconcile → Build → Audit"
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
    Reconcile {
        #[command(subcommand)]
        command: Reconcile,
    },
    Implementation {
        #[command(subcommand)]
        command: ImplementationCommand,
    },
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    Build(Build),
    Journal(SelectEffort),
    Status(SelectEffort),
    Inspect(Inspect),
    Lineage(Inspect),
    Guide {
        phase: Option<String>,
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
#[derive(Args)]
struct Build {
    #[arg(
        long,
        help = "Prepared effort; inferred only when exactly one Build is eligible"
    )]
    effort: Option<String>,
}
#[derive(Subcommand)]
enum Discovery {
    Prepare {
        #[arg(long)]
        effort: String,
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
enum Reconcile {
    Inputs {
        #[arg(long)]
        effort: String,
        #[arg(
            long = "discovery",
            required = true,
            help = "Explicit finalized Discovery artifact selector; repeatable"
        )]
        discoveries: Vec<String>,
    },
    Finalize {
        #[arg(long)]
        effort: String,
        #[arg(
            long = "discovery",
            required = true,
            help = "Exact Discovery artifact selector resolved by `reconcile inputs`; repeatable"
        )]
        discoveries: Vec<String>,
        #[arg(long)]
        bundle: PathBuf,
        #[command(flatten)]
        provenance: ProvenanceArgs,
    },
    Adopt {
        #[arg(long)]
        effort: String,
        #[arg(
            long,
            help = "Reconciled Discovery selector; inferred only when exactly one is ready"
        )]
        reconciled: Option<String>,
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
            help = "Adoption receipt selector; inferred only when exactly one is eligible"
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
    #[arg(long, default_value = "input_excluded", value_parser = parse_independence)]
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
        Command::Init(args) => {
            let input = resolve_init_input(root, args)?;
            let canonical_repo = fs::canonicalize(&input.project).with_context(|| {
                format!("cannot canonicalize repository {}", input.project.display())
            })?;
            let store_root = normalized_absolute_path(&input.root)?;
            ensure_outside_project(&store_root, &canonical_repo, "orchestration store root")?;
            initialize(&Store::open(&input.root)?, input)
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
            args.project.is_none()
                && args.effort.is_none()
                && args.request.is_none()
                && args.request_file.is_none()
                && args.request_kind.is_none()
                && args.constraints.is_empty(),
            "--from-file cannot be combined with --project, --effort, --request, --request-file, --request-kind, or --constraint"
        );
        let prepared = prepared_request::read(&path)?;
        return Ok(InitInput {
            root: root.unwrap_or(prepared.root),
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
            let kind = args
                .request_kind
                .context("--request-file requires --request-kind")?;
            let bytes = fs::read(&path)
                .with_context(|| format!("cannot read request file {}", path.display()))?;
            (
                kind,
                String::from_utf8(bytes).with_context(|| {
                    format!("request file {} is not valid UTF-8", path.display())
                })?,
            )
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
    let canonical_repo = fs::canonicalize(&input.project)
        .with_context(|| format!("cannot canonicalize repository {}", input.project.display()))?;
    ensure_outside_project(store.root(), &canonical_repo, "orchestration store root")?;
    let effort = store.init_effort(
        &canonical_repo,
        &input.effort,
        input.request_kind,
        input.request,
        input.constraints,
    )?;
    output(
        "SUCCESS",
        "EFFORT_READY",
        serde_json::json!({"effort": effort.id, "context": effort.context.id, "baseline": effort.baseline_commit, "baseline_tree": effort.baseline_tree}),
    );
    Ok(())
}

fn execute(store: Store, command: Command) -> Result<()> {
    match command {
        Command::Init(_) | Command::Guide { .. } => unreachable!(),
        Command::Discovery {
            command: Discovery::Prepare { effort, provenance },
        } => {
            let effort = store.load_effort(&effort)?;
            let (run, workspace) = orchestrate_discovery::prepare(
                &store,
                &effort,
                provenance.for_guide(DISCOVERY_GUIDE),
            )?;
            output(
                "SUCCESS",
                "PREPARED",
                serde_json::json!({"run": run, "workspace": workspace, "frozen_source": workspace.join("source")}),
            );
        }
        Command::Discovery {
            command: Discovery::Validate { effort, run },
        } => {
            let effort = store.load_effort(&effort)?;
            let result = orchestrate_discovery::validate(&store, &effort, &run)?;
            let details = if result.summary.outcome == "BLOCKED" {
                let questions: Vec<_> = result.nodes.iter().filter(|node| node.kind == EvidenceKind::Question && node.status == EvidenceStatus::Blocked).map(|node| serde_json::json!({"id": node.id, "title": node.title, "body": node.body})).collect();
                serde_json::json!({"run": run, "questions": questions, "summary": result.summary, "nodes": result.nodes.len()})
            } else {
                serde_json::json!({"run": run, "summary": result.summary, "nodes": result.nodes.len()})
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
                serde_json::json!({"artifact": reference, "outcome": outcome}),
            );
        }
        Command::Reconcile {
            command:
                Reconcile::Inputs {
                    effort,
                    discoveries,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let discoveries = orchestrate_reconcile::bind_inputs(&store, &effort, &discoveries)?;
            output(
                "SUCCESS",
                "READ_ONLY",
                serde_json::json!({"discoveries": discoveries}),
            );
        }
        Command::Reconcile {
            command:
                Reconcile::Finalize {
                    effort,
                    discoveries,
                    bundle,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let inputs = orchestrate_reconcile::bind_inputs(&store, &effort, &discoveries)?;
            let proposal: ReconcileProposal =
                json_file(&bundle_file(&bundle, "reconcile-proposal.json"))?;
            let reference = orchestrate_reconcile::finalize(
                &store,
                &effort,
                inputs,
                proposal,
                provenance.for_guide(RECONCILE_GUIDE),
            )?;
            let outcome = store.load_envelope(&effort, &reference)?.outcome;
            output(
                "SUCCESS",
                &outcome,
                serde_json::json!({"reconciled": reference, "outcome": outcome}),
            );
        }
        Command::Reconcile {
            command:
                Reconcile::Adopt {
                    effort,
                    reconciled,
                    authorization_label,
                    provenance,
                },
        } => {
            let effort = store.load_effort(&effort)?;
            let reconciled = match reconciled {
                Some(selector) => store.find_artifact_ref(&effort, &selector)?,
                None => orchestrate_audit::select_reconciled(&store, &effort)?,
            };
            let adoption = orchestrate_audit::adopt(
                &store,
                &effort,
                reconciled,
                authorization_label,
                provenance.for_guide(RECONCILE_GUIDE),
            )?;
            output(
                "SUCCESS",
                "ADOPTED",
                serde_json::json!({"adoption": adoption}),
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
                Some(selector) => store.find_artifact_ref(&effort, &selector)?,
                None => orchestrate_audit::select_adoption(&store, &effort)?,
            };
            let implementation = orchestrate_audit::register_implementation(
                &store,
                &effort,
                adoption,
                &commit,
                declaration,
                parse_status(&status)?,
                provenance.for_guide(AUDIT_GUIDE),
            )?;
            output(
                "SUCCESS",
                "REGISTERED_EXTERNAL",
                serde_json::json!({"implementation": implementation}),
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
                serde_json::json!({"audit": reference, "verdict": verdict}),
            );
        }
        Command::Build(args) => match orchestrate_build::run(
            &store,
            orchestrate_build::BuildRequest {
                effort: args.effort,
                project: std::env::current_dir()?,
            },
        )? {
            orchestrate_build::BuildResult::Completed(done) => output(
                "SUCCESS",
                "BUILD_COMPLETE",
                serde_json::json!({"implementation": done.implementation, "audit": done.audit}),
            ),
            orchestrate_build::BuildResult::Blocked { detail, state } => output(
                "STOPPED",
                "BLOCKED",
                serde_json::json!({"detail": detail, "state": state}),
            ),
        },
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
            let build_dir = store.phase_dir(&effort, "build")?;
            output(
                "SUCCESS",
                "READ_ONLY",
                serde_json::json!({"effort": effort, "build_dir": build_dir, "artifacts": store.list_artifacts(&effort)?}),
            );
        }
        Command::Inspect(args) => {
            let effort = store.load_effort(&args.effort)?;
            let reference = store.find_artifact_ref(&effort, &args.artifact)?;
            let (manifest, files) = store.load_bundle(&effort, &reference)?;
            output(
                "SUCCESS",
                "READ_ONLY",
                serde_json::json!({"manifest": manifest, "files": files.keys().collect::<Vec<_>>() }),
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
    }
    Ok(())
}
fn ensure_outside_project(path: &Path, project: &Path, label: &str) -> Result<()> {
    ensure!(
        !path.starts_with(project),
        "{label} must be outside the target repository {}; got {}",
        project.display(),
        path.display()
    );
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
        serde_json::json!({"operation_status": operation_status, "semantic_outcome": semantic_outcome, "details": details})
    )
}
fn guide(phase: Option<String>) -> Result<()> {
    match phase.as_deref() {
        None => println!("available guides: discovery, reconcile, audit"),
        Some("discovery") => print!("{DISCOVERY_GUIDE}"),
        Some("reconcile") => print!("{RECONCILE_GUIDE}"),
        Some("audit") => print!("{AUDIT_GUIDE}"),
        Some(other) => bail!("unknown guide phase {other}; Build execution is external"),
    };
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
fn normalized_absolute_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    if absolute.exists() {
        return fs::canonicalize(&absolute)
            .with_context(|| format!("cannot canonicalize {}", absolute.display()));
    }
    let mut existing = absolute.clone();
    let mut suffix = Vec::new();
    while !existing.exists() {
        suffix.push(existing.file_name().map(ToOwned::to_owned));
        if !existing.pop() {
            break;
        }
    }
    let mut normalized = fs::canonicalize(&existing)
        .with_context(|| format!("cannot canonicalize {}", existing.display()))?;
    for component in suffix.iter().rev().flatten() {
        normalized.push(component);
    }
    Ok(normalized)
}
