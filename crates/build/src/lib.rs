//! Deterministic checkpointed Build gate runner.
//!
//! Rust validates durable facts and routes the fixed Work → Review → Audit
//! loop. Providers own engineering judgment and return one structured result.

#[cfg(test)]
extern crate self as orchestrate_build;

pub mod adapter;
pub mod controller;
pub mod observation;
pub mod packet;
pub mod state;

pub use controller::{
    BuildRequest, BuildResult, reset, run, run_with_invoker_and_observer, run_with_observer,
    scaffold, status,
};
pub use observation::{ActionOutcome, BuildEvent, BuildObservation, BuildObserver, FinishedAction};
pub use state::{
    BuildCompletion, BuildConfig, BuildPlan, BuildState, Gate, RoleConfig, Scope, Status, Stop,
    StopKind, UnblockContext,
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
        let plan = temp_file("plan.json", r#"{"schema_version":3}"#);
        assert!(
            load_plan(&plan)
                .unwrap_err()
                .to_string()
                .contains("migration is not supported")
        );
        let config = temp_file("config.toml", "schema_version = 4\n");
        assert!(
            load_config(&config)
                .unwrap_err()
                .to_string()
                .contains("migration is not supported")
        );
        let state = temp_file("state.json", r#"{"schema_version":4}"#);
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
    fn plan_and_config_schema_versions_are_independent_and_current() {
        assert_eq!(super::state::PLAN_VERSION, 4);
        assert_eq!(super::state::STATE_VERSION, 5);
        assert_eq!(super::state::CONFIG_VERSION, 5);
        let plan = BuildPlan {
            schema_version: 4,
            reconciled: orchestrate_contracts::ArtifactRef {
                kind: orchestrate_contracts::ArtifactKind::ReconciledDiscovery,
                artifact_id: "r".into(),
                digest: "d".into(),
            },
            phases: vec!["phase_01".into()],
        };
        assert_eq!(plan.schema_version, 4);
        let config = BuildConfig {
            schema_version: 5,
            worker: super::RoleConfig {
                adapter: "codex".into(),
                model: Some("native-model".into()),
                effort: Some("high".into()),
                args: Some(vec!["--search".into()]),
            },
            reviewer: super::RoleConfig {
                adapter: "claude".into(),
                model: None,
                effort: None,
                args: None,
            },
            unblocker: None,
        };
        assert_eq!(config.worker.model.as_deref(), Some("native-model"));
        assert_eq!(config.worker.effort.as_deref(), Some("high"));
    }

    #[test]
    fn schema_five_config_loads_optional_effort_for_each_role() {
        let config = temp_file(
            "config.toml",
            r#"schema_version = 5

[worker]
adapter = "codex"

[reviewer]
adapter = "claude"
effort = "ultracode"

[unblocker]
adapter = "cursor"
effort = "High"
"#,
        );
        let loaded = load_config(&config).unwrap();
        assert_eq!(loaded.worker.effort, None);
        assert_eq!(loaded.reviewer.effort.as_deref(), Some("ultracode"));
        // Effort vocabulary and canonical-case validation belong to adapters.
        assert_eq!(loaded.unblocker.unwrap().effort.as_deref(), Some("High"));
        let _ = fs::remove_file(config);
    }
}
