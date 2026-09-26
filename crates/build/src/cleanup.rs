//! Ownership-scoped cleanup of generated Cargo products in contained checkouts.
//!
//! Only a checkout the controller itself recorded, for an action that is no
//! longer active and whose evidence is already durable, is ever considered.
//! Inside such a checkout only entries that are positively known generated
//! Cargo output are removed, one at a time; tracked source, history, evidence,
//! cited fixtures and unknown files stay.  A candidate directory that holds
//! anything the controller cannot identify as its own generated output is left
//! alone and reported instead of being deleted as a whole.  Cleanup is
//! dry-runnable, idempotent, and its failures are recorded separately from any
//! stop or verdict.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use orchestrate_core::{Effort, Store};
use serde::Serialize;

use crate::{BuildState, CheckoutRecord, ProjectLock, read_json};

/// The product directory names an eligible checkout may lose *entries* from.  A
/// directory merely named `target` is never enough on its own: eligibility comes
/// from the controller's own ownership record for that exact checkout first,
/// and even then only the known entries below are removed.
const GENERATED_PRODUCTS: &[&str] = &["target"];

/// Durable records of one action that may cite a fixture or an evidence file
/// living inside a generated-output tree.  Raw provider transports are not
/// citation sources: they are unedited provider output, so a path appearing
/// there is not a reference the Build relies on.
const CITATION_SOURCES: &[&str] = &["report.md", "result.json", "assessment.json", "action.json"];

#[derive(Clone, Debug, Serialize)]
pub struct CleanupOutcome {
    pub effort: String,
    pub dry_run: bool,
    pub removed: Vec<CleanupRemoval>,
    pub skipped: Vec<CleanupSkip>,
    /// Files and directories deliberately kept because they are cited by
    /// durable evidence.  They are reported, not silently left behind.
    pub preserved: Vec<CleanupPreservation>,
    pub failed: Vec<CleanupFailure>,
    pub reclaimed_bytes: u64,
    /// True when every eligible checkout reached a clean state: nothing failed
    /// and nothing had to be preserved.  A cited fixture or an unknown file
    /// inside an owned product directory leaves it not fully clean, and that is
    /// reported here rather than resolved by deleting the unknown content.
    pub complete: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CleanupRemoval {
    pub action_id: String,
    pub checkout: String,
    pub path: String,
    pub bytes: u64,
    pub status: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct CleanupPreservation {
    pub action_id: String,
    pub checkout: String,
    pub path: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CleanupSkip {
    pub action_id: Option<String>,
    pub checkout: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CleanupFailure {
    pub action_id: String,
    pub checkout: String,
    pub error: String,
}

/// Whether one recorded checkout may be cleaned now, and why not when it may
/// not.  Every reason is stated rather than silently skipped.
fn eligibility(state: &BuildState, record: &CheckoutRecord, build_dir: &Path) -> Result<()> {
    // The current action is only cleanable once the Build is terminal: before
    // that, its checkout is still the workspace its result came from.
    ensure!(
        record.action_id != state.action.id || state.terminal.is_some(),
        "action {} is still the active Build action",
        record.action_id
    );
    if let Some(stop) = &state.stop {
        ensure!(
            stop.action.id != record.action_id,
            "action {} is the stopped action; its evidence is still being interpreted",
            record.action_id
        );
    }
    let action_dir = build_dir.join("artifacts").join(&record.action_id);
    ensure!(
        action_dir.join("transport.completed").is_file(),
        "action {} never recorded a completed provider transport",
        record.action_id
    );
    let path = PathBuf::from(&record.path);
    // Canonical containment: the checkout must live under this effort's Build
    // directory, and must not be a symlink to somewhere else.
    let canonical_build = fs::canonicalize(build_dir)
        .with_context(|| format!("cannot canonicalize {}", build_dir.display()))?;
    let canonical = fs::canonicalize(&path)
        .with_context(|| format!("recorded checkout {} is unreadable", path.display()))?;
    ensure!(
        canonical.starts_with(&canonical_build),
        "recorded checkout {} is outside this Build directory",
        canonical.display()
    );
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_dir(),
        "recorded checkout {} is no longer a directory",
        path.display()
    );
    ensure!(
        fs::canonicalize(action_dir.join(checkout_directory_name(&record.kind)))? == canonical,
        "recorded checkout {} is not the {} directory of action {}",
        canonical.display(),
        record.kind,
        record.action_id
    );
    let head = crate::git(&canonical, ["rev-parse", "HEAD"]).with_context(|| {
        format!(
            "recorded checkout {} is not a readable Git checkout",
            canonical.display()
        )
    })?;
    ensure!(
        head == record.commit,
        "recorded checkout {} moved from {} to {head}; it is no longer the checkout the controller recorded",
        canonical.display(),
        record.commit
    );
    // A directory named `target` is not automatically generated output: a
    // repository may track files under that name.  Nothing tracked is ever
    // removed, so any tracked entry under a candidate product directory makes
    // the whole directory ineligible.
    for product in GENERATED_PRODUCTS {
        let tracked = crate::git(&canonical, ["ls-files", "--", product])?;
        ensure!(
            tracked.is_empty(),
            "recorded checkout {} tracks files under {product}, so it is not generated output and is left untouched",
            canonical.display()
        );
    }
    Ok(())
}

fn checkout_directory_name(kind: &str) -> &'static str {
    match kind {
        "review" => "source",
        "final_audit" => "verification",
        "once-over" | "once_over" => "once-over",
        _ => "source",
    }
}

/// Every durable text one action left about itself, so a fixture or evidence
/// file cited inside a product tree can be recognized before cleanup.
fn citation_text(build_dir: &Path, action_id: &str) -> String {
    let mut text = String::new();
    for source in CITATION_SOURCES {
        let path = build_dir.join("artifacts").join(action_id).join(source);
        if let Ok(bytes) = fs::read(&path) {
            text.push_str(&String::from_utf8_lossy(&bytes));
            text.push('\n');
        }
    }
    // A resolution can cite evidence for a checkout its continuation used,
    // and retained Build evidence can name a fixture directly.
    for directory in ["resolutions", "evidence"] {
        let dir = build_dir.join(directory);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().is_file()
                && let Ok(bytes) = fs::read(entry.path())
            {
                text.push_str(&String::from_utf8_lossy(&bytes));
                text.push('\n');
            }
        }
    }
    text
}

/// Whether one candidate entry is cited by the action's own durable records.
fn is_cited(text: &str, checkout: &Path, entry: &Path) -> bool {
    let relative = entry
        .strip_prefix(checkout)
        .unwrap_or(entry)
        .to_string_lossy()
        .replace('\\', "/");
    // A citation to one file inside a directory also protects the ancestor
    // directory from being classified as unrelated unknown content.
    text.contains(&relative)
        || (entry.is_dir() && text.contains(&format!("{relative}/")))
        || text.contains(&entry.to_string_lossy().into_owned())
}

fn cargo_hash_suffix(stem: &str) -> bool {
    stem.rsplit_once('-').is_some_and(|(_, hash)| {
        (16..=64).contains(&hash.len()) && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

/// Cargo dependency directories contain compiler products with a hash in the
/// filename. Names without that shape, nested directories, and all other
/// profile contents are ambiguous and remain in place.
fn known_dependency_artifact(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let path = Path::new(name);
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    if !matches!(
        extension,
        "rlib" | "rmeta" | "d" | "so" | "dylib" | "dll" | "a" | "o"
    ) {
        return false;
    }
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    cargo_hash_suffix(stem)
}

fn push_preserved(
    outcome: &mut CleanupOutcome,
    record: &CheckoutRecord,
    path: &Path,
    reason: String,
) {
    outcome.preserved.push(CleanupPreservation {
        action_id: record.action_id.clone(),
        checkout: record.path.clone(),
        path: path.to_string_lossy().into_owned(),
        reason,
    });
}

fn collect_profile_products(
    record: &CheckoutRecord,
    checkout: &Path,
    profile: &Path,
    citations: &str,
    outcome: &mut CleanupOutcome,
) -> Vec<PathBuf> {
    let mut generated = Vec::new();
    let entries = match fs::read_dir(profile) {
        Ok(entries) => entries,
        Err(error) => {
            outcome.failed.push(CleanupFailure {
                action_id: record.action_id.clone(),
                checkout: record.path.clone(),
                error: format!("cannot list {}: {error}", profile.display()),
            });
            outcome.complete = false;
            return generated;
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => paths.push(entry.path()),
            Err(error) => {
                outcome.failed.push(CleanupFailure {
                    action_id: record.action_id.clone(),
                    checkout: record.path.clone(),
                    error: format!("cannot inventory {}: {error}", profile.display()),
                });
                outcome.complete = false;
            }
        }
    }
    paths.sort();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                outcome.failed.push(CleanupFailure {
                    action_id: record.action_id.clone(),
                    checkout: record.path.clone(),
                    error: format!("cannot stat {}: {error}", path.display()),
                });
                outcome.complete = false;
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            push_preserved(
                outcome,
                record,
                &path,
                format!("{} is a symlink and is never followed", path.display()),
            );
        } else if metadata.is_dir() && matches!(name, "deps" | "examples") {
            let nested = match fs::read_dir(&path) {
                Ok(entries) => entries,
                Err(error) => {
                    outcome.failed.push(CleanupFailure {
                        action_id: record.action_id.clone(),
                        checkout: record.path.clone(),
                        error: format!("cannot list {}: {error}", path.display()),
                    });
                    outcome.complete = false;
                    continue;
                }
            };
            let mut children = Vec::new();
            for entry in nested {
                match entry {
                    Ok(entry) => children.push(entry.path()),
                    Err(error) => {
                        outcome.failed.push(CleanupFailure {
                            action_id: record.action_id.clone(),
                            checkout: record.path.clone(),
                            error: format!("cannot inventory {}: {error}", path.display()),
                        });
                        outcome.complete = false;
                    }
                }
            }
            children.sort();
            for child in children {
                let child_metadata = match fs::symlink_metadata(&child) {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        outcome.failed.push(CleanupFailure {
                            action_id: record.action_id.clone(),
                            checkout: record.path.clone(),
                            error: format!("cannot stat {}: {error}", child.display()),
                        });
                        outcome.complete = false;
                        continue;
                    }
                };
                if child_metadata.file_type().is_symlink() {
                    push_preserved(
                        outcome,
                        record,
                        &child,
                        format!("{} is a symlink and is never followed", child.display()),
                    );
                } else if child_metadata.is_file() && known_dependency_artifact(&child) {
                    if is_cited(citations, checkout, &child) {
                        push_preserved(
                            outcome,
                            record,
                            &child,
                            format!(
                                "{} is cited by this action's durable evidence and is preserved",
                                child.display()
                            ),
                        );
                    } else {
                        generated.push(child);
                    }
                } else {
                    push_preserved(
                        outcome,
                        record,
                        &child,
                        format!(
                            "{} is not a recognized hashed Cargo dependency artifact, so cleanup refuses to guess about it",
                            child.display()
                        ),
                    );
                }
            }
        } else if metadata.is_file() && name == ".cargo-lock" {
            if is_cited(citations, checkout, &path) {
                push_preserved(
                    outcome,
                    record,
                    &path,
                    format!(
                        "{} is cited by this action's durable evidence and is preserved",
                        path.display()
                    ),
                );
            } else {
                generated.push(path);
            }
        } else {
            let cited = is_cited(citations, checkout, &path);
            let reason = if cited {
                format!(
                    "{} contains evidence cited by this action's durable records and is preserved",
                    path.display()
                )
            } else {
                format!(
                    "{} is not a recognized generated Cargo product, so cleanup refuses to guess about it",
                    path.display()
                )
            };
            push_preserved(outcome, record, &path, reason);
        }
    }
    generated
}

fn canonical_product_root(checkout: &Path, root: &Path) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(root)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "product root {} is a symlink and cleanup refuses to follow it",
        root.display()
    );
    ensure!(
        metadata.is_dir(),
        "product root {} is not a directory",
        root.display()
    );
    let canonical_checkout = fs::canonicalize(checkout)?;
    let canonical_root = fs::canonicalize(root)?;
    ensure!(
        canonical_root.starts_with(&canonical_checkout),
        "product root {} resolves outside its owned checkout {}",
        canonical_root.display(),
        canonical_checkout.display()
    );
    Ok(canonical_root)
}

fn prune_empty_ancestors(path: &Path, product_root: &Path) {
    let mut parent = path.parent();
    while let Some(directory) = parent {
        if !directory.starts_with(product_root) {
            break;
        }
        match fs::remove_dir(directory) {
            Ok(()) => parent = directory.parent(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                parent = directory.parent();
            }
            // A non-empty directory contains preserved or unselected content.
            Err(_) => break,
        }
    }
}

/// Remove only the entries of one owned checkout that are positively known
/// generated Cargo output.  Everything else is preserved and reported: an
/// unknown entry, a symlinked entry, or an entry the action's own durable
/// records cite is never deleted, and the candidate is left for the operator to
/// inspect rather than resolved by deleting the whole directory.
#[allow(clippy::too_many_arguments)]
fn clean_products(
    record: &CheckoutRecord,
    checkout: &Path,
    build_dir: &Path,
    dry_run: bool,
    outcome: &mut CleanupOutcome,
) {
    let citations = citation_text(build_dir, &record.action_id);
    for product in GENERATED_PRODUCTS {
        let root = checkout.join(product);
        match fs::symlink_metadata(&root) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                outcome.failed.push(CleanupFailure {
                    action_id: record.action_id.clone(),
                    checkout: record.path.clone(),
                    error: format!("cannot inspect product root {}: {error}", root.display()),
                });
                outcome.complete = false;
                continue;
            }
            Ok(metadata) if metadata.file_type().is_symlink() => {
                outcome.skipped.push(CleanupSkip {
                    action_id: Some(record.action_id.clone()),
                    checkout: record.path.clone(),
                    reason: format!(
                        "product root {} is a symlink and cleanup refuses to follow it",
                        root.display()
                    ),
                });
                continue;
            }
            Ok(_) => {}
        }
        let canonical_root = match canonical_product_root(checkout, &root) {
            Ok(root) => root,
            Err(error) => {
                outcome.skipped.push(CleanupSkip {
                    action_id: Some(record.action_id.clone()),
                    checkout: record.path.clone(),
                    reason: format!("{error:#}"),
                });
                continue;
            }
        };
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) => {
                outcome.failed.push(CleanupFailure {
                    action_id: record.action_id.clone(),
                    checkout: record.path.clone(),
                    error: format!("cannot list {}: {error}", root.display()),
                });
                outcome.complete = false;
                continue;
            }
        };
        let mut paths = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => paths.push(entry.path()),
                Err(error) => {
                    outcome.failed.push(CleanupFailure {
                        action_id: record.action_id.clone(),
                        checkout: record.path.clone(),
                        error: format!("cannot inventory {}: {error}", root.display()),
                    });
                    outcome.complete = false;
                }
            }
        }
        paths.sort();
        let mut generated = Vec::new();
        for path in paths {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    outcome.failed.push(CleanupFailure {
                        action_id: record.action_id.clone(),
                        checkout: record.path.clone(),
                        error: format!("cannot stat {}: {error}", path.display()),
                    });
                    outcome.complete = false;
                    continue;
                }
            };
            if metadata.file_type().is_symlink() {
                push_preserved(
                    outcome,
                    record,
                    &path,
                    format!("{} is a symlink and is never followed", path.display()),
                );
            } else if metadata.is_file() && matches!(name, "CACHEDIR.TAG" | ".rustc_info.json") {
                if is_cited(&citations, checkout, &path) {
                    push_preserved(
                        outcome,
                        record,
                        &path,
                        format!(
                            "{} is cited by this action's durable evidence and is preserved",
                            path.display()
                        ),
                    );
                } else {
                    generated.push(path);
                }
            } else if metadata.is_dir() && matches!(name, "debug" | "release") {
                generated.extend(collect_profile_products(
                    record, checkout, &path, &citations, outcome,
                ));
            } else {
                push_preserved(
                    outcome,
                    record,
                    &path,
                    format!(
                        "{} is not a recognized generated Cargo product, so cleanup refuses to guess about it",
                        path.display()
                    ),
                );
            }
        }
        for path in generated {
            // Re-establish canonical containment immediately before deleting a
            // selected file; neither a symlinked target root nor a changed
            // directory component is allowed to redirect cleanup.
            let safe_root = canonical_product_root(checkout, &root);
            let file = fs::symlink_metadata(&path);
            let file_is_contained = match (&safe_root, &file) {
                (Ok(current_root), Ok(metadata))
                    if *current_root == canonical_root
                        && !metadata.file_type().is_symlink()
                        && metadata.is_file() =>
                {
                    fs::canonicalize(&path).is_ok_and(|file| file.starts_with(current_root))
                }
                _ => false,
            };
            if !file_is_contained {
                outcome.skipped.push(CleanupSkip {
                    action_id: Some(record.action_id.clone()),
                    checkout: record.path.clone(),
                    reason: format!("{} no longer resolves inside the same canonical product root; cleanup skipped it", path.display()),
                });
                continue;
            }
            let bytes = file.as_ref().unwrap().len();
            if dry_run {
                outcome.removed.push(CleanupRemoval {
                    action_id: record.action_id.clone(),
                    checkout: record.path.clone(),
                    path: path.to_string_lossy().into_owned(),
                    bytes,
                    status: "would_remove",
                });
                outcome.reclaimed_bytes += bytes;
            } else {
                match fs::remove_file(&path) {
                    Ok(()) => {
                        outcome.removed.push(CleanupRemoval {
                            action_id: record.action_id.clone(),
                            checkout: record.path.clone(),
                            path: path.to_string_lossy().into_owned(),
                            bytes,
                            status: "removed",
                        });
                        outcome.reclaimed_bytes += bytes;
                        prune_empty_ancestors(&path, &root);
                    }
                    Err(error) => {
                        outcome.failed.push(CleanupFailure {
                            action_id: record.action_id.clone(),
                            checkout: record.path.clone(),
                            error: format!("cannot remove {}: {error}", path.display()),
                        });
                        outcome.complete = false;
                    }
                }
            }
        }
    }
}

/// Clean eligible generated products.  It never touches source, history,
/// evidence or unknown files, and a failure is recorded, not raised as a Build
/// outcome.
pub fn cleanup(store: &Store, effort: &Effort, dry_run: bool) -> Result<CleanupOutcome> {
    let project = store.project_for(effort)?;
    let _lock = ProjectLock::acquire(store, &project)?;
    let build_dir = store.phase_dir(effort, "build")?;
    let state_path = build_dir.join("state.json");
    if !state_path.is_file() {
        // Without Build state the controller recorded no ownership at all, so
        // there is nothing eligible; that is reported, not treated as a failure.
        return Ok(CleanupOutcome {
            effort: effort.id.clone(),
            dry_run,
            removed: Vec::new(),
            preserved: Vec::new(),
            skipped: vec![CleanupSkip {
                action_id: None,
                checkout: build_dir.to_string_lossy().into_owned(),
                reason: format!(
                    "effort {} has no Build state, so the controller recorded no checkout ownership",
                    effort.id
                ),
            }],
            failed: Vec::new(),
            reclaimed_bytes: 0,
            complete: true,
        });
    }
    let mut state: BuildState = read_json(&state_path)?;
    let mut outcome = CleanupOutcome {
        effort: effort.id.clone(),
        dry_run,
        removed: Vec::new(),
        skipped: Vec::new(),
        preserved: Vec::new(),
        failed: Vec::new(),
        reclaimed_bytes: 0,
        complete: true,
    };
    for record in state.checkouts.clone() {
        let checkout = PathBuf::from(&record.path);
        if let Err(error) = eligibility(&state, &record, &build_dir) {
            outcome.skipped.push(CleanupSkip {
                action_id: Some(record.action_id.clone()),
                checkout: record.path.clone(),
                reason: format!("{error:#}"),
            });
            continue;
        }
        clean_products(&record, &checkout, &build_dir, dry_run, &mut outcome);
    }
    outcome.complete = outcome.complete && outcome.preserved.is_empty();
    // The record is durable evidence of the attempt, kept separate from any
    // stop or verdict; the journal keeps every attempt.
    let summary = serde_json::to_value(&outcome)?;
    // Every attempt is journaled as its own observation; the attempt id keeps
    // repeated cleanup harmless without pretending a re-run did not happen.
    let operation_id = format!("cleanup-{}", crate::now_ms());
    store.append_journal(
        effort,
        "build_cleanup",
        Some(&operation_id),
        serde_json::json!({
            "dry_run": dry_run,
            "removed": outcome.removed.len(),
            "skipped": outcome.skipped.len(),
            "preserved": outcome.preserved.len(),
            "failed": outcome.failed.len(),
            "reclaimed_bytes": outcome.reclaimed_bytes,
        }),
    )?;
    state.cleanup = Some(summary);
    crate::save_state(&state_path, &state)?;
    Ok(outcome)
}
