//! Shared mechanical infrastructure: external storage, Git snapshots, immutable bundles, and journals.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail, ensure};
use orchestrate_contracts::{
    ArtifactKind, ArtifactRef, DiscoveryRun, Envelope, PayloadDigest, Provenance, RequestKind,
    STORE_FORMAT_VERSION, artifact_ref, decode, digest_bytes, encode, safe_relative_path,
    validate_envelope,
};
use serde::{Deserialize, Serialize};

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Project {
    pub id: String,
    pub display_name: String,
    pub canonical_locator: PathBuf,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContextRevision {
    pub id: String,
    pub request_kind: RequestKind,
    pub request: String,
    pub constraints: Vec<String>,
    pub digest: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Cohort {
    pub id: String,
    pub project_id: String,
    pub effort_id: String,
    pub context_id: String,
    pub baseline_commit: String,
    pub baseline_tree: String,
    #[serde(default = "default_slots")]
    pub slots: Vec<String>,
    #[serde(default = "default_quorum")]
    pub quorum: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Effort {
    pub id: String,
    pub slug: String,
    pub project_id: String,
    pub context: ContextRevision,
    pub cohort: Cohort,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoreMarker {
    format_version: u32,
}
#[derive(Clone, Debug, Serialize)]
struct ContextFingerprint<'a> {
    request_kind: &'a RequestKind,
    request: &'a str,
    constraints: &'a [String],
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct GitIdentity {
    commit: String,
    tree: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct SourceManifest {
    commit: String,
    tree: String,
    files: BTreeMap<String, String>,
}
#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub commit: String,
    pub tree: String,
    pub path: PathBuf,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JournalEvent {
    pub event: String,
    pub ts_ms: u128,
    pub effort_id: String,
    pub cohort_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub details: serde_json::Value,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LineageNode {
    pub artifact: ArtifactRef,
    pub outcome: String,
    pub parents: Vec<ArtifactRef>,
}

impl Store {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        ensure!(
            root.is_absolute(),
            "orchestration root must be an absolute path"
        );
        fs::create_dir_all(root)?;
        let root = fs::canonicalize(root)?;
        let marker = root.join("store.json");
        if marker.exists() {
            let stored: StoreMarker = read_json(&marker)?;
            ensure!(
                stored.format_version == STORE_FORMAT_VERSION,
                "unsupported orchestration store format {}; expected {}",
                stored.format_version,
                STORE_FORMAT_VERSION
            );
        } else if fs::read_dir(&root)?.next().is_some() {
            bail!(
                "legacy orchestration store at {}; this version does not migrate prior stores",
                root.display()
            );
        } else {
            write_json_atomic(
                &marker,
                &StoreMarker {
                    format_version: STORE_FORMAT_VERSION,
                },
            )?;
        }
        Ok(Self { root })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn init_effort(
        &self,
        repo: &Path,
        slug: &str,
        request_kind: RequestKind,
        request: String,
        constraints: Vec<String>,
        slots: Vec<String>,
        quorum: usize,
    ) -> Result<Effort> {
        validate_label(slug)?;
        validate_slots_and_quorum(&slots, quorum)?;
        let canonical_repo = fs::canonicalize(repo)
            .with_context(|| format!("cannot canonicalize repository {}", repo.display()))?;
        ensure!(
            canonical_repo.join(".git").exists(),
            "target is not a supported Git repository"
        );
        let baseline = capture_git_identity(&canonical_repo)?;
        let project_id = short_digest(canonical_repo.to_string_lossy().as_bytes());
        let project = Project {
            id: project_id.clone(),
            display_name: canonical_repo
                .file_name()
                .and_then(|p| p.to_str())
                .unwrap_or("project")
                .to_owned(),
            canonical_locator: canonical_repo.clone(),
        };
        let context_bytes = encode(&ContextFingerprint {
            request_kind: &request_kind,
            request: &request,
            constraints: &constraints,
        })?;
        let context_digest = digest_bytes(&context_bytes);
        let context = ContextRevision {
            id: format!("ctx-{}", &context_digest[..16]),
            request_kind,
            request,
            constraints,
            digest: context_digest,
        };
        let effort_id = format!(
            "{}-{}",
            slugify(slug),
            &short_digest(format!("{}:{}", project_id, context.id).as_bytes())[..10]
        );
        let cohort = Cohort {
            id: format!("cohort-{}", unique_id()),
            project_id: project_id.clone(),
            effort_id: effort_id.clone(),
            context_id: context.id.clone(),
            baseline_commit: baseline.commit,
            baseline_tree: baseline.tree,
            slots,
            quorum,
        };
        let effort = Effort {
            id: effort_id,
            slug: slug.into(),
            project_id,
            context,
            cohort,
        };
        let dir = self.effort_dir(&effort.project_id, &effort.id);
        if dir.exists() {
            ensure!(
                dir.join("effort.json").is_file(),
                "effort directory already exists but is malformed: {}",
                dir.display()
            );
            let existing: Effort = read_json(&dir.join("effort.json"))?;
            ensure!(
                existing.slug == effort.slug
                    && existing.project_id == effort.project_id
                    && existing.context.request_kind == effort.context.request_kind
                    && existing.context.request == effort.context.request
                    && existing.context.constraints == effort.context.constraints
                    && existing.cohort.slots == effort.cohort.slots
                    && existing.cohort.quorum == effort.cohort.quorum,
                "effort already exists with a different identity: {}; choose a new effort slug or reuse the same prepared request unchanged",
                effort.id
            );
            return Ok(existing);
        }
        fs::create_dir_all(&dir)?;
        write_json_atomic(&dir.join("project.json"), &project)?;
        write_json_atomic(&dir.join("effort.json"), &effort)?;
        write_bytes_sync(&dir.join("request.md"), effort.context.request.as_bytes())?;
        self.materialize_snapshot(&canonical_repo, &effort.cohort.baseline_commit)?;
        self.append_journal(&effort, "cohort_created", None, serde_json::json!({"commit": effort.cohort.baseline_commit, "tree": effort.cohort.baseline_tree}))?;
        Ok(effort)
    }
    pub fn load_effort(&self, effort_id: &str) -> Result<Effort> {
        validate_id(effort_id)?;
        let found = self.find_effort_dirs(effort_id)?;
        match found.as_slice() {
            [dir] => read_json(&dir.join("effort.json")),
            [] => bail!("unknown effort {effort_id}"),
            _ => bail!("ambiguous effort {effort_id}"),
        }
    }
    pub fn project_for(&self, effort: &Effort) -> Result<Project> {
        read_json(
            &self
                .effort_dir(&effort.project_id, &effort.id)
                .join("project.json"),
        )
    }
    pub fn effort_dir(&self, project_id: &str, effort_id: &str) -> PathBuf {
        self.root
            .join("projects")
            .join(project_id)
            .join("efforts")
            .join(effort_id)
    }
    pub fn phase_dir(&self, effort: &Effort, phase: &str) -> Result<PathBuf> {
        validate_label(phase)?;
        let path = self.effort_dir(&effort.project_id, &effort.id).join(phase);
        fs::create_dir_all(&path)?;
        Ok(path)
    }
    pub fn append_journal(
        &self,
        effort: &Effort,
        event: &str,
        run_id: Option<&str>,
        details: serde_json::Value,
    ) -> Result<()> {
        let path = self
            .effort_dir(&effort.project_id, &effort.id)
            .join("journal.jsonl");
        let record = JournalEvent {
            event: event.into(),
            ts_ms: now_ms(),
            effort_id: effort.id.clone(),
            cohort_id: effort.cohort.id.clone(),
            run_id: run_id.map(str::to_owned),
            details,
        };
        let mut bytes = serde_json::to_vec(&record)?;
        bytes.push(b'\n');
        with_journal_lock(&path, || {
            let mut file = OpenOptions::new().append(true).create(true).open(&path)?;
            file.write_all(&bytes)?;
            file.sync_data()?;
            Ok(())
        })
    }
    pub fn read_journal(&self, effort: &Effort) -> Result<Vec<JournalEvent>> {
        let path = self
            .effort_dir(&effort.project_id, &effort.id)
            .join("journal.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        fs::read_to_string(path)?
            .lines()
            .map(|line| decode(line.as_bytes()))
            .collect()
    }
    pub fn materialize_snapshot(&self, repo: &Path, commit: &str) -> Result<Snapshot> {
        let identity = capture_git_identity_at(repo, commit)?;
        let destination = self.root.join("snapshots").join(&identity.commit);
        if destination.exists() {
            return Ok(Snapshot {
                commit: identity.commit,
                tree: identity.tree,
                path: destination,
            });
        }
        let staging = self
            .root
            .join(".staging")
            .join(format!("snapshot-{}", unique_id()));
        fs::create_dir_all(&staging)?;
        let source = staging.join("source");
        fs::create_dir(&source)?;
        let output = Command::new("git")
            .args(["archive", "--format=tar", &identity.commit])
            .current_dir(repo)
            .output()?;
        ensure!(
            output.status.success(),
            "git archive failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        tar::Archive::new(std::io::Cursor::new(output.stdout))
            .unpack(&source)
            .context("could not unpack committed snapshot")?;
        write_json_atomic(
            &staging.join("source-manifest.json"),
            &SourceManifest {
                commit: identity.commit.clone(),
                tree: identity.tree.clone(),
                files: git_file_inventory(repo, &identity.commit)?,
            },
        )?;
        fs::create_dir_all(
            destination
                .parent()
                .context("snapshot root has no parent")?,
        )?;
        fs::rename(&staging, &destination).or_else(|error| {
            if destination.exists() {
                Ok(())
            } else {
                Err(error)
            }
        })?;
        Ok(Snapshot {
            commit: identity.commit,
            tree: identity.tree,
            path: destination,
        })
    }
    pub fn discovery_workspace(&self, effort: &Effort, run: &DiscoveryRun) -> Result<PathBuf> {
        validate_id(&run.run_id)?;
        ensure!(run.phase == "discovery", "run phase must be discovery");
        ensure!(
            effort.cohort.slots.iter().any(|slot| slot == &run.slot),
            "unknown discovery slot {}",
            run.slot
        );
        ensure!(
            run.effort_id == effort.id
                && run.cohort_id == effort.cohort.id
                && run.context_id == effort.context.id
                && run.request_kind == effort.context.request_kind
                && run.baseline_commit == effort.cohort.baseline_commit
                && run.baseline_tree == effort.cohort.baseline_tree,
            "Discovery run belongs to another effort, cohort, context, or baseline"
        );
        let root = self.phase_dir(effort, "discovery")?.join(&run.run_id);
        ensure!(
            !root.exists(),
            "Discovery run already exists: {}",
            run.run_id
        );
        let source = root.join("source");
        let project = self.project_for(effort)?;
        fs::create_dir_all(&root)?;
        let cloned = Command::new("git")
            .args(["clone", "--shared", "--no-checkout"])
            .arg(&project.canonical_locator)
            .arg(&source)
            .output()?;
        ensure!(
            cloned.status.success(),
            "git clone failed: {}",
            String::from_utf8_lossy(&cloned.stderr)
        );
        let checkout = Command::new("git")
            .args(["checkout", "--detach", &run.baseline_commit])
            .current_dir(&source)
            .output()?;
        ensure!(
            checkout.status.success(),
            "git checkout failed: {}",
            String::from_utf8_lossy(&checkout.stderr)
        );
        ensure!(
            git(&source, ["rev-parse", "HEAD"])? == run.baseline_commit,
            "Discovery source checkout does not match cohort baseline"
        );
        ensure!(
            git(&source, ["status", "--porcelain"])?.is_empty(),
            "Discovery source checkout is not clean"
        );
        write_json_atomic(&root.join("run.json"), run)?;
        write_bytes_sync(&root.join("request.md"), effort.context.request.as_bytes())?;
        write_json_atomic(
            &root.join("context.json"),
            &serde_json::json!({"cohort_id": effort.cohort.id, "context_id": effort.context.id, "request_kind": effort.context.request_kind, "request": effort.context.request, "constraints": effort.context.constraints, "slot": run.slot, "baseline_commit": effort.cohort.baseline_commit, "baseline_tree": effort.cohort.baseline_tree}),
        )?;
        write_bytes_sync(
            &root.join("technical-spec.md"),
            TECHNICAL_SPEC_TEMPLATE.as_bytes(),
        )?;
        fs::create_dir_all(root.join("graph"))?;
        self.append_journal(
            effort,
            "discovery_workspace_prepared",
            Some(&run.run_id),
            serde_json::json!({"slot":run.slot}),
        )?;
        Ok(root)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn publish_bundle(
        &self,
        effort: &Effort,
        phase: &str,
        kind: ArtifactKind,
        run_id: String,
        outcome: String,
        parents: Vec<ArtifactRef>,
        provenance: Provenance,
        files: BTreeMap<String, Vec<u8>>,
    ) -> Result<ArtifactRef> {
        ensure!(!files.is_empty(), "artifact bundle cannot be empty");
        for path in files.keys() {
            ensure!(safe_relative_path(path), "unsafe artifact path {path}");
        }
        let phase_dir = self.phase_dir(effort, phase)?;
        let artifact_id = format!(
            "{}-{}",
            phase,
            short_digest(format!("{phase}:{run_id}").as_bytes())
        );
        let final_dir = phase_dir.join(&artifact_id);
        if final_dir.exists() {
            return self.recover_bundle(
                effort,
                &final_dir,
                kind,
                &artifact_id,
                &run_id,
                &outcome,
                &parents,
                &provenance,
                &files,
            );
        }
        let stage = self
            .root
            .join(".staging")
            .join(format!("{}-{}", artifact_id, unique_id()));
        fs::create_dir_all(&stage)?;
        let mut payloads = Vec::new();
        for (path, bytes) in &files {
            write_bytes_sync(&stage.join(path), bytes)?;
            payloads.push(PayloadDigest {
                path: path.clone(),
                sha256: digest_bytes(bytes),
                bytes: bytes.len() as u64,
            });
        }
        let envelope = Envelope {
            schema_version: orchestrate_contracts::SCHEMA_VERSION,
            kind: kind.clone(),
            artifact_id: artifact_id.clone(),
            run_id: run_id.clone(),
            project_id: effort.project_id.clone(),
            effort_id: effort.id.clone(),
            cohort_id: Some(effort.cohort.id.clone()),
            outcome: outcome.clone(),
            created_at_ms: now_ms(),
            finalized_at_ms: now_ms(),
            producer_version: orchestrate_contracts::PRODUCT_VERSION.into(),
            parents: parents.clone(),
            payloads,
            provenance: provenance.clone(),
        };
        validate_envelope(&envelope)?;
        let manifest = encode(&envelope)?;
        write_bytes_sync(&stage.join("manifest.json"), &manifest)?;
        sync_dir(&stage)?;
        if let Err(error) = fs::rename(&stage, &final_dir) {
            if final_dir.exists() {
                return self.recover_bundle(
                    effort,
                    &final_dir,
                    kind,
                    &artifact_id,
                    &run_id,
                    &outcome,
                    &parents,
                    &provenance,
                    &files,
                );
            }
            return Err(error).with_context(|| format!("cannot commit artifact {artifact_id}"));
        }
        sync_dir(&phase_dir)?;
        Ok(artifact_ref(&envelope, &manifest))
    }
    #[allow(clippy::too_many_arguments)]
    fn recover_bundle(
        &self,
        effort: &Effort,
        dir: &Path,
        kind: ArtifactKind,
        artifact_id: &str,
        run_id: &str,
        outcome: &str,
        parents: &[ArtifactRef],
        provenance: &Provenance,
        files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<ArtifactRef> {
        let manifest = fs::read(dir.join("manifest.json"))?;
        let envelope: Envelope = decode(&manifest)?;
        validate_envelope(&envelope)?;
        ensure!(
            envelope.kind == kind
                && envelope.artifact_id == artifact_id
                && envelope.run_id == run_id
                && envelope.project_id == effort.project_id
                && envelope.effort_id == effort.id
                && envelope.outcome == outcome
                && envelope.parents == parents
                && envelope.provenance == *provenance,
            "operation identity conflicts with an existing publication"
        );
        ensure!(
            envelope.payloads.len() == files.len(),
            "operation identity conflicts with existing bundle content"
        );
        for payload in &envelope.payloads {
            let expected = files
                .get(&payload.path)
                .context("existing bundle has unexpected path")?;
            ensure!(
                payload.bytes == expected.len() as u64 && payload.sha256 == digest_bytes(expected),
                "operation identity conflicts with existing bundle content"
            );
        }
        self.load_bundle(effort, &artifact_ref(&envelope, &manifest))?;
        Ok(artifact_ref(&envelope, &manifest))
    }
    pub fn load_bundle(
        &self,
        effort: &Effort,
        reference: &ArtifactRef,
    ) -> Result<(Envelope, BTreeMap<String, Vec<u8>>)> {
        let dir = self.find_artifact_dir(effort, &reference.artifact_id)?;
        let manifest = fs::read(dir.join("manifest.json"))?;
        ensure!(
            digest_bytes(&manifest) == reference.digest,
            "artifact manifest digest mismatch"
        );
        let envelope: Envelope = decode(&manifest)?;
        validate_envelope(&envelope)?;
        ensure!(
            envelope.kind == reference.kind
                && envelope.project_id == effort.project_id
                && envelope.effort_id == effort.id,
            "artifact reference mismatch or cross-effort reference"
        );
        let mut files = BTreeMap::new();
        for payload in &envelope.payloads {
            let bytes = fs::read(dir.join(&payload.path))?;
            ensure!(
                bytes.len() as u64 == payload.bytes && digest_bytes(&bytes) == payload.sha256,
                "artifact payload integrity failure"
            );
            files.insert(payload.path.clone(), bytes);
        }
        Ok((envelope, files))
    }
    pub fn load_json<T: for<'a> Deserialize<'a>>(
        &self,
        effort: &Effort,
        reference: &ArtifactRef,
        path: &str,
    ) -> Result<(Envelope, T)> {
        let (envelope, files) = self.load_bundle(effort, reference)?;
        Ok((
            envelope,
            decode(
                files
                    .get(path)
                    .context("artifact lacks requested JSON payload")?,
            )?,
        ))
    }
    pub fn find_artifact_ref(&self, effort: &Effort, artifact_id: &str) -> Result<ArtifactRef> {
        let dir = self.find_artifact_dir(effort, artifact_id)?;
        let bytes = fs::read(dir.join("manifest.json"))?;
        let envelope: Envelope = decode(&bytes)?;
        Ok(artifact_ref(&envelope, &bytes))
    }
    pub fn load_envelope(&self, effort: &Effort, reference: &ArtifactRef) -> Result<Envelope> {
        Ok(self.load_bundle(effort, reference)?.0)
    }
    pub fn list_artifacts(&self, effort: &Effort) -> Result<Vec<ArtifactRef>> {
        let mut out = Vec::new();
        for phase in ["discovery", "consensus", "agreement", "build", "audit"] {
            let dir = self.effort_dir(&effort.project_id, &effort.id).join(phase);
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(dir)? {
                let manifest = entry?.path().join("manifest.json");
                if manifest.exists() {
                    let bytes = fs::read(&manifest)?;
                    if let Ok(e) = decode::<Envelope>(&bytes) {
                        out.push(artifact_ref(&e, &bytes));
                    }
                }
            }
        }
        out.sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
        Ok(out)
    }
    pub fn reconstruct_lineage(
        &self,
        effort: &Effort,
        target: &ArtifactRef,
    ) -> Result<Vec<LineageNode>> {
        let mut nodes = Vec::new();
        let mut visiting = std::collections::BTreeSet::new();
        self.lineage_inner(effort, target, &mut visiting, &mut nodes)?;
        Ok(nodes)
    }
    fn lineage_inner(
        &self,
        effort: &Effort,
        target: &ArtifactRef,
        visiting: &mut std::collections::BTreeSet<(ArtifactKind, String, String)>,
        nodes: &mut Vec<LineageNode>,
    ) -> Result<()> {
        let key = (
            target.kind.clone(),
            target.artifact_id.clone(),
            target.digest.clone(),
        );
        ensure!(
            visiting.insert(key.clone()),
            "artifact lineage contains a cycle"
        );
        let e = self.load_envelope(effort, target)?;
        nodes.push(LineageNode {
            artifact: target.clone(),
            outcome: e.outcome.clone(),
            parents: e.parents.clone(),
        });
        for parent in &e.parents {
            self.lineage_inner(effort, parent, visiting, nodes)?;
        }
        visiting.remove(&key);
        Ok(())
    }
    fn find_effort_dirs(&self, effort_id: &str) -> Result<Vec<PathBuf>> {
        let projects = self.root.join("projects");
        if !projects.exists() {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        for p in fs::read_dir(projects)? {
            let path = p?.path().join("efforts").join(effort_id);
            if path.join("effort.json").exists() {
                found.push(path);
            }
        }
        Ok(found)
    }
    fn find_artifact_dir(&self, effort: &Effort, artifact_id: &str) -> Result<PathBuf> {
        validate_id(artifact_id)?;
        let mut found = Vec::new();
        for phase in ["discovery", "consensus", "agreement", "build", "audit"] {
            let path = self
                .effort_dir(&effort.project_id, &effort.id)
                .join(phase)
                .join(artifact_id);
            if path.join("manifest.json").exists() {
                found.push(path);
            }
        }
        match found.as_slice() {
            [path] => Ok(path.clone()),
            [] => bail!("unknown artifact {artifact_id}"),
            _ => bail!("ambiguous artifact {artifact_id}"),
        }
    }
}

pub const TECHNICAL_SPEC_TEMPLATE: &str = "# Technical specification\n\n";
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}
fn capture_git_identity(repo: &Path) -> Result<GitIdentity> {
    capture_git_identity_at(repo, "HEAD")
}
fn capture_git_identity_at(repo: &Path, revision: &str) -> Result<GitIdentity> {
    Ok(GitIdentity {
        commit: git(
            repo,
            ["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
        )?,
        tree: git(
            repo,
            [
                "rev-parse",
                "--verify",
                &format!(
                    "{}^{{tree}}",
                    git(
                        repo,
                        ["rev-parse", "--verify", &format!("{revision}^{{commit}}")]
                    )?
                ),
            ],
        )?,
    })
}
fn git_file_inventory(repo: &Path, commit: &str) -> Result<BTreeMap<String, String>> {
    let listing = git(repo, ["ls-tree", "-r", "-z", commit])?;
    let mut inventory = BTreeMap::new();
    for entry in listing.split('\0').filter(|p| !p.is_empty()) {
        let (meta, path) = entry.split_once('\t').context("invalid git tree entry")?;
        let fields: Vec<_> = meta.split_whitespace().collect();
        ensure!(
            fields.len() == 3 && fields[1] != "commit",
            "unsupported git tree entry"
        );
        inventory.insert(path.into(), fields[2].into());
    }
    Ok(inventory)
}
fn git<const N: usize>(repo: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(repo).output()?;
    ensure!(
        output.status.success(),
        "git command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn unique_id() -> String {
    format!(
        "{}-{}",
        now_ms(),
        ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}
fn short_digest(bytes: &[u8]) -> String {
    digest_bytes(bytes)[..16].into()
}
fn slugify(v: &str) -> String {
    v.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .into()
}
fn validate_label(v: &str) -> Result<()> {
    ensure!(
        !v.is_empty()
            && v.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')),
        "invalid label {v}"
    );
    Ok(())
}
fn validate_id(v: &str) -> Result<()> {
    validate_label(v)
}
fn default_slots() -> Vec<String> {
    vec!["a".into(), "b".into(), "c".into()]
}
fn default_quorum() -> usize {
    2
}
fn validate_slots_and_quorum(slots: &[String], quorum: usize) -> Result<()> {
    ensure!(
        slots.len() >= 2,
        "a cohort requires at least two Discovery slots"
    );
    let mut seen = std::collections::HashSet::new();
    for slot in slots {
        validate_label(slot)?;
        ensure!(seen.insert(slot), "duplicate Discovery slot {slot}");
    }
    ensure!(
        (2..=slots.len()).contains(&quorum),
        "quorum must be between 2 and the number of Discovery slots ({})",
        slots.len()
    );
    Ok(())
}
fn with_journal_lock<T>(path: &Path, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let lock_path = path.with_extension("jsonl.lock");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_lock) => {
                let result = f();
                let _ = fs::remove_file(&lock_path);
                return result;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if Instant::now() >= deadline {
                    bail!("timed out waiting for journal lock {}", lock_path.display());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error.into()),
        }
    }
}
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_bytes_sync(path, &encode(value)?)
}
fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T> {
    decode(&fs::read(path).with_context(|| format!("cannot read {}", path.display()))?)
}
pub fn write_bytes_sync(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("path has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|v| v.to_str()).unwrap_or("file"),
        unique_id()
    ));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(temp, path)?;
    sync_dir(parent)
}
fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?.sync_all().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::Write;

    #[test]
    fn concurrent_journal_appends_remain_line_framed() {
        let dir = std::env::temp_dir().join(format!("journal-{}", unique_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.jsonl");
        let handles: Vec<_> = (0..16)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for j in 0..16 {
                        super::with_journal_lock(&path, || {
                            let mut file = OpenOptions::new()
                                .append(true)
                                .create(true)
                                .open(&path)
                                .unwrap();
                            let line = format!("{{\"i\":{i},\"j\":{j}}}\n");
                            file.write_all(line.as_bytes()).unwrap();
                            Ok(())
                        })
                        .unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 16 * 16);
        for line in text.lines() {
            serde_json::from_str::<serde_json::Value>(line).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
