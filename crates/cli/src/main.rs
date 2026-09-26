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
    /// Optional advisory whole-implementation once-over. Internal role guide.
    OnceOver {
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
    },
    /// Bounded preflight of the configured execution path.  Static by default;
    /// a live probe needs explicit authorization.
    Preflight {
        #[arg(long)]
        effort: String,
        #[arg(
            long,
            help = "Run the optional live probe; refused without --authorize-live"
        )]
        live: bool,
        #[arg(
            long = "authorize-live",
            help = "Recorded authorization to spend provider inference on the live probe"
        )]
        authorize_live: bool,
    },
    /// Remove eligible generated Cargo products from inactive owned checkouts.
    Cleanup {
        #[arg(long)]
        effort: String,
        #[arg(
            long = "dry-run",
            help = "Report what would be removed and remove nothing"
        )]
        dry_run: bool,
    },
    /// Export one effort's evidence, verifying membership and hashes before
    /// promoting the archive.
    Export {
        #[arg(long)]
        effort: String,
        #[arg(long, help = "Final archive path; a .partial file is used first")]
        output: PathBuf,
    },
    /// Record an operator intervention on a stopped Build and create the
    /// continuation boundary it authorizes.  Dispatches no provider action.
    Resolve {
        #[arg(long)]
        effort: String,
        #[arg(long, help = "Exact stopped action id reported by the stopped Build")]
        action: String,
        #[arg(long, value_parser = parse_resolution_kind)]
        kind: orchestrate_build::ResolutionKind,
        #[arg(long, conflicts_with = "note_file")]
        note: Option<String>,
        #[arg(long = "note-file", conflicts_with = "note")]
        note_file: Option<PathBuf>,
        #[arg(
            long = "evidence",
            help = "Evidence file bound to this resolution; repeatable"
        )]
        evidence: Vec<PathBuf>,
        #[arg(
            long = "confirm-not-running",
            help = "Recorded confirmation that an accepted-but-uncertain provider action is no longer running"
        )]
        confirm_not_running: bool,
        #[arg(
            long,
            help = "New role configuration for future invocations; environment_repair only"
        )]
        config: Option<PathBuf>,
    },
    /// Record an additive authority refusal for a historical in-flight action
    /// with no durable stop. This does not change state or resume provider work.
    AmendAuthority {
        #[arg(long)]
        effort: String,
        #[arg(long, help = "Exact current action id from the historical Build")]
        action: String,
        #[arg(long, conflicts_with = "note_file")]
        note: Option<String>,
        #[arg(long = "note-file", conflicts_with = "note")]
        note_file: Option<PathBuf>,
        #[arg(
            long = "evidence",
            help = "Evidence file bound to this authority amendment; repeatable"
        )]
        evidence: Vec<PathBuf>,
        #[arg(
            long = "confirm-not-running",
            help = "Recorded confirmation that the historical provider action is no longer running"
        )]
        confirm_not_running: bool,
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
    if let Some(action) = command.guide_action() {
        print!(
            "{}",
            orchestrate_guides::guide(action).expect("registered guide")
        );
        return Ok(());
    }
    match command {
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

fn execute(store: Store, command: Command) -> Result<()> {
    match command {
        Command::Init(_)
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
        | Command::Unblock { .. }
        | Command::OnceOver { .. } => unreachable!(),
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
        } => match orchestrate_build::run(
            &store,
            orchestrate_build::BuildRequest {
                effort,
                project: std::env::current_dir()?,
            },
        )? {
            orchestrate_build::BuildResult::Completed(done) => output(
                "SUCCESS",
                "BUILD_COMPLETE",
                serde_json::json!({"implementation": done.implementation, "audit": done.audit}),
            ),
            orchestrate_build::BuildResult::Blocked {
                detail,
                state,
                trigger,
                stopped_action,
                stop_record,
            } => output(
                "STOPPED",
                "BLOCKED",
                serde_json::json!({
                    "detail": detail,
                    "state": state,
                    "trigger": trigger,
                    "stopped_action": stopped_action,
                    "stop_record": stop_record,
                }),
            ),
        },
        Command::Build {
            command:
                Some(BuildCommand::Resolve {
                    effort,
                    action,
                    kind,
                    note,
                    note_file,
                    evidence,
                    confirm_not_running,
                    config,
                }),
            ..
        } => {
            let note = match (note, note_file) {
                (Some(note), None) => note,
                (None, Some(path)) => fs::read_to_string(&path)
                    .with_context(|| format!("cannot read {}", path.display()))?,
                (None, None) => bail!("a resolution requires --note or --note-file"),
                (Some(_), Some(_)) => unreachable!("clap rejects both note sources"),
            };
            let outcome = orchestrate_build::resolve(
                &store,
                orchestrate_build::ResolutionRequest {
                    effort,
                    action,
                    kind,
                    note,
                    evidence,
                    confirm_not_running,
                    config,
                },
            )?;
            match outcome {
                orchestrate_build::ResolutionOutcome::Resolved {
                    resolution,
                    resolution_id,
                    kind,
                    stopped_action,
                    continuation_action,
                    continuation_kind,
                    config_version,
                } => output(
                    "SUCCESS",
                    "RESOLVED",
                    serde_json::json!({
                        "resolution": resolution,
                        "resolution_id": resolution_id,
                        "kind": kind,
                        "stopped_action": stopped_action,
                        "continuation_action": continuation_action,
                        "continuation_kind": continuation_kind,
                        "config_version": config_version,
                        "next": "run `orchestrate build` to dispatch the authorized continuation",
                    }),
                ),
                orchestrate_build::ResolutionOutcome::Refused {
                    resolution,
                    resolution_id,
                    reason,
                    successor_guidance,
                } => output(
                    "SUCCESS",
                    "REFUSED",
                    serde_json::json!({
                        "resolution": resolution,
                        "resolution_id": resolution_id,
                        "reason": reason,
                        "successor_guidance": successor_guidance,
                    }),
                ),
            }
        }
        Command::Build {
            command:
                Some(BuildCommand::AmendAuthority {
                    effort,
                    action,
                    note,
                    note_file,
                    evidence,
                    confirm_not_running,
                }),
            ..
        } => {
            let note = match (note, note_file) {
                (Some(note), None) => note,
                (None, Some(path)) => fs::read_to_string(&path)
                    .with_context(|| format!("cannot read {}", path.display()))?,
                (None, None) => bail!("an authority amendment requires --note or --note-file"),
                (Some(_), Some(_)) => unreachable!("clap rejects both note sources"),
            };
            let outcome = orchestrate_build::amend_authority(
                &store,
                orchestrate_build::AuthorityAmendmentRequest {
                    effort,
                    action,
                    note,
                    evidence,
                    confirm_not_running,
                },
            )?;
            output(
                "SUCCESS",
                "AUTHORITY_AMENDED_REFUSED",
                serde_json::to_value(outcome)?,
            );
        }
        Command::Build {
            command: Some(BuildCommand::Status { effort }),
            ..
        } => {
            let effort = store.load_effort(&effort)?;
            let status = orchestrate_build::status::build_status(&store, &effort)?;
            // The human summary goes to stderr; stdout stays one JSON value.
            for line in orchestrate_build::status::status_lines(&store, &effort)? {
                eprintln!("{line}");
            }
            output("SUCCESS", "READ_ONLY", status);
        }
        Command::Build {
            command:
                Some(BuildCommand::Preflight {
                    effort,
                    live,
                    authorize_live,
                }),
            ..
        } => {
            let effort = store.load_effort(&effort)?;
            let mut report = orchestrate_build::preflight::static_preflight(&store, &effort)?;
            if live {
                // The live probe runs only with the explicit authorization the
                // contract requires; without it the refusal and its disclosure
                // are returned instead of inference.
                let probe = orchestrate_build::preflight::live_preflight_with_local_adapters(
                    &store,
                    &effort,
                    authorize_live,
                )?;
                report.live = Some(probe);
                report.mode = "live";
            }
            let retained = orchestrate_build::preflight::retain_report(&store, &effort, &report)?;
            output(
                "SUCCESS",
                "READ_ONLY",
                serde_json::json!({"preflight": report, "retained": retained}),
            );
        }
        Command::Build {
            command: Some(BuildCommand::Cleanup { effort, dry_run }),
            ..
        } => {
            let effort = store.load_effort(&effort)?;
            let outcome = orchestrate_build::cleanup::cleanup(&store, &effort, dry_run)?;
            let complete = outcome.complete;
            output(
                "SUCCESS",
                if complete {
                    "CLEANUP_COMPLETE"
                } else {
                    "CLEANUP_INCOMPLETE"
                },
                serde_json::to_value(&outcome)?,
            );
        }
        Command::Build {
            command:
                Some(BuildCommand::Export {
                    effort,
                    output: archive,
                }),
            ..
        } => {
            let effort = store.load_effort(&effort)?;
            let outcome = orchestrate_build::export::export(&store, &effort, &archive)?;
            let complete = outcome.complete;
            let status = outcome.export_status;
            // Collection outcome is reported independently of any Build
            // semantic outcome; a failure here is this command's own failure.
            output("SUCCESS", status, serde_json::to_value(&outcome)?);
            if !complete {
                bail!(
                    "evidence export is {status}; no complete archive was promoted ({} failed, {} changed, {} omissions)",
                    outcome.failed,
                    outcome.changed,
                    outcome.omissions.len()
                );
            }
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
fn output(operation_status: &str, semantic_outcome: &str, details: serde_json::Value) {
    println!(
        "{}",
        serde_json::json!({"operation_status": operation_status, "semantic_outcome": semantic_outcome, "details": details})
    )
}
impl Command {
    /// The action whose embedded guide this command prints, if it is a guide lookup.
    /// Guide lookup must not open a store, so `run` handles it before any other work.
    fn guide_action(&self) -> Option<&'static str> {
        match self {
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
            Command::OnceOver {
                command: GuideOnly::Guide,
            } => Some("once-over"),
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
fn parse_resolution_kind(
    value: &str,
) -> std::result::Result<orchestrate_build::ResolutionKind, String> {
    orchestrate_build::ResolutionKind::parse(value).map_err(|error| error.to_string())
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
