use anyhow::{Context, Result, ensure};
use orchestrate_contracts::{
    ArtifactRef, PayloadDigest, decode, digest_bytes, encode, investigation::InvestigationManifest,
};
use orchestrate_core::write_bytes_sync;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Component, Path},
    process::Command,
};

pub fn json(path: &Path, value: &impl Serialize) -> Result<()> {
    write_bytes_sync(path, &encode(value)?)
}
pub fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").args(args).current_dir(repo).output()?;
    ensure!(
        out.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8(out.stdout)?
        .trim_end_matches('\n')
        .to_owned())
}
pub fn safe(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':', '\0'])
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
        && Path::new(path)
            .components()
            .all(|p| matches!(p, Component::Normal(_)))
}
/// Read files or the text of a final symlink; never follow symlinked parent directories.
pub fn read(root: &Path, relative: &str) -> Result<Vec<u8>> {
    ensure!(safe(relative), "unsafe artifact/source path {relative:?}");
    let mut path = root.to_owned();
    let parts: Vec<_> = relative.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        path.push(part);
        let meta = fs::symlink_metadata(&path)?;
        if i + 1 < parts.len() {
            ensure!(
                meta.is_dir() && !meta.file_type().is_symlink(),
                "symlinked or non-directory parent in {relative}"
            );
        } else if meta.file_type().is_symlink() {
            return Ok(fs::read_link(&path)?.to_string_lossy().as_bytes().to_vec());
        } else {
            ensure!(meta.is_file(), "source is not a file: {relative}");
        }
    }
    Ok(fs::read(path)?)
}
pub fn inventory(repo: &Path, commit: &str) -> Result<BTreeMap<String, String>> {
    let listing = git(repo, &["ls-tree", "-r", "--name-only", "-z", commit])?;
    listing
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(|path| Ok((path.to_owned(), digest_bytes(&read(repo, path)?))))
        .collect()
}
pub fn verify_inventory(root: &Path, files: &BTreeMap<String, String>) -> Result<()> {
    for (path, digest) in files {
        ensure!(
            digest_bytes(&read(root, path)?) == *digest,
            "source/input digest mismatch: {path}"
        );
    }
    Ok(())
}
pub fn seal(
    root: &Path,
    identity: &str,
    input_digest: &str,
    files: &[String],
    parents: &[ArtifactRef],
) -> Result<String> {
    let mut payloads = Vec::new();
    let mut seen = HashSet::new();
    for path in files {
        ensure!(seen.insert(path), "duplicate manifest payload {path}");
        let bytes = read(root, path)?;
        payloads.push(PayloadDigest {
            path: path.clone(),
            sha256: digest_bytes(&bytes),
            bytes: bytes.len() as u64,
        });
    }
    payloads.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = InvestigationManifest {
        schema_version: 1,
        identity: identity.into(),
        input_digest: input_digest.into(),
        payloads,
        audit_parents: parents.to_vec(),
    };
    let bytes = encode(&manifest)?;
    ensure!(
        !root.join("manifest.json").exists(),
        "refusing to replace a frozen investigation manifest"
    );
    write_bytes_sync(&root.join("manifest.json"), &bytes)?;
    Ok(digest_bytes(&bytes))
}
pub fn verify(root: &Path, expected: Option<&str>) -> Result<InvestigationManifest> {
    let bytes = read(root, "manifest.json")?;
    if let Some(digest) = expected {
        ensure!(digest_bytes(&bytes) == digest, "manifest digest mismatch");
    }
    let manifest: InvestigationManifest = decode(&bytes)?;
    ensure!(
        manifest.schema_version == 1
            && !manifest.identity.is_empty()
            && !manifest.input_digest.is_empty(),
        "invalid investigation manifest identity/version"
    );
    let mut seen = HashSet::new();
    for payload in &manifest.payloads {
        ensure!(seen.insert(&payload.path), "duplicate manifest path");
        let bytes = read(root, &payload.path)
            .with_context(|| format!("missing frozen payload {}", payload.path))?;
        ensure!(
            bytes.len() as u64 == payload.bytes && digest_bytes(&bytes) == payload.sha256,
            "payload digest mismatch: {}",
            payload.path
        );
    }
    Ok(manifest)
}
