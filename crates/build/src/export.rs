//! Observational evidence export.
//!
//! The expected-member inventory is derived from durable source state and the
//! controller's own writer obligations *before* anything is traversed or read.
//! Bulky contained-checkout product trees and Discovery source/scratch trees are
//! excluded by class and are never walked, so a huge unreadable compilation tree
//! can never be staged.  Members are classified, streamed into an exclusively
//! created partial archive, verified by reading that archive back, and only then
//! promoted to the final name.  Collection outcome is reported separately from
//! the Build's semantic outcome, and export dispatches nothing.
//!
//! Two properties this module deliberately keeps:
//!
//! * **Containment.** A member is only collected when its resolved path stays
//!   inside this effort.  A symlinked directory component — an `evidence` or an
//!   action directory that points elsewhere — is refused before it is traversed
//!   or read, so unrelated files outside the effort can never reach the archive.
//! * **Ownership of scratch paths.** The staging directory and the partial
//!   archive are unique to this export operation and created exclusively.  A
//!   pre-existing file with a similar name belongs to someone else and is left
//!   exactly where it is; only this operation's own artifacts are removed.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail, ensure};
use orchestrate_contracts::{ArtifactKind, digest_bytes};
use orchestrate_core::{Effort, Store};
use serde::Serialize;

use crate::archive::{ZipWriter, central_directory, read_member, safe_member_name};
use crate::{BuildPlan, BuildState, DispatchState, read_json};

/// Files every action directory may hold.  They are enumerated by name, so the
/// inventory never walks a checkout or a product tree.
const ACTION_MEMBERS: &[&str] = &[
    "action.json",
    "instruction.md",
    "binding-requirements.json",
    "report.md",
    "result.json",
    "assessment.json",
    "transport.jsonl",
    "transport.completed",
    "provider-stderr.log",
    "provider-exit.json",
    "invocation.json",
];

/// Root files of one effort.
const EFFORT_MEMBERS: &[&str] = &["effort.json", "request.md", "journal.jsonl"];

/// Records introduced by this controller version.  Their absence is disclosed
/// as a version or interruption fact, never filled with a plausible value.
const NEW_FORMAT_MEMBERS: &[&str] = &[
    "invocation.json",
    "instruction.md",
    "provider-stderr.log",
    "provider-exit.json",
    "binding-requirements.json",
];

/// Build-directory files that a prepared Build always owns.
const BUILD_MEMBERS: &[&str] = &["state.json", "plan.json", "config.toml"];

/// Controller-owned record directories whose whole contained tree is small,
/// immutable and part of the evidence.  They are walked with containment and
/// depth checks so nested records — a live-probe directory, an evidence
/// subdirectory — are collected rather than silently skipped.
const CONTROLLER_DIRECTORIES: &[(&str, &str)] = &[
    ("evidence", "build/evidence"),
    ("resolutions", "build/resolutions"),
    ("config-history", "build/config-history"),
    ("preflight", "build/preflight"),
];

/// How deep a controller-owned record directory may nest before the export
/// refuses to walk further rather than following an unbounded tree.
const MAX_RECORD_DEPTH: usize = 8;

/// Directory classes that are excluded before walking, with the disclosure the
/// operator needs to know what they would have contained.
const EXCLUDED_CLASSES: &[(&str, &str)] = &[
    (
        "contained checkout trees (source/verification/once-over and their target trees)",
        "excluded by class and never opened: they duplicate the product source at an already-exported commit and hold generated Cargo products. The exact commit of each checkout is recorded in state.json, its ownership in the checkout records, and the exported Git evidence restores the same history and any partial work.",
    ),
    (
        "Discovery source/scratch workspaces",
        "excluded by class: Discovery runs keep their own source checkout and scratch research trees, whose findings are carried by the published Discovery artifact.",
    ),
    (
        "outer capture-wrapper logs",
        "not part of the effort: controller stdout/stderr from the outer invocation, shell transcripts and host version dumps live outside the store and are retained by the capture wrapper (docs/examples/build-run-capture.sh), which reports them separately from this archive.",
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberClass {
    Copied,
    /// Legitimately absent at the source, for example a receipt an interrupted
    /// action never wrote.  It is disclosed, not fabricated.
    Absent,
    Failed,
    Changed,
}

#[derive(Clone, Debug, Serialize)]
pub struct MemberOutcome {
    pub name: String,
    pub source: String,
    pub class: MemberClass,
    pub required: bool,
    pub sha256: Option<String>,
    pub bytes: u64,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExportOutcome {
    pub effort: String,
    pub archive: Option<PathBuf>,
    /// The partial archive of this export operation.  It is kept after a failed
    /// collection as evidence of the attempt and is never another operation's
    /// file.
    pub partial: PathBuf,
    /// Scratch paths that already existed and were deliberately not touched.
    pub leftovers: Vec<String>,
    pub export_status: &'static str,
    pub complete: bool,
    pub expected: usize,
    pub copied: usize,
    pub absent: usize,
    pub failed: usize,
    pub changed: usize,
    pub members: Vec<MemberOutcome>,
    pub excluded_classes: Vec<serde_json::Value>,
    /// Referenced evidence that is knowingly outside the selection.
    pub omissions: Vec<String>,
    pub verification: serde_json::Value,
    pub note: &'static str,
}

/// One member the inventory expects, with the reason it is expected.
#[derive(Clone, Debug)]
struct ExpectedMember {
    name: String,
    path: PathBuf,
    required: bool,
    /// Set when selection refused this member before it could be read.  It is
    /// never stat'd, followed or archived.
    refusal: Option<String>,
    /// True for scratch members this export created itself, which live outside
    /// the effort on purpose and are not subject to effort containment.
    staging: bool,
    /// A digest a durable record already published for this member, for example
    /// the hash a resolution recorded for operator-supplied evidence.  Bytes
    /// that no longer match it are a change, not a copy.
    expected_sha256: Option<String>,
    /// True for a file an immutable durable record explicitly references at a
    /// path the operator chose, which may legitimately live outside the effort.
    /// It is an exact digest-bound reference, not a traversal of the effort's
    /// own directories, so it is read where the record says it is.
    external_reference: bool,
    detail: Option<String>,
}

impl ExpectedMember {
    fn new(name: String, path: PathBuf, required: bool) -> Self {
        Self {
            name,
            path,
            required,
            refusal: None,
            staging: false,
            expected_sha256: None,
            external_reference: false,
            detail: None,
        }
    }

    fn with_expected_sha256(mut self, sha256: impl Into<String>) -> Self {
        self.expected_sha256 = Some(sha256.into());
        self
    }

    fn referenced(mut self) -> Self {
        self.external_reference = true;
        self
    }

    fn refused(mut self, reason: impl Into<String>) -> Self {
        self.refusal = Some(reason.into());
        self
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn staged(mut self) -> Self {
        self.staging = true;
        self
    }
}

/// Whether `path`, after resolving every symlink, still lives inside `root`
/// (also resolved).  A symlink that stays inside the effort is legitimate and
/// keeps working; one that leaves it is refused before it is traversed or read.
fn contained_in(root: &Path, path: &Path) -> bool {
    match (fs::canonicalize(root), fs::canonicalize(path)) {
        (Ok(root), Ok(path)) => path.starts_with(root),
        _ => false,
    }
}

fn escape_detail(root: &Path, path: &Path) -> String {
    let resolved = fs::canonicalize(path)
        .map(|target| target.to_string_lossy().into_owned())
        .unwrap_or_else(|error| format!("<unresolvable: {error}>"));
    format!(
        "{} resolves to {resolved}, outside {}, so a symlinked directory component is refused rather than followed",
        path.display(),
        root.display()
    )
}

/// The record-directory reader is injectable so traversal errors can be
/// exercised deterministically even when tests run with elevated privileges.
pub(super) trait RecordDirectoryReader {
    fn read_directory(
        &self,
        directory: &Path,
    ) -> std::result::Result<Vec<std::result::Result<PathBuf, String>>, String>;
}

struct FilesystemRecordDirectoryReader;

impl RecordDirectoryReader for FilesystemRecordDirectoryReader {
    fn read_directory(
        &self,
        directory: &Path,
    ) -> std::result::Result<Vec<std::result::Result<PathBuf, String>>, String> {
        fs::read_dir(directory)
            .map(|entries| {
                entries
                    .map(|entry| {
                        entry
                            .map(|entry| entry.path())
                            .map_err(|error| error.to_string())
                    })
                    .collect()
            })
            .map_err(|error| error.to_string())
    }
}

/// True when a member name is one of the records this controller version added.
fn legacy_member(name: &str) -> bool {
    name.rsplit('/')
        .next()
        .is_some_and(|file| NEW_FORMAT_MEMBERS.contains(&file))
}

/// Action identifiers are opaque after the namespace prefix. Resolution
/// continuations use a digest form (`act-<hex>`), while ordinary dispatches use
/// timestamp/pid/counter segments. Validate only path safety here; producers
/// and durable field names establish whether a value is an action reference.
fn safe_action_reference(value: &str) -> bool {
    value.starts_with("act-")
        && value.len() > 4
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// Every action id any durable record mentions: Build state anywhere in its
/// structure, the journal the controller appends to, and the phase reviews it
/// retained.  A reference to an action that ran and whose directory no longer
/// exists is a detectable omission rather than something the copied subset
/// quietly defines; an action that was prepared and never dispatched is a
/// legitimate absence, disclosed as exactly that.
struct ReferencedActions {
    /// Actions whose records existed and are now gone: lost evidence.
    required: BTreeSet<String>,
    /// The current action of a Build that has not dispatched it yet.
    pending: Option<String>,
}

fn referenced_action_ids(
    store: &Store,
    effort: &Effort,
    state: Option<&BuildState>,
) -> Result<ReferencedActions> {
    let mut referenced = BTreeSet::new();
    const ACTION_ID_FIELDS: &[&str] = &[
        "action_id",
        "from_action_id",
        "to_action_id",
        "continuation_action",
        "governed_action",
        "stopped_action",
        "audit_attempt_id",
    ];
    fn insert_action_id(value: &serde_json::Value, referenced: &mut BTreeSet<String>) {
        if let Some(value) = value.as_str().filter(|value| safe_action_reference(value)) {
            referenced.insert(value.to_owned());
        }
    }
    fn collect_json(
        value: &serde_json::Value,
        action_object: bool,
        referenced: &mut BTreeSet<String>,
    ) {
        match value {
            serde_json::Value::String(text) if action_object && safe_action_reference(text) => {
                referenced.insert(text.to_owned());
            }
            serde_json::Value::String(_) => {}
            serde_json::Value::Array(items) => {
                for item in items {
                    collect_json(item, action_object, referenced);
                }
            }
            serde_json::Value::Object(fields) => {
                for (key, item) in fields {
                    if ACTION_ID_FIELDS.contains(&key.as_str()) {
                        insert_action_id(item, referenced);
                    }
                    if key == "id" && action_object {
                        insert_action_id(item, referenced);
                    }
                    let nested_action = matches!(
                        key.as_str(),
                        "action"
                            | "from_action"
                            | "to_action"
                            | "interrupted_action"
                            | "predecessor"
                    );
                    collect_json(item, nested_action, referenced);
                }
            }
            _ => {}
        }
    }
    if let Some(state) = state {
        let value = serde_json::to_value(state)?;
        collect_json(&value, false, &mut referenced);
    }
    let journal = store.effort_dir(effort).join("journal.jsonl");
    if let Ok(text) = fs::read_to_string(&journal) {
        for line in text.lines() {
            // A journal line is one JSON record; a line that does not parse is
            // left to the journal's own member, which is collected verbatim.
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                collect_json(&value, false, &mut referenced);
            }
        }
    }
    // A journal entry is written after an action finished, so anything it names
    // ran.  Only the current action of a Build that has not dispatched it yet is
    // allowed to have no directory at all.
    let pending = state
        .filter(|state| matches!(state.action.dispatch, DispatchState::Prepared))
        .map(|state| state.action.id.clone());
    if let Some(pending) = &pending {
        referenced.remove(pending);
    }
    Ok(ReferencedActions {
        required: referenced,
        pending,
    })
}

/// Which members of one action directory the controller's own writer behaviour
/// makes required.  A completed action that lost its packet, its transport or
/// its receipt is lost evidence, not a legitimate absence; an interrupted or
/// older action that never wrote one is disclosed as that absence.
fn action_stop<'a>(
    state: Option<&'a BuildState>,
    action_id: &str,
) -> Option<&'a crate::StopRecord> {
    let state = state?;
    state
        .stop
        .iter()
        .chain(state.stop_history.iter())
        .find(|stop| stop.action.id == action_id)
}

fn action_packet(action_dir: &Path) -> Option<serde_json::Value> {
    fs::read(action_dir.join("action.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// Return the receipt outcome only when its action and scope match the exact
/// action packet. A completed provider process by itself does not make the
/// receipt valid or prove that an Audit assessment was consumed.
fn matching_receipt(action_dir: &Path) -> Option<String> {
    let packet = action_packet(action_dir)?;
    let receipt: serde_json::Value = fs::read(action_dir.join("result.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())?;
    (receipt.get("action_id")? == packet.get("action_id")?
        && receipt.get("scope")? == packet.get("scope")?)
    .then(|| receipt.get("outcome")?.as_str().map(str::to_owned))?
}

fn action_obligations(
    action_dir: &Path,
    state: Option<&BuildState>,
    published_audits: &BTreeSet<String>,
    transitioned_audits: &BTreeSet<String>,
) -> (HashSet<String>, HashMap<String, String>) {
    let mut required: HashSet<String> = HashSet::new();
    let mut details = HashMap::new();
    // Every controller version writes the packet with the directory.
    required.insert("action.json".into());
    let completed = action_dir.join("transport.completed").is_file();
    let dispatched_by_this_version = action_dir.join("invocation.json").is_file();
    let packet = action_packet(action_dir);
    let action_id = packet
        .as_ref()
        .and_then(|value| value.get("action_id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let kind = packet
        .as_ref()
        .and_then(|value| value.get("kind"))
        .and_then(serde_json::Value::as_str);
    let receipt = matching_receipt(action_dir);
    let stop = action_stop(state, action_id);
    if completed {
        required.insert("transport.jsonl".into());
        let recorded_missing_receipt = stop.is_some_and(|stop| {
            stop.receipt_validation == "receipt missing"
                && stop
                    .action_failure
                    .as_ref()
                    .is_some_and(|failure| *failure == crate::StopTrigger::InvalidReceipt)
        });
        if !recorded_missing_receipt {
            required.insert("result.json".into());
            let detail = match (stop, receipt.as_deref()) {
                (Some(stop), _) if stop.receipt_validation == "receipt present; its validation failed" => format!(
                    "the controller recorded `{}`; these bytes are the invalid receipt it stopped on and were not consumed",
                    stop.receipt_validation
                ),
                (None, None) => "the provider transport completed, no matching receipt exists, and no recorded missing-receipt stop explains its absence; the required receipt evidence is unavailable".into(),
                (_, None) => "the provider transport completed but no receipt matching this action and scope is available; the retained bytes or recorded failure establish what the controller saw".into(),
                _ => "the provider transport completed; this matching receipt is required to establish the controller transition".into(),
            };
            details.insert("result.json".into(), detail);
        } else {
            details.insert(
                "result.json".into(),
                format!(
                    "the controller stop records `{}` for this action; no receipt was produced, so its absence is the recorded failure rather than lost consumed evidence",
                    stop.expect("checked above").receipt_validation
                ),
            );
        }
        if kind == Some("final_audit") {
            match receipt.as_deref() {
                Some("complete")
                    if published_audits.contains(action_id)
                        || transitioned_audits.contains(action_id) =>
                {
                    required.insert("assessment.json".into());
                    details.insert(
                        "assessment.json".into(),
                        "the matching complete Audit receipt and its exact publication or recorded controller transition prove this assessment was consumed; its absence is lost evidence".into(),
                    );
                }
                Some("blocked") => {
                    details.insert(
                        "assessment.json".into(),
                        "the matching Audit receipt records blocked; the controller stopped before an assessment was required or published, so absence is expected".into(),
                    );
                }
                _ => {
                    let failure = stop.map(|stop| {
                        format!(
                            "the recorded {} stop says `{}`",
                            format!("{:?}", stop.trigger),
                            stop.receipt_validation
                        )
                    });
                    details.insert(
                        "assessment.json".into(),
                        format!(
                            "no valid complete Audit receipt plus publication or controller transition establishes that an assessment was consumed{}; its absence is not inferred to be lost evidence",
                            failure.map(|failure| format!(" ({failure})")).unwrap_or_default()
                        ),
                    );
                }
            }
        }
    }
    if dispatched_by_this_version {
        // This controller version always retains the exact instruction it
        // supplied and the requirement projection the action read.
        required.insert("instruction.md".into());
        required.insert("binding-requirements.json".into());
        let exit_recorded = fs::read(action_dir.join("invocation.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value.get("exit").map(serde_json::Value::is_null))
            .is_some_and(|is_null| !is_null);
        if exit_recorded {
            // The local command adapter records its raw stderr and process exit
            // for every launch it made.
            required.insert("provider-exit.json".into());
            required.insert("provider-stderr.log".into());
        }
    }
    (required, details)
}

fn published_audit_attempts(store: &Store, effort: &Effort) -> Result<BTreeSet<String>> {
    let mut attempts = BTreeSet::new();
    for reference in store.list_artifacts(effort)? {
        if reference.kind != ArtifactKind::Audit {
            continue;
        }
        let envelope = store.load_envelope(effort, &reference)?;
        if let Some(attempt) = envelope.run_id.strip_prefix("build-audit-")
            && safe_action_reference(attempt)
        {
            attempts.insert(attempt.to_owned());
        }
    }
    Ok(attempts)
}

fn transitioned_final_audits(store: &Store, effort: &Effort) -> Result<BTreeSet<String>> {
    let mut actions = BTreeSet::new();
    for entry in store.read_journal(effort)? {
        if !matches!(
            entry.event.as_str(),
            "build_action_transition" | "build_unblock_started"
        ) {
            continue;
        }
        let from = &entry.details["from_action"];
        if from["kind"] == "final_audit"
            && let Some(action_id) = from["id"].as_str().filter(|id| safe_action_reference(id))
        {
            actions.insert(action_id.to_owned());
        }
    }
    Ok(actions)
}

/// Collect the files of one controller-owned record directory, checking
/// containment before every traversal and refusing to follow a symlinked
/// component rather than reading whatever it points at.
fn collect_record_files(
    root: &Path,
    directory: &Path,
    label: &str,
    depth: usize,
    members: &mut Vec<ExpectedMember>,
    reader: &dyn RecordDirectoryReader,
) {
    if !contained_in(root, directory) {
        members.push(
            ExpectedMember::new(label.to_owned(), directory.to_path_buf(), true)
                .refused(escape_detail(root, directory)),
        );
        return;
    }
    if depth == 0 {
        members.push(
            ExpectedMember::new(label.to_owned(), directory.to_path_buf(), false).refused(format!(
                "this record tree nests deeper than the {MAX_RECORD_DEPTH} levels the export walks, so it is disclosed rather than followed"
            )),
        );
        return;
    }
    let entries = match reader.read_directory(directory) {
        Ok(entries) => entries,
        Err(error) => {
            members.push(
                ExpectedMember::new(
                    format!("{label}/.inventory-error"),
                    directory.to_path_buf(),
                    true,
                )
                .refused(format!(
                    "cannot inventory evidence directory {}: {error}",
                    directory.display()
                )),
            );
            return;
        }
    };
    let mut paths = Vec::new();
    for (index, entry) in entries.into_iter().enumerate() {
        match entry {
            Ok(path) => paths.push(path),
            Err(error) => members.push(
                ExpectedMember::new(
                    format!("{label}/.inventory-error-{index}"),
                    directory.to_path_buf(),
                    true,
                )
                .refused(format!(
                    "cannot inventory an entry in evidence directory {}: {error}",
                    directory.display()
                )),
            ),
        }
    }
    paths.sort();
    for path in paths {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let member_name = format!("{label}/{name}");
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => members.push(
                ExpectedMember::new(member_name, path.clone(), false)
                    .refused("a symlink is never followed into the archive".to_owned()),
            ),
            Ok(metadata) if metadata.is_dir() => {
                let nested = format!("{label}/{name}");
                collect_record_files(root, &path, &nested, depth - 1, members, reader);
            }
            Ok(_) => members.push(ExpectedMember::new(member_name, path, false)),
            Err(_) => members.push(
                ExpectedMember::new(member_name, path.clone(), false)
                    .refused("this record could not be examined".to_owned()),
            ),
        }
    }
}

/// Build the inventory from durable source state.  It never walks a checkout,
/// a product tree, or a Discovery workspace.
fn inventory(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    state: Option<&BuildState>,
    plan: Option<&BuildPlan>,
    record_reader: &dyn RecordDirectoryReader,
) -> Result<(Vec<ExpectedMember>, Vec<String>)> {
    let mut members = Vec::new();
    let mut omissions = Vec::new();
    let effort_dir = store.effort_dir(effort);
    for name in EFFORT_MEMBERS {
        members.push(ExpectedMember::new(
            format!("effort/{name}"),
            effort_dir.join(name),
            true,
        ));
    }
    // A prepared Build is a durable commitment: once any of its files exists,
    // its plan and configuration are required evidence and a missing one is an
    // unexpected omission.  An effort whose Build was never prepared has no
    // required Build-level member at all, and any member that is present at
    // collection time must survive collection.
    let prepared = state.is_some()
        || plan.is_some()
        || ["plan.json", "config.toml"]
            .iter()
            .any(|name| build_dir.join(name).exists());
    let build_member = |name: &str| -> bool {
        match name {
            // State exists only once a Build has started; before that its
            // absence is the observed fact that nothing was dispatched.
            "state.json" => state.is_some(),
            _ => prepared || build_dir.join(name).exists(),
        }
    };
    for name in BUILD_MEMBERS {
        members.push(ExpectedMember::new(
            format!("build/{name}"),
            build_dir.join(name),
            build_member(name),
        ));
    }
    if let Some(plan) = plan {
        let required = build_member(&plan.detailed_plan);
        members.push(ExpectedMember::new(
            format!("build/{}", plan.detailed_plan),
            build_dir.join(&plan.detailed_plan),
            required,
        ));
    }
    for name in [
        "work.md",
        "review.md",
        "unblock.md",
        "once-over.md",
        "final-audit.md",
    ] {
        members.push(ExpectedMember::new(
            format!("build/instructions/{name}"),
            build_dir.join("instructions").join(name),
            false,
        ));
    }
    // Controller-owned record directories: small, immutable and walked whole,
    // with containment checked before each level.
    for (directory, label) in CONTROLLER_DIRECTORIES {
        let dir = build_dir.join(directory);
        match fs::symlink_metadata(&dir) {
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                members.push(
                    ExpectedMember::new(format!("{label}/.inventory-error"), dir.clone(), true)
                        .refused(format!(
                            "cannot inspect evidence directory {}: {error}",
                            dir.display()
                        )),
                );
                continue;
            }
            Ok(metadata) if !metadata.is_dir() && !metadata.file_type().is_symlink() => {
                members.push(
                    ExpectedMember::new((*label).to_owned(), dir.clone(), true).refused(format!(
                        "evidence path {} is not a directory and could not be inventoried",
                        dir.display()
                    )),
                );
                continue;
            }
            Ok(_) => {}
        }
        collect_record_files(
            &effort_dir,
            &dir,
            label,
            MAX_RECORD_DEPTH,
            &mut members,
            record_reader,
        );
    }
    // Every action directory that exists, plus every action the durable records
    // still reference: a referenced action with no directory is a detectable
    // omission rather than something the copied subset quietly defines.
    let actions_dir = build_dir.join("artifacts");
    let mut on_disk = BTreeSet::new();
    if actions_dir.is_dir() && !contained_in(&effort_dir, &actions_dir) {
        members.push(
            ExpectedMember::new("build/artifacts".to_owned(), actions_dir.clone(), false)
                .refused(escape_detail(&effort_dir, &actions_dir)),
        );
    } else if actions_dir.is_dir() {
        for entry in fs::read_dir(&actions_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                on_disk.insert(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    let referenced = referenced_action_ids(store, effort, state)?;
    let published_audits = published_audit_attempts(store, effort)?;
    let transitioned_audits = transitioned_final_audits(store, effort)?;
    if let Some(pending) = &referenced.pending
        && !on_disk.contains(pending)
    {
        // The Build prepared this action and never dispatched it, so no records
        // for it were ever written: that absence is observed, not a loss.
        members.push(
            ExpectedMember::new(
                format!("build/artifacts/{pending}/action.json"),
                actions_dir.join(pending).join("action.json"),
                false,
            )
            .with_detail(format!(
                "the Build prepared action {pending} and never dispatched it, so it has no records yet"
            )),
        );
    }
    for id in &referenced.required {
        if !on_disk.contains(id) {
            omissions.push(format!(
                "action {id} is referenced by the Build's durable state or journal but has no artifact directory; its inputs, results and transports are unavailable"
            ));
            members.push(
                ExpectedMember::new(
                    format!("build/artifacts/{id}/action.json"),
                    actions_dir.join(id).join("action.json"),
                    true,
                )
                .with_detail(
                    "a durable record references this action but its directory is missing",
                ),
            );
        }
    }
    for action in &on_disk {
        let dir = actions_dir.join(action);
        let (required, details) =
            action_obligations(&dir, state, &published_audits, &transitioned_audits);
        for name in ACTION_MEMBERS {
            let mut member = ExpectedMember::new(
                format!("build/artifacts/{action}/{name}"),
                dir.join(name),
                required.contains(*name),
            );
            if *name == "action.json" {
                member = member.with_detail(
                    "the action packet every controller version writes; its loss is lost evidence",
                );
            }
            if let Some(detail) = details.get(*name) {
                member = member.with_detail(detail.clone());
            } else if member.required {
                member = member.with_detail(match *name {
                    "transport.jsonl" => {
                        "this action recorded a completed provider transport, so its absence is lost transport evidence"
                    }
                    "result.json" => {
                        "this action's writer or recorded transition makes the receipt required evidence"
                    }
                    _ => "this controller version wrote this record for the action, so its absence is a loss",
                });
            }
            members.push(member);
        }
    }
    // Operator-supplied resolution evidence, wherever the operator kept it.  It
    // is referenced by an immutable durable record, so it is required evidence:
    // the archive includes it or the export reports that it is unavailable.
    for record in resolution_records(build_dir)? {
        for (index, evidence) in record.evidence.iter().enumerate() {
            let path = PathBuf::from(&evidence.path);
            members.push(
                ExpectedMember::new(
                    format!(
                        "resolutions/evidence/{}-{index}-{}",
                        record.resolution_id,
                        evidence.file_name()
                    ),
                    path,
                    true,
                )
                .with_expected_sha256(evidence.sha256.clone())
                .referenced()
                .with_detail(format!(
                    "evidence the operator supplied for resolution {}; it is referenced by that immutable record",
                    record.resolution_id
                )),
            );
        }
    }
    // Authority, lineage and every published bundle this effort owns.
    for reference in store.list_artifacts(effort)? {
        let dir = store.artifact_dir(effort, &reference.artifact_id)?;
        if !contained_in(&effort_dir, &dir) {
            members.push(
                ExpectedMember::new(
                    format!("artifacts/{}", reference.artifact_id),
                    dir.clone(),
                    true,
                )
                .refused(escape_detail(&effort_dir, &dir)),
            );
            continue;
        }
        let mut entries = fs::read_dir(&dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            members.push(ExpectedMember::new(
                format!("artifacts/{}/{name}", reference.artifact_id),
                path,
                name == "manifest.json",
            ));
        }
    }
    // Containment of the containing directory is established for every selected
    // member before anything is stat'd or read, so a symlinked directory
    // component is refused even when the leaf it would hold does not exist.
    for member in &mut members {
        if member.staging || member.external_reference || member.refusal.is_some() {
            continue;
        }
        let Some(parent) = member.path.parent() else {
            continue;
        };
        if parent.exists() && !contained_in(&effort_dir, parent) {
            member.refusal = Some(escape_detail(&effort_dir, parent));
        }
    }
    Ok((members, omissions))
}

/// One resolution record's serialized form, for the evidence it references.
#[derive(Clone, Debug, serde::Deserialize)]
struct ResolutionEvidenceView {
    resolution_id: String,
    evidence: Vec<ResolutionEvidenceRef>,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct ResolutionEvidenceRef {
    path: String,
    sha256: String,
}

impl ResolutionEvidenceRef {
    fn file_name(&self) -> String {
        Path::new(&self.path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "evidence".into())
    }
}

fn resolution_records(build_dir: &Path) -> Result<Vec<ResolutionEvidenceView>> {
    let dir = build_dir.join("resolutions");
    let mut records = Vec::new();
    if !dir.is_dir() {
        return Ok(records);
    }
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        if let Ok(record) = read_json::<ResolutionEvidenceView>(&path) {
            records.push(record);
        }
    }
    records.sort_by(|left, right| left.resolution_id.cmp(&right.resolution_id));
    Ok(records)
}

fn git_output(repo: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .with_context(|| format!("cannot run git {} in {}", args.join(" "), repo.display()))
}

fn git_lines(repo: &Path, args: &[&str]) -> Option<String> {
    let output = git_output(repo, args).ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Source and Git evidence: what an inspector needs to restore and read the
/// implementation on its own, without the product checkout.
fn git_evidence(
    store: &Store,
    effort: &Effort,
    state: Option<&BuildState>,
    staging: &Path,
    members: &mut Vec<ExpectedMember>,
    omissions: &mut Vec<String>,
) -> Result<()> {
    let project = store.project_for(effort)?;
    let repo = project.canonical_locator.clone();
    let head = git_lines(&repo, &["rev-parse", "HEAD"]);
    let status = git_lines(
        &repo,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )
    .unwrap_or_default();
    let implementation_commit = match state.and_then(|state| state.implementation.as_ref()) {
        Some(reference) => store
            .load_json::<orchestrate_contracts::Implementation>(
                effort,
                reference,
                "implementation.json",
            )
            .ok()
            .map(|(_, implementation)| implementation.target_commit),
        None => None,
    };
    let summary = serde_json::json!({
        "repository": repo,
        "head": head,
        "status": status,
        "build_start_commit": state.map(|state| state.frozen.build_start_commit.clone()),
        "discovery_baseline_commit": state.map(|state| state.frozen.discovery_baseline_commit.clone()),
        "implementation_commit": implementation_commit,
        "note": "the recorded commits, the exact HEAD observed at collection and the working-tree status; the bundle below carries the implementation's own history and needs no baseline, and the partial-work patch carries tracked changes that were never committed",
    });
    let summary_path = staging.join("git-evidence-checkout.json");
    fs::write(&summary_path, serde_json::to_vec_pretty(&summary)?)?;
    members.push(
        ExpectedMember::new("git-evidence/checkout.json".to_owned(), summary_path, true).staged(),
    );

    // Partial work: the tracked changes a stopped run left uncommitted, and the
    // names of untracked files.  Untracked contents are deliberately not
    // collected; the list is what tells an inspector they existed.
    let patch_path = staging.join("git-evidence-partial-work.patch");
    match git_output(&repo, &["diff", "HEAD", "--no-color", "--binary"]) {
        Ok(output) if output.status.success() => {
            fs::write(&patch_path, &output.stdout)?;
            members.push(
                ExpectedMember::new(
                    "git-evidence/partial-work.patch".to_owned(),
                    patch_path.clone(),
                    true,
                )
                .staged(),
            );
        }
        Ok(output) => omissions.push(format!(
            "the working-tree diff could not be collected ({}), so uncommitted tracked changes are not in the archive",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(error) => omissions.push(format!(
            "the working-tree diff could not be collected ({error:#}), so uncommitted tracked changes are not in the archive"
        )),
    }
    let untracked_path = staging.join("git-evidence-untracked-files.txt");
    let untracked = status
        .lines()
        .filter(|line| line.starts_with("??"))
        .map(|line| line.trim_start_matches("?? ").to_owned())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        &untracked_path,
        format!(
            "# untracked paths observed at collection; their contents are not collected\n{untracked}\n"
        ),
    )?;
    members.push(
        ExpectedMember::new(
            "git-evidence/untracked-files.txt".to_owned(),
            untracked_path,
            true,
        )
        .staged(),
    );

    let Some(state) = state else {
        // Nothing has been implemented yet, so there is no implementation
        // history to bundle: that absence is legitimate and disclosed on the
        // member itself rather than reported as a collection omission.
        members.push(
            ExpectedMember::new(
                "git-evidence/history.bundle".into(),
                staging.join("git-evidence-history.bundle"),
                false,
            )
            .with_detail(
                "no Build state exists yet, so there is no implementation history to bundle",
            )
            .staged(),
        );
        return Ok(());
    };
    if head.is_none() {
        members.push(
            ExpectedMember::new(
                "git-evidence/history.bundle".into(),
                staging.join("git-evidence-history.bundle"),
                false,
            )
            .with_detail(
                "the repository has no commit yet, so a stopped run before its first commit has no history to bundle; the checkout summary records that fact",
            )
            .staged(),
        );
        return Ok(());
    }
    // The bundle carries the whole history reachable from HEAD, so it needs no
    // prerequisite commit and restores into an empty repository.  A range such
    // as build-start..HEAD would require the excluded baseline and fail — or
    // produce nothing — for an unchanged-source re-verification.
    let bundle_path = staging.join("git-evidence-history.bundle");
    let bundle = Command::new("git")
        .args(["bundle", "create"])
        .arg(&bundle_path)
        .arg("HEAD")
        .current_dir(&repo)
        .output()
        .context("cannot run git bundle for export")?;
    if bundle.status.success() {
        members.push(
            ExpectedMember::new("git-evidence/history.bundle".to_owned(), bundle_path, true)
                .with_detail(format!(
                    "the full history reachable from {}, which the recorded build start {} belongs to; no prerequisite commit is needed to restore it",
                    head.as_deref().unwrap_or("HEAD"),
                    state.frozen.build_start_commit
                ))
                .staged(),
        );
    } else {
        let detail = String::from_utf8_lossy(&bundle.stderr).trim().to_owned();
        omissions.push(format!(
            "the implementation history could not be bundled ({detail}); the recorded commits remain in the exported state and checkout summary"
        ));
    }
    Ok(())
}

/// Remove this operation's own staging directory on the way out of any path,
/// including an early failure, so a broken collection cannot accumulate scratch
/// it created.  It is disarmed once the outcome reports the removal explicitly.
struct StagingGuard {
    path: PathBuf,
    armed: bool,
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// Own the scratch paths of exactly this export operation.  Nothing is created
/// over an existing file or directory: a colliding name belongs to someone else
/// and is reported instead.
struct OwnedRun {
    staging: PathBuf,
    partial: PathBuf,
    writer: ZipWriter,
}

fn run_nonce() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!(
        "{}-{nanos:x}-{:x}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn begin_owned_run(output: &Path, leftovers: &mut Vec<String>) -> Result<OwnedRun> {
    let stem = output
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "evidence".into());
    // A pre-existing file under a name an earlier export *would* have used is
    // disclosed and left alone; this operation never removes it.
    for legacy in [
        output.with_extension("partial"),
        output.with_extension("evidence-staging"),
    ] {
        if legacy.exists() {
            leftovers.push(format!(
                "{} already existed before this export and was left untouched; it is not this operation's file",
                legacy.display()
            ));
        }
    }
    for _ in 0..16 {
        let nonce = run_nonce();
        let staging = output.with_file_name(format!("{stem}.{nonce}.evidence-staging"));
        let partial = output.with_file_name(format!("{stem}.{nonce}.partial"));
        match fs::create_dir(&staging) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("cannot create export staging {}", staging.display())
                });
            }
        }
        match ZipWriter::create_new(&partial) {
            Ok(writer) => {
                return Ok(OwnedRun {
                    staging,
                    partial,
                    writer,
                });
            }
            Err(error) => {
                let _ = fs::remove_dir(&staging);
                match error
                    .downcast_ref::<std::io::Error>()
                    .map(std::io::Error::kind)
                {
                    Some(ErrorKind::AlreadyExists) => continue,
                    _ => {
                        return Err(error).with_context(|| {
                            format!("cannot create export archive {}", partial.display())
                        });
                    }
                }
            }
        }
    }
    bail!(
        "cannot create an exclusively owned export path beside {}; remove nothing automatically, but check for unrelated files under that name",
        output.display()
    )
}

pub fn export(store: &Store, effort: &Effort, output: &Path) -> Result<ExportOutcome> {
    export_with_record_reader(store, effort, output, &FilesystemRecordDirectoryReader)
}

pub(super) fn export_with_record_reader(
    store: &Store,
    effort: &Effort,
    output: &Path,
    record_reader: &dyn RecordDirectoryReader,
) -> Result<ExportOutcome> {
    let build_dir = store.phase_dir(effort, "build")?;
    let mut plan_error: Option<String> = None;
    let state_path = build_dir.join("state.json");
    let state: Option<BuildState> = state_path
        .is_file()
        .then(|| read_json(&state_path))
        .transpose()
        .with_context(|| format!("cannot read {}", state_path.display()))?;
    // A plan that exists but does not parse must not silently drop the detailed
    // plan from the inventory: that is disclosed as a collection omission.
    let plan: Option<BuildPlan> = if build_dir.join("plan.json").is_file() {
        match read_json(&build_dir.join("plan.json")) {
            Ok(plan) => Some(plan),
            Err(error) => {
                plan_error = Some(format!(
                    "build/plan.json exists but could not be read ({error:#}), so the detailed implementation plan it names could not be selected"
                ));
                None
            }
        }
    } else {
        None
    };
    // The archive and its staging directory must live outside the effort being
    // exported, so a collection can never be staged inside its own source.
    let effort_dir = fs::canonicalize(store.effort_dir(effort))?;
    let output_parent = output
        .parent()
        .filter(|parent| parent.exists())
        .map(fs::canonicalize)
        .transpose()?
        .unwrap_or_else(|| PathBuf::from("/"));
    ensure!(
        !output_parent.starts_with(&effort_dir) && !output.starts_with(&effort_dir),
        "refusing to export into the effort being exported ({})",
        effort_dir.display()
    );
    let mut leftovers = Vec::new();
    let OwnedRun {
        staging,
        partial,
        mut writer,
    } = begin_owned_run(output, &mut leftovers)?;
    // From here on the guard owns the staging directory, so a failure anywhere
    // removes only this operation's own scratch; the partial archive stays as
    // evidence of the attempted collection.
    let mut staging_guard = StagingGuard {
        path: staging.clone(),
        armed: true,
    };
    let mut expected = Vec::new();
    let mut omissions = Vec::new();
    let (members, notes) = inventory(
        store,
        effort,
        &build_dir,
        state.as_ref(),
        plan.as_ref(),
        record_reader,
    )?;
    expected.extend(members);
    omissions.extend(notes);
    if let Some(detail) = plan_error {
        omissions.push(detail);
    }
    {
        let (mut git_members, mut git_omissions) = (Vec::new(), Vec::new());
        git_evidence(
            store,
            effort,
            state.as_ref(),
            &staging,
            &mut git_members,
            &mut git_omissions,
        )?;
        expected.extend(git_members);
        omissions.append(&mut git_omissions);
    }

    let mut members = Vec::new();
    let mut copied = 0;
    let mut absent = 0;
    let mut failed = 0;
    let mut changed = 0;
    let mut digests = BTreeMap::new();
    for item in &expected {
        // A member name is generated from known names and checked again here so
        // no archive path can escape its own root.
        ensure!(
            safe_member_name(&item.name),
            "refusing unsafe archive member name {}",
            item.name
        );
        if let Some(refusal) = &item.refusal {
            failed += 1;
            members.push(MemberOutcome {
                name: item.name.clone(),
                source: item.path.to_string_lossy().into_owned(),
                class: MemberClass::Failed,
                required: item.required,
                sha256: None,
                bytes: 0,
                detail: Some(refusal.clone()),
            });
            continue;
        }
        let metadata = match fs::symlink_metadata(&item.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                let class = if item.required {
                    MemberClass::Failed
                } else {
                    MemberClass::Absent
                };
                match class {
                    MemberClass::Failed => failed += 1,
                    _ => absent += 1,
                }
                members.push(MemberOutcome {
                    name: item.name.clone(),
                    source: item.path.to_string_lossy().into_owned(),
                    class,
                    required: item.required,
                    sha256: None,
                    bytes: 0,
                    detail: Some(item.detail.clone().unwrap_or_else(|| match class {
                        MemberClass::Failed => {
                            "required evidence is missing at the source".to_owned()
                        }
                        _ if legacy_member(&item.name) => format!(
                            "{} is a record introduced by this controller version; it is absent because the action was interrupted or was written by an older controller. It is disclosed, not fabricated.",
                            item.name.rsplit('/').next().unwrap_or(&item.name)
                        ),
                        _ => "legitimately absent at the source".to_owned(),
                    })),
                });
                continue;
            }
            Err(error) => {
                failed += 1;
                members.push(MemberOutcome {
                    name: item.name.clone(),
                    source: item.path.to_string_lossy().into_owned(),
                    class: MemberClass::Failed,
                    required: item.required,
                    sha256: None,
                    bytes: 0,
                    detail: Some(format!("cannot stat: {error}")),
                });
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            failed += 1;
            members.push(MemberOutcome {
                name: item.name.clone(),
                source: item.path.to_string_lossy().into_owned(),
                class: MemberClass::Failed,
                required: item.required,
                sha256: None,
                bytes: 0,
                detail: Some("a symlink is never followed into the archive".into()),
            });
            continue;
        }
        // Containment is checked before the member is read, so a symlinked
        // directory component — an `evidence`, action or instruction directory
        // pointing elsewhere — can never expose files outside the effort even
        // though the leaf itself looks ordinary.
        if !item.staging && !item.external_reference && !contained_in(&effort_dir, &item.path) {
            failed += 1;
            members.push(MemberOutcome {
                name: item.name.clone(),
                source: item.path.to_string_lossy().into_owned(),
                class: MemberClass::Failed,
                required: item.required,
                sha256: None,
                bytes: 0,
                detail: Some(escape_detail(&effort_dir, &item.path)),
            });
            continue;
        }
        if !metadata.is_file() {
            failed += 1;
            members.push(MemberOutcome {
                name: item.name.clone(),
                source: item.path.to_string_lossy().into_owned(),
                class: MemberClass::Failed,
                required: item.required,
                sha256: None,
                bytes: 0,
                detail: Some("expected evidence is not a regular file".into()),
            });
            continue;
        }
        let before = metadata.modified().ok();
        before_read(&item.path);
        // One coherent read: the bytes that are hashed, stability-checked and
        // archived are the same bytes.  There is no second read for the archive
        // to disagree with, so a change during collection cannot slip between a
        // check and a later open.
        let bytes = match fs::read(&item.path) {
            Ok(bytes) => bytes,
            Err(error) => {
                failed += 1;
                members.push(MemberOutcome {
                    name: item.name.clone(),
                    source: item.path.to_string_lossy().into_owned(),
                    class: MemberClass::Failed,
                    required: item.required,
                    sha256: None,
                    bytes: 0,
                    detail: Some(format!("cannot read: {error}")),
                });
                continue;
            }
        };
        let after = fs::metadata(&item.path).ok();
        let collected_digest = digest_bytes(&bytes);
        // A member a durable record already hashed must still match that hash:
        // bytes that changed since the record was written are a change, not a
        // copy of the referenced evidence.
        let digest_mismatch = item
            .expected_sha256
            .as_deref()
            .is_some_and(|expected| expected != collected_digest);
        let changed_during = changed_during_collection(before, after.as_ref(), bytes.len() as u64)
            || digest_mismatch;
        let written = if changed_during {
            changed += 1;
            None
        } else {
            Some(writer.add_bytes(&item.name, &bytes)?)
        };
        if let Some(written) = written {
            copied += 1;
            digests.insert(item.name.clone(), written.sha256.clone());
        }
        members.push(MemberOutcome {
            name: item.name.clone(),
            source: item.path.to_string_lossy().into_owned(),
            class: if changed_during {
                MemberClass::Changed
            } else {
                MemberClass::Copied
            },
            required: item.required,
            sha256: written.map(|member| member.sha256.clone()),
            bytes: bytes.len() as u64,
            detail: changed_during
                .then(|| {
                    if digest_mismatch {
                        "this referenced evidence no longer matches the digest the durable record published for it"
                            .to_owned()
                    } else {
                        "the source changed while it was being collected".to_owned()
                    }
                })
                .or_else(|| item.detail.clone()),
        });
    }
    let expected_names = members
        .iter()
        .filter(|outcome| outcome.class != MemberClass::Absent)
        .map(|outcome| outcome.name.clone())
        .collect::<Vec<_>>();
    writer.finish()?;

    // Verify membership and hashes by reading the partial archive back before it
    // can become a complete result.  A CRC check alone proves nothing about
    // whether every selected member arrived, and the recorded hash is the hash
    // of the exact bytes that were collected.
    let archived = central_directory(&partial)?;
    let by_name = archived
        .iter()
        .map(|member| (member.name.clone(), member))
        .collect::<BTreeMap<_, _>>();
    let mut missing = Vec::new();
    let mut mismatched = Vec::new();
    for name in &expected_names {
        match by_name.get(name) {
            None => missing.push(name.clone()),
            Some(member) => {
                let (_, digest) = read_member(&partial, member)?;
                if digests.get(name) != Some(&digest) {
                    mismatched.push(name.clone());
                }
            }
        }
    }
    let unexpected = archived
        .iter()
        .map(|member| member.name.clone())
        .filter(|name| !expected_names.contains(name))
        .collect::<Vec<_>>();
    let verification_complete = missing.is_empty() && mismatched.is_empty();
    let complete = verification_complete && failed == 0 && changed == 0 && omissions.is_empty();
    let archive = if complete {
        // Only a verified archive is promoted from its partial name.
        fs::rename(&partial, output)
            .with_context(|| format!("cannot promote {}", output.display()))?;
        Some(output.to_path_buf())
    } else {
        None
    };
    // Only the staging directory this operation created is removed, and a
    // failure to remove it is reported rather than swallowed.  The partial
    // archive stays: it is this operation's own evidence of an incomplete
    // collection.
    let staging_left = fs::remove_dir_all(&staging)
        .err()
        .map(|error| error.to_string());
    staging_guard.armed = false;
    Ok(ExportOutcome {
        effort: effort.id.clone(),
        archive,
        partial,
        leftovers,
        export_status: if complete {
            "complete"
        } else if verification_complete {
            "incomplete"
        } else {
            "failed"
        },
        complete,
        expected: expected.len(),
        copied,
        absent,
        failed,
        changed,
        members,
        excluded_classes: EXCLUDED_CLASSES
            .iter()
            .map(
                |(class, disclosure)| serde_json::json!({"class": class, "disclosure": disclosure}),
            )
            .collect(),
        omissions,
        verification: serde_json::json!({
            "members_expected": expected_names.len(),
            "members_present": archived.len(),
            "missing": missing,
            "hash_mismatches": mismatched,
            "unexpected_members": unexpected,
            "staging_removal_error": staging_left,
            "note": "promotion requires every selected member to be present with its recorded hash",
        }),
        note: "collection outcome is reported separately from the Build's semantic outcome, and export neither dispatches nor interprets anything",
    })
}

/// Whether a member changed while it was being collected: a different
/// modification time, or a different length than the bytes that were read.
pub(crate) fn changed_during_collection(
    before: Option<std::time::SystemTime>,
    after: Option<&fs::Metadata>,
    read_bytes: u64,
) -> bool {
    let Some(after) = after else {
        // The member disappeared after it was read: it certainly did not stay
        // stable through collection.
        return true;
    };
    after.modified().ok() != before || after.len() != read_bytes
}

#[cfg(not(test))]
fn before_read(_path: &Path) {}

#[cfg(test)]
fn before_read(path: &Path) {
    collection_hook::before_read(path);
}

/// Test-only seam at the real collection boundary.  A member's bytes are taken
/// between the pre-read stat and the read that is archived, so a fixture can
/// prove that a change in exactly that window is detected rather than assumed.
#[cfg(test)]
pub(crate) mod collection_hook {
    use std::cell::RefCell;
    use std::path::Path;

    type Hook = Box<dyn FnMut(&Path)>;

    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    pub(crate) fn set(hook: Option<Hook>) {
        HOOK.with(|slot| *slot.borrow_mut() = hook);
    }

    pub(crate) fn before_read(path: &Path) {
        HOOK.with(|slot| {
            if let Some(hook) = slot.borrow_mut().as_mut() {
                hook(path);
            }
        });
    }
}
