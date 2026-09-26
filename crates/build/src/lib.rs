//! Deterministic checkpointed Build gate runner.
//!
//! Rust validates durable facts and routes the fixed Work → Review → Audit
//! loop. Providers own engineering judgment and return one structured result.

pub mod adapter;
pub mod controller;
pub mod packet;
pub mod state;

pub use controller::{BuildRequest, BuildResult, reset, run, scaffold, status};
pub use state::{
    BuildCompletion, BuildConfig, BuildPlan, BuildState, Gate, PlanPhase, RoleConfig, Scope,
    Session, Status, Stop, StopKind, UnblockContext,
};

pub const BUILD_PLAN_VERSION: u32 = state::PLAN_VERSION;
pub const BUILD_CONFIG_VERSION: u32 = state::CONFIG_VERSION;
pub const BUILD_STATE_VERSION: u32 = state::STATE_VERSION;

#[cfg(test)]
mod tests {
    use super::state::{BuildConfig, BuildPlan, load_config, load_plan, load_state};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_file(name: &str, contents: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("orchestrate-build-{nonce}-{name}"));
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn legacy_plan_and_config_versions_are_rejected_with_guidance() {
        let plan = temp_file("plan.json", r#"{"schema_version":2}"#);
        assert!(
            load_plan(&plan)
                .unwrap_err()
                .to_string()
                .contains("migration is not supported")
        );
        let config = temp_file("config.toml", "schema_version = 3\n");
        assert!(
            load_config(&config)
                .unwrap_err()
                .to_string()
                .contains("migration is not supported")
        );
        let state = temp_file("state.json", r#"{"schema_version":3}"#);
        assert!(
            load_state(&state)
                .unwrap_err()
                .to_string()
                .contains("Historical state is intentionally unsupported")
        );
        let _ = fs::remove_file(plan);
        let _ = fs::remove_file(config);
        let _ = fs::remove_file(state);
    }

    #[test]
    fn new_plan_and_config_schemas_deserialize_exact_role_fields() {
        let plan = BuildPlan {
            schema_version: 3,
            reconciled: orchestrate_contracts::ArtifactRef {
                kind: orchestrate_contracts::ArtifactKind::ReconciledDiscovery,
                artifact_id: "r".into(),
                digest: "d".into(),
            },
            detailed_plan: "detail.md".into(),
            phases: vec![super::PlanPhase {
                id: "P1".into(),
                tasks: vec!["task".into()],
                requirement_ids: vec!["R-1".into()],
            }],
        };
        assert_eq!(plan.schema_version, 3);
        let config = BuildConfig {
            schema_version: 4,
            worker: super::RoleConfig {
                adapter: "codex".into(),
                model: Some("native-model".into()),
                args: Some(vec!["--search".into()]),
            },
            reviewer: super::RoleConfig {
                adapter: "claude".into(),
                model: None,
                args: None,
            },
            unblocker: None,
        };
        assert_eq!(config.worker.model.as_deref(), Some("native-model"));
    }
}
