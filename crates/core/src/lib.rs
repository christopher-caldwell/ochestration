//! Shared mechanical infrastructure: external storage, Git snapshots, and immutable publication.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail, ensure};
use orchestrate_contracts::{
    ArtifactKind, ArtifactRef, Envelope, Invalidation, PayloadDigest, Provenance, artifact_ref,
    decode, digest_bytes, encode, validate_envelope,
};
use serde::{Deserialize, Serialize};
use wait_timeout::ChildExt;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProviderObservation<T> {
    pub value: T,
    pub program: PathBuf,
    pub exit_code: i32,
    pub stderr: String,
}

/// Invokes one explicitly selected provider command using a bounded JSON protocol.
///
/// The command receives one JSON input on standard input and must emit exactly one validated JSON
/// output on standard output. Its executable identity is recorded by the caller as provenance.
pub fn invoke_provider_json<I: Serialize, O: for<'a> Deserialize<'a>>(
    program: &Path,
    input: &I,
    max_output_bytes: usize,
) -> Result<ProviderObservation<O>> {
    ensure!(
        program.is_absolute(),
        "provider command must be an absolute selected path"
    );
    let input = encode(input)?;
    let mut child = Command::new(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("could not launch provider {}", program.display()))?;
    let stdin = child.stdin.as_mut().context("provider stdin unavailable")?;
    stdin.write_all(&input)?;
    if child.wait_timeout(Duration::from_secs(60))?.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        bail!("provider exceeded the 60-second execution limit");
    }
    let output = child.wait_with_output()?;
    ensure!(
        output.stdout.len() <= max_output_bytes,
        "provider output exceeded declared byte limit"
    );
    ensure!(
        output.status.success(),
        "provider failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = decode(&output.stdout).context("provider returned malformed protocol output")?;
    Ok(ProviderObservation {
        value,
        program: program.to_owned(),
        exit_code: output.status.code().unwrap_or(-1),
        stderr: String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(16_384)
            .collect(),
    })
}

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
    pub slots: [String; 3],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Effort {
    pub id: String,
    pub slug: String,
    pub project_id: String,
    pub context: ContextRevision,
    pub cohort: Cohort,
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

/// A validated node in an artifact's authoritative parent graph.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LineageNode {
    pub artifact: ArtifactRef,
    pub outcome: String,
    pub parents: Vec<ArtifactRef>,
    pub invalidated_by: Option<ArtifactRef>,
}

impl Store {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        ensure!(
            root.is_absolute(),
            "orchestration root must be an absolute path"
        );
        fs::create_dir_all(root)
            .with_context(|| format!("cannot create store root {}", root.display()))?;
        let root = fs::canonicalize(root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn init_effort(
        &self,
        repo: &Path,
        slug: &str,
        request: String,
        constraints: Vec<String>,
    ) -> Result<Effort> {
        validate_label(slug)?;
        let canonical_repo = fs::canonicalize(repo)
            .with_context(|| format!("cannot canonicalize repository {}", repo.display()))?;
        ensure!(
            canonical_repo.join(".git").exists(),
            "target is not a supported Git repository"
        );
        let baseline = capture_git_identity(&canonical_repo)?;
        let project_id = short_digest(canonical_repo.to_string_lossy().as_bytes());
        let display_name = canonical_repo
            .file_name()
            .and_then(|part| part.to_str())
            .unwrap_or("project")
            .to_owned();
        let project = Project {
            id: project_id.clone(),
            display_name,
            canonical_locator: canonical_repo.clone(),
        };
        let context_digest = digest_bytes(request.as_bytes());
        let context = ContextRevision {
            id: format!("ctx-{}", &context_digest[..16]),
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
            slots: ["a".to_owned(), "b".to_owned(), "c".to_owned()],
        };
        let effort = Effort {
            id: effort_id,
            slug: slug.to_owned(),
            project_id,
            context,
            cohort,
        };
        let dir = self.effort_dir(&effort.project_id, &effort.id);
        if dir.exists() {
            bail!("effort already exists: {}", effort.id);
        }
        fs::create_dir_all(&dir)?;
        write_json_atomic(&dir.join("project.json"), &project)?;
        write_json_atomic(&dir.join("effort.json"), &effort)?;
        self.materialize_snapshot(&canonical_repo, &effort.cohort.baseline_commit)?;
        Ok(effort)
    }

    pub fn load_effort(&self, effort_id: &str) -> Result<Effort> {
        validate_id(effort_id)?;
        let matches = self.find_effort_dirs(effort_id)?;
        match matches.as_slice() {
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
        let archive = staging.join("source.tar");
        let output = Command::new("git")
            .args(["archive", "--format=tar", &identity.commit])
            .current_dir(repo)
            .output()
            .context("failed to start git archive")?;
        ensure!(
            output.status.success(),
            "git archive failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::write(&archive, output.stdout)?;
        let unpack = staging.join("source");
        fs::create_dir(&unpack)?;
        let status = Command::new("tar")
            .args([
                "-xf",
                archive.to_string_lossy().as_ref(),
                "-C",
                unpack.to_string_lossy().as_ref(),
            ])
            .status()
            .context("failed to start tar")?;
        ensure!(status.success(), "could not unpack committed snapshot");
        write_json_atomic(
            &unpack.join("source-manifest.json"),
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

    pub fn discovery_workspace(&self, effort: &Effort, run_id: &str) -> Result<PathBuf> {
        validate_id(run_id)?;
        let source = self
            .root
            .join("snapshots")
            .join(&effort.cohort.baseline_commit)
            .join("source");
        ensure!(source.exists(), "frozen source snapshot is missing");
        let destination = self
            .phase_dir(effort, "discovery")?
            .join(run_id)
            .join("source");
        if !destination.exists() {
            copy_tree(&source, &destination)?;
        }
        Ok(destination)
    }

    #[allow(clippy::too_many_arguments)] // The envelope fields are deliberately explicit at the authority boundary.
    pub fn publish<T: Serialize>(
        &self,
        effort: &Effort,
        phase: &str,
        kind: ArtifactKind,
        run_id: String,
        outcome: String,
        parents: Vec<ArtifactRef>,
        provenance: Provenance,
        payload: &T,
    ) -> Result<ArtifactRef> {
        let phase_dir = self.phase_dir(effort, phase)?;
        // The operation/run identity, rather than an acknowledgement, determines
        // the public artifact location. A caller that loses its acknowledgement
        // can therefore discover the already-committed compatible result.
        let artifact_id = format!(
            "{}-{}",
            phase,
            short_digest(format!("{phase}:{run_id}").as_bytes())
        );
        let final_dir = phase_dir.join(&artifact_id);
        let payload_bytes = encode(payload)?;
        if final_dir.exists() {
            return self.recover_publication(
                effort,
                &final_dir,
                kind,
                &artifact_id,
                &run_id,
                &outcome,
                &parents,
                &provenance,
                &payload_bytes,
            );
        }
        let stage = self
            .root
            .join(".staging")
            .join(format!("{}-{}", artifact_id, unique_id()));
        fs::create_dir_all(&stage)?;
        write_bytes_sync(&stage.join("payload.json"), &payload_bytes)?;
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
            producer_version: orchestrate_contracts::PRODUCT_VERSION.to_owned(),
            parents: parents.clone(),
            payloads: vec![PayloadDigest {
                path: "payload.json".to_owned(),
                sha256: digest_bytes(&payload_bytes),
                bytes: payload_bytes.len() as u64,
            }],
            provenance: provenance.clone(),
        };
        validate_envelope(&envelope)?;
        let manifest_bytes = encode(&envelope)?;
        write_bytes_sync(&stage.join("manifest.json"), &manifest_bytes)?;
        sync_dir(&stage)?;
        if let Err(error) = fs::rename(&stage, &final_dir) {
            if final_dir.exists() {
                return self.recover_publication(
                    effort,
                    &final_dir,
                    kind,
                    &artifact_id,
                    &run_id,
                    &outcome,
                    &parents,
                    &provenance,
                    &payload_bytes,
                );
            }
            return Err(error).with_context(|| format!("cannot commit artifact {artifact_id}"));
        }
        sync_dir(&phase_dir)?;
        Ok(artifact_ref(&envelope, &manifest_bytes))
    }

    #[allow(clippy::too_many_arguments)]
    fn recover_publication(
        &self,
        effort: &Effort,
        final_dir: &Path,
        kind: ArtifactKind,
        artifact_id: &str,
        run_id: &str,
        outcome: &str,
        parents: &[ArtifactRef],
        provenance: &Provenance,
        payload_bytes: &[u8],
    ) -> Result<ArtifactRef> {
        let manifest_bytes = fs::read(final_dir.join("manifest.json"))
            .context("existing publication lacks a valid manifest")?;
        let envelope: Envelope = decode(&manifest_bytes)?;
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
            envelope.payloads.len() == 1
                && envelope.payloads[0].path == "payload.json"
                && envelope.payloads[0].bytes == payload_bytes.len() as u64
                && envelope.payloads[0].sha256 == digest_bytes(payload_bytes),
            "operation identity conflicts with existing payload content"
        );
        self.load_envelope(
            effort,
            &ArtifactRef {
                kind,
                artifact_id: artifact_id.to_owned(),
                digest: digest_bytes(&manifest_bytes),
            },
        )?;
        Ok(artifact_ref(&envelope, &manifest_bytes))
    }

    pub fn load_artifact<T: for<'a> Deserialize<'a>>(
        &self,
        effort: &Effort,
        reference: &ArtifactRef,
    ) -> Result<(Envelope, T)> {
        let dir = self.find_artifact_dir(effort, &reference.artifact_id)?;
        let manifest_bytes = fs::read(dir.join("manifest.json"))?;
        ensure!(
            digest_bytes(&manifest_bytes) == reference.digest,
            "artifact manifest digest mismatch"
        );
        let envelope: Envelope = decode(&manifest_bytes)?;
        validate_envelope(&envelope)?;
        ensure!(
            envelope.kind == reference.kind && envelope.artifact_id == reference.artifact_id,
            "artifact reference mismatch"
        );
        ensure!(
            envelope.project_id == effort.project_id && envelope.effort_id == effort.id,
            "cross-effort artifact reference"
        );
        for payload in &envelope.payloads {
            let bytes = fs::read(dir.join(&payload.path))?;
            ensure!(
                bytes.len() as u64 == payload.bytes && digest_bytes(&bytes) == payload.sha256,
                "artifact payload integrity failure"
            );
        }
        Ok((envelope, decode(&fs::read(dir.join("payload.json"))?)?))
    }

    pub fn find_artifact_ref(&self, effort: &Effort, artifact_id: &str) -> Result<ArtifactRef> {
        let dir = self.find_artifact_dir(effort, artifact_id)?;
        let bytes = fs::read(dir.join("manifest.json"))?;
        let envelope: Envelope = decode(&bytes)?;
        Ok(artifact_ref(&envelope, &bytes))
    }

    pub fn load_envelope(&self, effort: &Effort, reference: &ArtifactRef) -> Result<Envelope> {
        let dir = self.find_artifact_dir(effort, &reference.artifact_id)?;
        let bytes = fs::read(dir.join("manifest.json"))?;
        ensure!(
            digest_bytes(&bytes) == reference.digest,
            "artifact manifest digest mismatch"
        );
        let envelope: Envelope = decode(&bytes)?;
        validate_envelope(&envelope)?;
        ensure!(
            envelope.kind == reference.kind && envelope.artifact_id == reference.artifact_id,
            "artifact reference mismatch"
        );
        ensure!(
            envelope.project_id == effort.project_id && envelope.effort_id == effort.id,
            "cross-effort artifact reference"
        );
        for payload in &envelope.payloads {
            let payload_bytes = fs::read(dir.join(&payload.path))?;
            ensure!(
                payload_bytes.len() as u64 == payload.bytes
                    && digest_bytes(&payload_bytes) == payload.sha256,
                "artifact payload integrity failure"
            );
        }
        Ok(envelope)
    }

    pub fn list_artifacts(&self, effort: &Effort) -> Result<Vec<ArtifactRef>> {
        let mut results = Vec::new();
        let effort_dir = self.effort_dir(&effort.project_id, &effort.id);
        for phase in [
            "discovery",
            "consensus",
            "agreement",
            "build",
            "audit",
            "invalidation",
        ] {
            let phase_dir = effort_dir.join(phase);
            if !phase_dir.exists() {
                continue;
            }
            for entry in fs::read_dir(phase_dir)? {
                let entry = entry?;
                let manifest = entry.path().join("manifest.json");
                if manifest.exists() {
                    let bytes = fs::read(&manifest)?;
                    if let Ok(envelope) = decode::<Envelope>(&bytes) {
                        results.push(artifact_ref(&envelope, &bytes));
                    }
                }
            }
        }
        results.sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
        Ok(results)
    }

    pub fn is_invalidated(
        &self,
        effort: &Effort,
        target: &ArtifactRef,
    ) -> Result<Option<Invalidation>> {
        for reference in self.list_artifacts(effort)? {
            if reference.kind == ArtifactKind::Invalidation {
                let (_, notice): (_, Invalidation) = self.load_artifact(effort, &reference)?;
                if notice.target == *target {
                    return Ok(Some(notice));
                }
            }
        }
        Ok(None)
    }

    /// Returns the first current invalidation attached to `target` or any of its
    /// authoritative parents.  The immutable history remains readable; this is
    /// only an eligibility check for a new consequential operation.
    pub fn lineage_invalidation(
        &self,
        effort: &Effort,
        target: &ArtifactRef,
    ) -> Result<Option<Invalidation>> {
        let mut visited = std::collections::BTreeSet::new();
        self.lineage_invalidation_inner(effort, target, &mut visited)
    }

    /// Reconstructs the selected artifact's complete authoritative parent graph.
    /// Every visited manifest and payload is verified. A missing, cross-effort, or
    /// cyclic parent is deliberately an error rather than a guessed replacement.
    pub fn reconstruct_lineage(
        &self,
        effort: &Effort,
        target: &ArtifactRef,
    ) -> Result<Vec<LineageNode>> {
        let mut nodes = Vec::new();
        let mut visiting = std::collections::BTreeSet::new();
        let mut complete = std::collections::BTreeSet::new();
        self.reconstruct_lineage_inner(effort, target, &mut visiting, &mut complete, &mut nodes)?;
        Ok(nodes)
    }

    fn reconstruct_lineage_inner(
        &self,
        effort: &Effort,
        target: &ArtifactRef,
        visiting: &mut std::collections::BTreeSet<(ArtifactKind, String, String)>,
        complete: &mut std::collections::BTreeSet<(ArtifactKind, String, String)>,
        nodes: &mut Vec<LineageNode>,
    ) -> Result<()> {
        let identity = (
            target.kind.clone(),
            target.artifact_id.clone(),
            target.digest.clone(),
        );
        if complete.contains(&identity) {
            return Ok(());
        }
        ensure!(
            visiting.insert(identity.clone()),
            "artifact lineage contains a cycle"
        );
        let envelope = self.load_envelope(effort, target)?;
        let invalidated_by = self.invalidation_reference(effort, target)?;
        nodes.push(LineageNode {
            artifact: target.clone(),
            outcome: envelope.outcome.clone(),
            parents: envelope.parents.clone(),
            invalidated_by,
        });
        for parent in &envelope.parents {
            self.reconstruct_lineage_inner(effort, parent, visiting, complete, nodes)?;
        }
        visiting.remove(&identity);
        complete.insert(identity);
        Ok(())
    }

    fn invalidation_reference(
        &self,
        effort: &Effort,
        target: &ArtifactRef,
    ) -> Result<Option<ArtifactRef>> {
        for reference in self.list_artifacts(effort)? {
            if reference.kind == ArtifactKind::Invalidation {
                let (_, notice): (_, Invalidation) = self.load_artifact(effort, &reference)?;
                if notice.target == *target {
                    return Ok(Some(reference));
                }
            }
        }
        Ok(None)
    }

    fn lineage_invalidation_inner(
        &self,
        effort: &Effort,
        target: &ArtifactRef,
        visited: &mut std::collections::BTreeSet<(ArtifactKind, String, String)>,
    ) -> Result<Option<Invalidation>> {
        let identity = (
            target.kind.clone(),
            target.artifact_id.clone(),
            target.digest.clone(),
        );
        ensure!(
            visited.insert(identity),
            "artifact lineage contains a cycle"
        );
        if let Some(notice) = self.is_invalidated(effort, target)? {
            return Ok(Some(notice));
        }
        let envelope = self.load_envelope(effort, target)?;
        for parent in envelope.parents {
            if let Some(notice) = self.lineage_invalidation_inner(effort, &parent, visited)? {
                return Ok(Some(notice));
            }
        }
        Ok(None)
    }

    fn find_effort_dirs(&self, effort_id: &str) -> Result<Vec<PathBuf>> {
        let projects = self.root.join("projects");
        if !projects.exists() {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        for project in fs::read_dir(projects)? {
            let path = project?.path().join("efforts").join(effort_id);
            if path.join("effort.json").exists() {
                found.push(path);
            }
        }
        Ok(found)
    }

    fn find_artifact_dir(&self, effort: &Effort, artifact_id: &str) -> Result<PathBuf> {
        validate_id(artifact_id)?;
        let base = self.effort_dir(&effort.project_id, &effort.id);
        let mut found = Vec::new();
        for phase in [
            "discovery",
            "consensus",
            "agreement",
            "build",
            "audit",
            "invalidation",
        ] {
            let candidate = base.join(phase).join(artifact_id);
            if candidate.join("manifest.json").exists() {
                found.push(candidate);
            }
        }
        match found.as_slice() {
            [path] => Ok(path.clone()),
            [] => bail!("unknown artifact {artifact_id}"),
            _ => bail!("ambiguous artifact {artifact_id}"),
        }
    }
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

fn capture_git_identity(repo: &Path) -> Result<GitIdentity> {
    capture_git_identity_at(repo, "HEAD")
}
fn capture_git_identity_at(repo: &Path, revision: &str) -> Result<GitIdentity> {
    let commit = git(
        repo,
        ["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
    )?;
    let tree = git(
        repo,
        ["rev-parse", "--verify", &format!("{commit}^{{tree}}")],
    )?;
    Ok(GitIdentity { commit, tree })
}
fn git_file_inventory(repo: &Path, commit: &str) -> Result<BTreeMap<String, String>> {
    let listing = git(repo, ["ls-tree", "-r", "-z", commit])?;
    let mut inventory = BTreeMap::new();
    for entry in listing.split('\0').filter(|part| !part.is_empty()) {
        let (metadata, path) = entry.split_once('\t').context("invalid git tree entry")?;
        let fields: Vec<_> = metadata.split_whitespace().collect();
        ensure!(fields.len() == 3, "invalid git tree metadata");
        ensure!(
            fields[1] != "commit",
            "submodules are not supported in committed snapshots"
        );
        inventory.insert(path.to_owned(), format!("{} {}", fields[0], fields[2]));
    }
    Ok(inventory)
}
fn git<const N: usize>(repo: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .context("could not start git")?;
    ensure!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}
fn unique_id() -> String {
    format!(
        "{:x}-{:x}",
        now_ms(),
        ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}
fn short_digest(bytes: &[u8]) -> String {
    digest_bytes(bytes)[..16].to_owned()
}
fn slugify(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}
fn validate_label(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 100
            && !value.contains('/')
            && !value.contains('\\')
            && value != "."
            && value != "..",
        "unsafe label"
    );
    Ok(())
}
fn validate_id(value: &str) -> Result<()> {
    validate_label(value)
}
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_bytes_sync(path, &encode(value)?)
}
fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T> {
    decode(&fs::read(path).with_context(|| format!("cannot read {}", path.display()))?)
}
fn write_bytes_sync(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("path has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file"),
        unique_id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    sync_dir(parent)
}
fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?.sync_all().map_err(Into::into)
}
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&source_path)?;
            ensure!(
                !target.is_absolute()
                    && !target
                        .components()
                        .any(|part| matches!(part, Component::ParentDir)),
                "unsafe source symlink"
            );
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, destination_path)?;
        } else if metadata.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &destination_path)?;
            fs::set_permissions(&destination_path, metadata.permissions())?;
        } else {
            bail!("unsupported source file type");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_cannot_escape() {
        assert!(validate_label("../outside").is_err());
        assert!(validate_label("valid name").is_ok());
    }

    #[test]
    fn provider_protocol_rejects_malformed_output() {
        let directory =
            std::env::temp_dir().join(format!("orchestrate-provider-test-{}", unique_id()));
        fs::create_dir_all(&directory).unwrap();
        let script = directory.join("provider.sh");
        fs::write(&script, "#!/bin/sh\nprintf 'not-json'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let result: Result<ProviderObservation<serde_json::Value>> =
            invoke_provider_json(&script, &serde_json::json!({"request":"fixture"}), 1024);
        assert!(result.is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn publication_retry_recovers_only_identical_operation_content() {
        let directory =
            std::env::temp_dir().join(format!("orchestrate-publication-test-{}", unique_id()));
        let store = Store::open(&directory).unwrap();
        let effort = Effort {
            id: "effort".to_owned(),
            slug: "effort".to_owned(),
            project_id: "project".to_owned(),
            context: ContextRevision {
                id: "context".to_owned(),
                request: "request".to_owned(),
                constraints: Vec::new(),
                digest: "d".repeat(64),
            },
            cohort: Cohort {
                id: "cohort".to_owned(),
                project_id: "project".to_owned(),
                effort_id: "effort".to_owned(),
                context_id: "context".to_owned(),
                baseline_commit: "b".repeat(40),
                baseline_tree: "t".repeat(40),
                slots: ["a".to_owned(), "b".to_owned(), "c".to_owned()],
            },
        };
        let provenance = Provenance {
            host: "test".to_owned(),
            execution_provider: None,
            requested_model: None,
            observed_model: None,
            requested_effort: None,
            observed_effort: None,
            guide_digest: "g".repeat(64),
            independence: orchestrate_contracts::Independence::AccessEnforced,
        };
        let first = store
            .publish(
                &effort,
                "audit",
                ArtifactKind::Audit,
                "stable-operation".to_owned(),
                "PASS".to_owned(),
                Vec::new(),
                provenance.clone(),
                &serde_json::json!({"result":"same"}),
            )
            .unwrap();
        let retry = store
            .publish(
                &effort,
                "audit",
                ArtifactKind::Audit,
                "stable-operation".to_owned(),
                "PASS".to_owned(),
                Vec::new(),
                provenance.clone(),
                &serde_json::json!({"result":"same"}),
            )
            .unwrap();
        assert_eq!(first, retry);
        assert!(
            store
                .publish(
                    &effort,
                    "audit",
                    ArtifactKind::Audit,
                    "stable-operation".to_owned(),
                    "PASS".to_owned(),
                    Vec::new(),
                    provenance,
                    &serde_json::json!({"result":"different"}),
                )
                .is_err()
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
