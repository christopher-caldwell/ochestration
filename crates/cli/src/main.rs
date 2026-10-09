use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use orchestrate_contracts::{
    AuditAssessment, EvidenceKind, EvidenceStatus, ImplementationStatus, Provenance,
    ReconcileProposal, RequestKind,
};
use orchestrate_core::{PreparedSource, Store, now_ms};
use std::{
    fs,
    path::{Path, PathBuf},
};

mod build_display;
#[cfg(test)]
mod build_integration_tests;
#[cfg(test)]
#[path = "../../build/test_support/mod.rs"]
mod build_test_support;
mod chat_import;
mod command_display;
mod import_display;
mod prepared_request;

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
    /// Import a collaborative Chat Discovery ZIP into the current repository; stop before Build.
    Import {
        bundle: PathBuf,
        #[arg(long, help = "Print the structured import result as JSON")]
        json: bool,
    },
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
    Investigate {
        #[command(subcommand)]
        command: InvestigateCommand,
    },
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    /// Run an explicitly authorized Build, print help, or materialize templates.
    #[command(args_conflicts_with_subcommands = true)]
    Build {
        #[arg(
            long,
            help = "Prepared effort; inferred only when exactly one Build is eligible"
        )]
        effort: Option<String>,
        #[command(subcommand)]
        command: Option<BuildCommand>,
    },
    PrepDiscoveryTicket {
        #[command(subcommand)]
        command: GuideOnly,
    },
    PrepDiscoveryFreeform {
        #[command(subcommand)]
        command: GuideOnly,
    },
    Work {
        #[command(subcommand)]
        command: GuideOnly,
    },
    Review {
        #[command(subcommand)]
        command: GuideOnly,
    },
    FinalAudit {
        #[command(subcommand)]
        command: GuideOnly,
    },
    Unblock {
        #[command(subcommand)]
        command: GuideOnly,
    },
    Journal(SelectEffort),
    Status(SelectEffort),
    Inspect(Inspect),
    Lineage(Inspect),
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
enum GuideOnly {
    /// Print the current embedded guide and exit.
    Guide,
}
#[derive(Subcommand)]
enum BuildCommand {
    /// Print the current embedded Build guide and exit.
    Guide,
    /// Non-executing preparation/help path; prints the canonical Build guide.
    Prepare,
    /// Write plan.json and config.toml into the effort Build directory.
    Scaffold {
        #[arg(long)]
        effort: String,
    },
    /// Read-only status of a prepared, running or stopped Build.
    Status {
        #[arg(long)]
        effort: String,
        #[arg(long, help = "Print the structured Build status as JSON")]
        json: bool,
    },
    /// Restore the checkpoint and requeue the same gate after an explicit reset.
    Reset {
        #[arg(long)]
        effort: String,
    },
}
#[derive(Subcommand)]
enum Discovery {
    /// Print the current embedded Discovery guide and exit.
    Guide,
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
    /// Print the current embedded Reconcile guide and exit.
    Guide,
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
enum InvestigateCommand {
    /// Print the embedded investigation guide without initializing storage.
    Guide,
    /// Launch one explicit, fixed-cohort investigation from a versioned config.
    Run {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Verify retained artifacts and inspect a run offline; never resumes it.
    Inspect {
        #[arg(long)]
        run: String,
        #[arg(long)]
        json: bool,
    },
}
#[derive(Subcommand)]
enum AuditCommand {
    /// Print the current embedded Audit guide and exit.
    Guide,
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
    let include_store_root = root.is_some();
    if let Some(action) = command.guide_action() {
        print!(
            "{}",
            orchestrate_guides::guide(action).expect("registered guide")
        );
        return Ok(());
    }
    match command {
        Command::Investigate { command } => {
            let root = normalized_absolute_path(&root.unwrap_or_else(default_root))?;
            let (result, as_json) = match command {
                InvestigateCommand::Run { config, json } => {
                    (orchestrate_investigate::run(&root, &config)?, json)
                }
                InvestigateCommand::Inspect { run, json } => {
                    (orchestrate_investigate::inspect(&root, &run)?, json)
                }
                InvestigateCommand::Guide => unreachable!("guide handled before storage"),
            };
            if as_json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                print!("{}", orchestrate_investigate::aggregation::render(&result));
                println!(
                    "\nArtifacts: {}",
                    root.join("investigations").join(&result.run_id).display()
                );
            }
            Ok(())
        }
        Command::Import { bundle, json } => {
            let repo = std::env::current_dir()?;
            let canonical_repo = fs::canonicalize(&repo)?;
            let store_root = normalized_absolute_path(&root.unwrap_or_else(default_root))?;
            ensure_outside_project(&store_root, &canonical_repo, "orchestration store root")?;
            let store = Store::open(store_root)?;
            let result = chat_import::import(&store, &repo, &bundle)?;
            if json {
                output("SUCCESS", "IMPLEMENTATION_READY", result);
            } else {
                let editor = std::env::var("VISUAL")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .or_else(|| {
                        std::env::var("EDITOR")
                            .ok()
                            .filter(|value| !value.trim().is_empty())
                    });
                print!(
                    "{}",
                    import_display::render(
                        &result,
                        editor.as_deref(),
                        include_store_root.then(|| store.root())
                    )?
                );
            }
            Ok(())
        }
        Command::Init(args) => {
            let input = resolve_init_input(root, args)?;
            let canonical_repo = fs::canonicalize(&input.project).with_context(|| {
                format!("cannot canonicalize repository {}", input.project.display())
            })?;
            let store_root = normalized_absolute_path(&input.root)?;
            ensure_outside_project(&store_root, &canonical_repo, "orchestration store root")?;
            initialize(&Store::open(&input.root)?, input)
        }
        command => execute(store(root)?, command, include_store_root),
    }
}
struct InitInput {
    root: PathBuf,
    project: PathBuf,
    effort: String,
    request_kind: RequestKind,
    request: String,
    constraints: Vec<String>,
    prepared_source: Option<PreparedSource>,
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
        let absolute_path = fs::canonicalize(&path)
            .with_context(|| format!("cannot canonicalize prepared request {}", path.display()))?;
        let prepared_sha256 = orchestrate_contracts::digest_bytes(&fs::read(&absolute_path)?);
        let prepared = prepared_request::read(&absolute_path)?;
        return Ok(InitInput {
            root: root.unwrap_or(prepared.root),
            project: prepared.project,
            effort: prepared.effort,
            request_kind: prepared.request_kind,
            request: prepared.body,
            constraints: prepared.constraints,
            prepared_source: Some(PreparedSource {
                absolute_path,
                sha256: prepared_sha256,
                initialized_at_ms: now_ms(),
            }),
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
        prepared_source: None,
    })
}
fn initialize(store: &Store, input: InitInput) -> Result<()> {
    let canonical_repo = fs::canonicalize(&input.project)
        .with_context(|| format!("cannot canonicalize repository {}", input.project.display()))?;
    ensure_outside_project(store.root(), &canonical_repo, "orchestration store root")?;
    let effort = store.init_effort_with_source(
        &canonical_repo,
        &input.effort,
        input.request_kind,
        input.request,
        input.constraints,
        input.prepared_source,
    )?;
    output(
        "SUCCESS",
        "EFFORT_READY",
        serde_json::json!({"effort": effort.id, "context": effort.context.id, "baseline": effort.baseline_commit, "baseline_tree": effort.baseline_tree}),
    );
    Ok(())
}

fn execute(store: Store, command: Command, include_store_root: bool) -> Result<()> {
    match command {
        Command::Investigate { .. }
        | Command::Init(_)
        | Command::Import { .. }
        | Command::Discovery {
            command: Discovery::Guide,
        }
        | Command::Reconcile {
            command: Reconcile::Guide,
        }
        | Command::Audit {
            command: AuditCommand::Guide,
        }
        | Command::Build {
            command: Some(BuildCommand::Guide),
            ..
        }
        | Command::Build {
            command: Some(BuildCommand::Prepare),
            ..
        }
        | Command::PrepDiscoveryTicket { .. }
        | Command::PrepDiscoveryFreeform { .. }
        | Command::Work { .. }
        | Command::Review { .. }
        | Command::FinalAudit { .. }
        | Command::Unblock { .. } => unreachable!(),
        Command::Discovery {
            command: Discovery::Prepare { effort, provenance },
        } => {
            let effort = store.load_effort(&effort)?;
            let (run, workspace) = orchestrate_discovery::prepare(
                &store,
                &effort,
                provenance.for_guide(orchestrate_guides::DISCOVERY),
            )?;
            let run_metadata: orchestrate_contracts::DiscoveryRun =
                json_file(&workspace.join("run.json"))?;
            output(
                "SUCCESS",
                "PREPARED",
                serde_json::json!({"run": run, "source_label": run_metadata.source_label, "workspace": workspace, "frozen_source": workspace.join("source"), "scratch": workspace.join("scratch")}),
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
            let artifact_path = store.artifact_dir(&effort, &reference.artifact_id)?;
            output(
                "SUCCESS",
                "FINALIZED",
                serde_json::json!({"artifact": reference, "artifact_path": artifact_path, "outcome": outcome}),
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
                provenance.for_guide(orchestrate_guides::RECONCILE),
            )?;
            let outcome = store.load_envelope(&effort, &reference)?.outcome;
            let artifact_path = store.artifact_dir(&effort, &reference.artifact_id)?;
            output(
                "SUCCESS",
                &outcome,
                serde_json::json!({"reconciled": reference, "artifact_path": artifact_path, "outcome": outcome}),
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
                provenance.for_guide(orchestrate_guides::RECONCILE),
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
                provenance.for_guide(orchestrate_guides::AUDIT),
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
                provenance.for_guide(orchestrate_guides::AUDIT),
            )?;
            let verdict = store.load_envelope(&effort, &reference)?.outcome;
            output(
                "SUCCESS",
                "ASSESSMENT_FINALIZED",
                serde_json::json!({"audit": reference, "verdict": verdict}),
            );
        }
        Command::Build {
            effort,
            command: None,
        } => println!(
            "{}",
            build_output(orchestrate_build::run_with_observer(
                &store,
                orchestrate_build::BuildRequest {
                    effort,
                    project: std::env::current_dir()?,
                },
                &mut build_display::Display::new(std::io::stderr(), build_display::Mode::stderr())
                    .with_root(include_store_root.then(|| store.root().to_path_buf())),
            )?)
        ),
        Command::Build {
            command: Some(BuildCommand::Status { effort, json }),
            ..
        } => {
            let status = orchestrate_build::status(&store, &effort)?;
            if json {
                output("SUCCESS", "READ_ONLY", status);
            } else {
                let resolved_effort = store.load_effort(&effort)?;
                let build_dir = store.effort_dir(&resolved_effort).join("build");
                let state = if status["status"] == "uninitialized" {
                    None
                } else {
                    Some(serde_json::from_value::<orchestrate_build::BuildState>(
                        status,
                    )?)
                };
                let plan = state.as_ref().and_then(|state| {
                    orchestrate_build::controller::observation_plan(&build_dir, state)
                });
                print!(
                    "{}",
                    build_display::status_report(
                        &resolved_effort.id,
                        &build_dir,
                        state.as_ref(),
                        plan.as_ref(),
                        include_store_root.then(|| store.root())
                    )
                );
            }
        }
        Command::Build {
            command: Some(BuildCommand::Reset { effort }),
            ..
        } => {
            output(
                "SUCCESS",
                "RESET",
                orchestrate_build::reset(&store, &effort)?,
            );
        }
        Command::Build {
            command: Some(BuildCommand::Scaffold { effort }),
            ..
        } => {
            let directory = orchestrate_build::scaffold(&store, &effort)?;
            output(
                "SUCCESS",
                "SCAFFOLDED",
                serde_json::json!({"build_dir": directory}),
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
fn build_output(result: orchestrate_build::BuildResult) -> serde_json::Value {
    match result {
        orchestrate_build::BuildResult::Completed(done) => output_value(
            "SUCCESS",
            "BUILD_COMPLETE",
            serde_json::json!({"implementation": done.implementation, "audit": done.audit}),
        ),
        orchestrate_build::BuildResult::Blocked { detail, state } => output_value(
            "STOPPED",
            "BLOCKED",
            serde_json::json!({"detail": detail, "state": state}),
        ),
    }
}
fn output_value(
    operation_status: &str,
    semantic_outcome: &str,
    details: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({"operation_status": operation_status, "semantic_outcome": semantic_outcome, "details": details})
}
fn output(operation_status: &str, semantic_outcome: &str, details: serde_json::Value) {
    println!(
        "{}",
        output_value(operation_status, semantic_outcome, details)
    )
}

impl Command {
    /// The action whose embedded guide this command prints, if it is a guide lookup.
    /// Guide lookup must not open a store, so `run` handles it before any other work.
    fn guide_action(&self) -> Option<&'static str> {
        match self {
            Command::Investigate {
                command: InvestigateCommand::Guide,
            } => Some("investigate"),
            Command::Discovery {
                command: Discovery::Guide,
            } => Some("discovery"),
            Command::Reconcile {
                command: Reconcile::Guide,
            } => Some("reconcile"),
            Command::Audit {
                command: AuditCommand::Guide,
            } => Some("audit"),
            Command::Build {
                command: Some(BuildCommand::Guide),
                ..
            } => Some("build"),
            Command::Build {
                command: Some(BuildCommand::Prepare),
                ..
            } => Some("build"),
            Command::PrepDiscoveryTicket {
                command: GuideOnly::Guide,
            } => Some("prep-discovery-ticket"),
            Command::PrepDiscoveryFreeform {
                command: GuideOnly::Guide,
            } => Some("prep-discovery-freeform"),
            Command::Work {
                command: GuideOnly::Guide,
            } => Some("work"),
            Command::Review {
                command: GuideOnly::Guide,
            } => Some("review"),
            Command::FinalAudit {
                command: GuideOnly::Guide,
            } => Some("final-audit"),
            Command::Unblock {
                command: GuideOnly::Guide,
            } => Some("unblock"),
            _ => None,
        }
    }
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
