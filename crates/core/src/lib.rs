//! Shared mechanical infrastructure: external storage, Git snapshots, immutable bundles, and journals.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
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
    pub storage_name: String,
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
pub struct Effort {
    pub id: String,
    pub slug: String,
    pub project_id: String,
    pub context: ContextRevision,
    pub baseline_commit: String,
    pub baseline_tree: String,
    pub project_storage_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepared_source: Option<PreparedSource>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PreparedSource {
    pub absolute_path: PathBuf,
    pub sha256: String,
    pub initialized_at_ms: u128,
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
            validate_marker(&marker)?;
        } else {
            let has_legacy_content =
                fs::read_dir(&root)?
                    .filter_map(|entry| entry.ok())
                    .any(|entry| {
                        let name = entry.file_name();
                        let name = name.to_string_lossy();
                        name != "store.json"
                            && name != ".staging"
                            && !name.starts_with(".store.json.tmp-")
                    });
            if has_legacy_content {
                bail!(
                    "legacy orchestration store at {}; this version does not migrate prior stores",
                    root.display()
                );
            }
            // Multiple model windows may create a fresh store simultaneously. All contenders
            // write the same marker, so temporary marker files and a marker committed by a
            // contender that finished since the scan above are both ignored rather than
            // mistaken for a legacy store.
            if marker.exists() {
                validate_marker(&marker)?;
            } else {
                write_json_atomic(
                    &marker,
                    &StoreMarker {
                        format_version: STORE_FORMAT_VERSION,
                    },
                )?;
            }
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
    ) -> Result<Effort> {
        self.init_effort_with_source(repo, slug, request_kind, request, constraints, None)
    }
    pub fn init_effort_with_source(
        &self,
        repo: &Path,
        slug: &str,
        request_kind: RequestKind,
        request: String,
        constraints: Vec<String>,
        prepared_source: Option<PreparedSource>,
    ) -> Result<Effort> {
        validate_label(slug)?;
        let canonical_repo = fs::canonicalize(repo)
            .with_context(|| format!("cannot canonicalize repository {}", repo.display()))?;
        ensure!(
            canonical_repo.join(".git").exists(),
            "target is not a supported Git repository"
        );
        let project_id = short_digest(canonical_repo.to_string_lossy().as_bytes());
        let display_name = canonical_repo
            .file_name()
            .and_then(|p| p.to_str())
            .unwrap_or("project")
            .to_owned();
        let storage_name = storage_name(&display_name)?;
        let project = Project {
            id: project_id.clone(),
            display_name,
            storage_name: storage_name.clone(),
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
        let dir = self
            .root
            .join("projects")
            .join(&storage_name)
            .join("efforts")
            .join(slug);
        if dir.exists() {
            return self.verify_existing_effort(&dir, &project, slug, &context);
        }
        self.ensure_project(&project)?;
        let baseline = capture_git_identity(&canonical_repo)?;
        let effort_id = format!(
            "effort-{}",
            short_digest(format!("{}:{slug}", project_id).as_bytes())
        );
        let effort = Effort {
            id: effort_id,
            slug: slug.into(),
            project_id,
            context,
            baseline_commit: baseline.commit,
            baseline_tree: baseline.tree,
            project_storage_name: storage_name,
            prepared_source,
        };
        // Write the complete effort off to the side, then atomically publish it.  Concurrent
        // model windows either win this rename or read the complete effort the winner wrote.
        self.materialize_snapshot(&canonical_repo, &effort.baseline_commit)?;
        let stage = self
            .root
            .join(".staging")
            .join(format!("effort-{}", unique_id()));
        fs::create_dir_all(&stage)?;
        write_json_atomic(&stage.join("effort.json"), &effort)?;
        write_bytes_sync(&stage.join("request.md"), effort.context.request.as_bytes())?;
        sync_dir(&stage)?;
        fs::create_dir_all(dir.parent().context("effort directory has no parent")?)?;
        match fs::rename(&stage, &dir) {
            Ok(()) => {
                sync_dir(dir.parent().context("effort directory has no parent")?)?;
                self.append_journal(
                    &effort,
                    "effort_created",
                    None,
                    serde_json::json!({"commit": effort.baseline_commit, "tree": effort.baseline_tree}),
                )?;
                Ok(effort)
            }
            Err(_error) if dir.exists() => {
                self.verify_existing_effort(&dir, &project, slug, &effort.context)
            }
            Err(error) => {
                Err(error).with_context(|| format!("cannot initialize effort {}", effort.id))
            }
        }
    }
    fn ensure_project(&self, expected: &Project) -> Result<()> {
        let dir = self.root.join("projects").join(&expected.storage_name);
        fs::create_dir_all(&dir)?;
        let path = dir.join("project.json");
        if publish_json_new_atomic(&path, expected)? {
            Ok(())
        } else {
            let existing: Project = read_json(&path)?;
            ensure!(
                existing.canonical_locator == expected.canonical_locator
                    && existing.id == expected.id,
                "project storage name '{}' is already used by a different canonical repository at {}; rename one repository directory before initializing",
                expected.storage_name,
                existing.canonical_locator.display()
            );
            Ok(())
        }
    }
    fn verify_existing_effort(
        &self,
        dir: &Path,
        project: &Project,
        slug: &str,
        expected_context: &ContextRevision,
    ) -> Result<Effort> {
        ensure!(
            dir.join("effort.json").is_file(),
            "effort directory already exists but is malformed: {}",
            dir.display()
        );
        let existing: Effort = read_json(&dir.join("effort.json"))?;
        ensure!(
            existing.project_id == project.id,
            "project storage name '{}' is already used by a different canonical repository; rename one repository directory before initializing",
            project.storage_name
        );
        ensure!(
            existing.slug == slug
                && existing.context.request_kind == expected_context.request_kind
                && existing.context.request == expected_context.request
                && existing.context.constraints == expected_context.constraints,
            "Effort \"{}\" already exists with a different frozen request or constraints. An effort's prepared request is immutable. Discovery-time clarifications must be recorded inside the Discovery run, not used to initialize a different request under the same effort.",
            slug
        );
        Ok(existing)
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
    /// Return every effort for one canonical project identity.  Callers that need to
    /// select an effort must still apply their own explicit eligibility rules; this
    /// deliberately does not infer a "latest" effort.
    pub fn efforts_for_project(&self, project: &Project) -> Result<Vec<Effort>> {
        let root = self.project_dir(project).join("efforts");
        if !root.exists() {
            return Ok(Vec::new());
        }
        let mut efforts: Vec<Effort> = Vec::new();
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            if path.join("effort.json").is_file() {
                efforts.push(read_json(&path.join("effort.json"))?);
            }
        }
        efforts.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(efforts)
    }
    /// Resolve an effort project by its canonical checkout location without using a
    /// presentation slug.  This is intentionally read-only and returns no match
    /// when the store has not seen the checkout.
    pub fn project_by_locator(&self, locator: &Path) -> Result<Option<Project>> {
        let projects = self.root.join("projects");
        if !projects.exists() {
            return Ok(None);
        }
        for entry in fs::read_dir(projects)? {
            let project_path = entry?.path();
            let project_file = project_path.join("project.json");
            if project_file.is_file() {
                let project: Project = read_json(&project_file)?;
                if project.canonical_locator == locator {
                    return Ok(Some(project));
                }
            }
        }
        Ok(None)
    }
    pub fn project_for(&self, effort: &Effort) -> Result<Project> {
        read_json(
            &self
                .root
                .join("projects")
                .join(&effort.project_storage_name)
                .join("project.json"),
        )
    }
    pub fn project_dir(&self, project: &Project) -> PathBuf {
        self.root.join("projects").join(&project.storage_name)
    }
    pub fn effort_dir(&self, effort: &Effort) -> PathBuf {
        self.root
            .join("projects")
            .join(&effort.project_storage_name)
            .join("efforts")
            .join(&effort.slug)
    }
    pub fn phase_dir(&self, effort: &Effort, phase: &str) -> Result<PathBuf> {
        validate_label(phase)?;
        let path = self.effort_dir(effort).join(phase);
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
        let path = self.effort_dir(effort).join("journal.jsonl");
        let record = JournalEvent {
            event: event.into(),
            ts_ms: now_ms(),
            effort_id: effort.id.clone(),
            run_id: run_id.map(str::to_owned),
            details,
        };
        let mut bytes = serde_json::to_vec(&record)?;
        bytes.push(b'\n');
        let mut file = OpenOptions::new().append(true).create(true).open(&path)?;
        file.write_all(&bytes)?;
        file.sync_data()?;
        Ok(())
    }
    pub fn read_journal(&self, effort: &Effort) -> Result<Vec<JournalEvent>> {
        let path = self.effort_dir(effort).join("journal.jsonl");
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
            run.effort_id == effort.id
                && run.context_id == effort.context.id
                && run.request_kind == effort.context.request_kind
                && run.baseline_commit == effort.baseline_commit
                && run.baseline_tree == effort.baseline_tree,
            "Discovery run belongs to another effort, context, or baseline"
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
        verify_git_checkout(&source, &run.baseline_commit, &run.baseline_tree)?;
        write_json_atomic(&root.join("run.json"), run)?;
        write_bytes_sync(&root.join("request.md"), effort.context.request.as_bytes())?;
        write_json_atomic(
            &root.join("context.json"),
            &serde_json::json!({"context_id": effort.context.id, "request_kind": effort.context.request_kind, "request": effort.context.request, "constraints": effort.context.constraints, "baseline_commit": effort.baseline_commit, "baseline_tree": effort.baseline_tree}),
        )?;
        write_bytes_sync(
            &root.join("technical-spec.md"),
            TECHNICAL_SPEC_TEMPLATE.as_bytes(),
        )?;
        fs::create_dir_all(root.join("graph"))?;
        fs::create_dir_all(root.join("scratch"))?;
        self.append_journal(
            effort,
            "discovery_workspace_prepared",
            Some(&run.run_id),
            serde_json::json!({}),
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
    pub fn artifact_dir(&self, effort: &Effort, artifact_id: &str) -> Result<PathBuf> {
        self.find_artifact_dir(effort, artifact_id)
    }
    pub fn load_envelope(&self, effort: &Effort, reference: &ArtifactRef) -> Result<Envelope> {
        Ok(self.load_bundle(effort, reference)?.0)
    }
    pub fn list_artifacts(&self, effort: &Effort) -> Result<Vec<ArtifactRef>> {
        let mut out = Vec::new();
        for phase in ["discovery", "reconcile", "adoption", "build", "audit"] {
            let dir = self.effort_dir(effort).join(phase);
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
            let efforts = p?.path().join("efforts");
            if !efforts.exists() {
                continue;
            }
            for entry in fs::read_dir(efforts)? {
                let path = entry?.path();
                let effort_file = path.join("effort.json");
                if effort_file.exists() {
                    let effort: Effort = read_json(&effort_file)?;
                    if effort.id == effort_id || effort.slug == effort_id {
                        found.push(path);
                    }
                }
            }
        }
        Ok(found)
    }
    fn find_artifact_dir(&self, effort: &Effort, artifact_id: &str) -> Result<PathBuf> {
        validate_id(artifact_id)?;
        let mut found = Vec::new();
        for phase in ["discovery", "reconcile", "adoption", "build", "audit"] {
            let path = self.effort_dir(effort).join(phase).join(artifact_id);
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

/// Verify that a Discovery source checkout is still the exact frozen Git worktree.
///
/// This deliberately asks Git for all untracked and ignored paths. Discovery workspaces
/// reserve `scratch/` for generated and experimental files, so nothing outside Git's own
/// administrative metadata is allowed to accumulate in `source/`.
pub fn verify_git_checkout(
    source: &Path,
    expected_commit: &str,
    expected_tree: &str,
) -> Result<()> {
    let expected_root = fs::canonicalize(source).with_context(|| {
        format!(
            "Discovery source checkout is missing or inaccessible: {}",
            source.display()
        )
    })?;
    ensure!(
        git(&expected_root, ["rev-parse", "--is-inside-work-tree"])? == "true",
        "Discovery source {} is not a Git worktree",
        source.display()
    );
    let actual_root = fs::canonicalize(PathBuf::from(git(
        &expected_root,
        ["rev-parse", "--show-toplevel"],
    )?))
    .context("Git returned an inaccessible Discovery worktree root")?;
    ensure!(
        actual_root == expected_root,
        "Discovery source {} resolves to Git worktree root {} rather than itself",
        source.display(),
        actual_root.display()
    );

    let actual_commit = git(&expected_root, ["rev-parse", "--verify", "HEAD^{commit}"])?;
    ensure!(
        actual_commit == expected_commit,
        "Discovery source HEAD differs from frozen baseline: expected {}, got {}",
        expected_commit,
        actual_commit
    );
    let actual_tree = git(&expected_root, ["rev-parse", "--verify", "HEAD^{tree}"])?;
    ensure!(
        actual_tree == expected_tree,
        "Discovery source HEAD tree differs from frozen baseline: expected {}, got {}",
        expected_tree,
        actual_tree
    );
    verify_tracked_working_tree(&expected_root, expected_tree)?;
    let status = git(
        &expected_root,
        [
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignored=matching",
        ],
    )?;
    ensure!(
        status.is_empty(),
        "Discovery source checkout is dirty: {}",
        concise_git_status(&status)
    );
    Ok(())
}

/// Compare tracked paths against the frozen tree through an isolated index. The checkout's
/// index can contain `assume-unchanged` or `skip-worktree` flags, which intentionally suppress
/// ordinary status checks. This temporary index starts from the expected tree without those
/// mutable flags and is removed before validation returns.
fn verify_tracked_working_tree(source: &Path, expected_tree: &str) -> Result<()> {
    let index = std::env::temp_dir().join(format!("orchestrate-git-index-{}", unique_id()));
    let lock = index.with_extension("lock");
    ensure!(
        !index.exists() && !lock.exists(),
        "cannot allocate isolated Git index for Discovery source validation"
    );
    let result = (|| {
        let environment = [("GIT_INDEX_FILE", index.as_path())];
        git_with_env(source, ["read-tree", expected_tree], &environment)?;
        // Refresh only the isolated index. A changed path makes this command return one, but
        // still records enough stat information for diff-files to distinguish clean files.
        let _ = git_with_env_allow_differences(
            source,
            ["update-index", "--really-refresh"],
            &environment,
        )?;
        let differences = git_with_env(
            source,
            ["diff-files", "--raw", "--no-ext-diff"],
            &environment,
        )?;
        ensure!(
            differences.is_empty(),
            "Discovery source tracked working-tree content differs from frozen tree: {}",
            concise_git_status(&differences)
        );
        Ok(())
    })();
    let _ = fs::remove_file(&index);
    let _ = fs::remove_file(&lock);
    result
}

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
    git_with_env(repo, args, &[])
}
fn git_with_env<const N: usize>(
    repo: &Path,
    args: [&str; N],
    environment: &[(&str, &Path)],
) -> Result<String> {
    let output = git_with_env_output(repo, args, environment)?;
    ensure!(
        output.status.success(),
        "git command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn git_with_env_allow_differences<const N: usize>(
    repo: &Path,
    args: [&str; N],
    environment: &[(&str, &Path)],
) -> Result<String> {
    let output = git_with_env_output(repo, args, environment)?;
    ensure!(
        output.status.success() || output.status.code() == Some(1),
        "git command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn git_with_env_output<const N: usize>(
    repo: &Path,
    args: [&str; N],
    environment: &[(&str, &Path)],
) -> Result<std::process::Output> {
    let mut command = Command::new("git");
    command.args(args).current_dir(repo);
    for (key, value) in environment {
        command.env(key, value);
    }
    Ok(command.output()?)
}
fn concise_git_status(status: &str) -> String {
    let entries = status.lines().take(8).collect::<Vec<_>>();
    let suffix = if status.lines().count() > entries.len() {
        "; …"
    } else {
        ""
    };
    format!("{}{}", entries.join("; "), suffix)
}
fn unique_id() -> String {
    format!(
        "{}-{}-{}",
        now_ms(),
        std::process::id(),
        ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}
fn short_digest(bytes: &[u8]) -> String {
    digest_bytes(bytes)[..16].into()
}
fn storage_name(value: &str) -> Result<String> {
    let name = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let name = name.trim_matches(['.', '_', '-']).to_owned();
    ensure!(
        !name.is_empty(),
        "project directory name has no safe storage representation"
    );
    Ok(name)
}
fn validate_label(v: &str) -> Result<()> {
    ensure!(
        !v.is_empty()
            && v != "."
            && v != ".."
            && v.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')),
        "invalid label {v}"
    );
    Ok(())
}
fn validate_id(v: &str) -> Result<()> {
    validate_label(v)
}
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_bytes_sync(path, &encode(value)?)
}
/// Atomically publish bytes only when `path` does not yet exist.
///
/// A completed temporary file is linked into place, so concurrent project initializers either
/// publish a complete `project.json` or observe the complete file published by their peer.
/// Unlike `rename`, `hard_link` never replaces an existing destination.
fn publish_json_new_atomic<T: Serialize>(path: &Path, value: &T) -> Result<bool> {
    publish_bytes_new_atomic(path, &encode(value)?)
}
fn publish_bytes_new_atomic(path: &Path, bytes: &[u8]) -> Result<bool> {
    let parent = path.parent().context("path has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|v| v.to_str()).unwrap_or("file"),
        unique_id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);

    match fs::hard_link(&temp, path) {
        Ok(()) => {
            fs::remove_file(&temp)?;
            sync_dir(parent)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            fs::remove_file(&temp)?;
            Ok(false)
        }
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error.into())
        }
    }
}
fn validate_marker(marker: &Path) -> Result<()> {
    let stored: StoreMarker = read_json(marker)?;
    ensure!(
        stored.format_version == STORE_FORMAT_VERSION,
        "unsupported orchestration store format {}; expected {}",
        stored.format_version,
        STORE_FORMAT_VERSION
    );
    Ok(())
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

    #[test]
    fn concurrent_store_creation_never_reports_a_legacy_store() {
        // Contenders race between the marker probe and the legacy scan, so a marker committed by
        // a peer must not be read back as legacy content.
        for _ in 0..32 {
            let dir = std::env::temp_dir().join(format!("store-open-{}", unique_id()));
            std::fs::create_dir_all(&dir).unwrap();
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let dir = dir.clone();
                    std::thread::spawn(move || Store::open(&dir).map(|store| store.root).is_ok())
                })
                .collect();
            for handle in handles {
                assert!(handle.join().unwrap());
            }
            assert!(dir.join("store.json").is_file());
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
