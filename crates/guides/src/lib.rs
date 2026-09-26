//! Compile-time Orchestrate instructions.
//!
//! Skills only dispatch. This crate is the one copy of each guide, embedded into
//! the `orchestrate` binary with `include_str!`. Model-facing output and provenance
//! digests both use these strings.

pub const PREP_DISCOVERY_TICKET: &str =
    include_str!("../resources/guides/prep-discovery-ticket.md");
pub const PREP_DISCOVERY_FREEFORM: &str =
    include_str!("../resources/guides/prep-discovery-freeform.md");
pub const DISCOVERY: &str = include_str!("../resources/guides/discovery.md");
pub const RECONCILE: &str = include_str!("../resources/guides/reconcile.md");
pub const BUILD: &str = include_str!("../resources/guides/build.md");
pub const AUDIT: &str = include_str!("../resources/guides/audit.md");

/// Internal Build-controller roles. They are not installed skills.
pub const WORK: &str = include_str!("../resources/guides/work.md");
pub const REVIEW: &str = include_str!("../resources/guides/review.md");
pub const FINAL_AUDIT: &str = include_str!("../resources/guides/final-audit.md");
pub const UNBLOCK: &str = include_str!("../resources/guides/unblock.md");
/// Optional advisory whole-implementation once-over. Off unless configured.
pub const ONCE_OVER: &str = include_str!("../resources/guides/once-over.md");
/// The one durable-output contract for roles whose permission prevents them
/// from writing their evidence files.  It is materialized beside a role's own
/// guide and named by `action.json`, so the roles share one copy of it.
pub const EVIDENCE_OUTPUT: &str = include_str!("../resources/guides/evidence-output.md");

/// User-installed actions. Sorted by name. `unblock`, `work`, `review`, and
/// `final-audit` are deliberately absent: the Build controller materializes those
/// guides itself.
pub const ACTIONS: &[(&str, &str)] = &[
    ("audit", AUDIT),
    ("build", BUILD),
    ("discovery", DISCOVERY),
    ("prep-discovery-freeform", PREP_DISCOVERY_FREEFORM),
    ("prep-discovery-ticket", PREP_DISCOVERY_TICKET),
    ("reconcile", RECONCILE),
];

/// Guides the Build controller shows to a role. Not installed as skills.
pub const INTERNAL: &[(&str, &str)] = &[
    ("final-audit", FINAL_AUDIT),
    ("once-over", ONCE_OVER),
    ("review", REVIEW),
    ("unblock", UNBLOCK),
    ("work", WORK),
];

pub fn guide(action: &str) -> Option<&'static str> {
    ACTIONS
        .iter()
        .chain(INTERNAL)
        .find(|(name, _)| *name == action)
        .map(|(_, body)| *body)
}

pub mod templates {
    pub const PLAN_JSON: &str = include_str!("../resources/templates/plan.json");
    pub const CONFIG_TOML: &str = include_str!("../resources/templates/config.toml");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_guide_is_nonempty_and_unique() {
        for registry in [ACTIONS, INTERNAL] {
            let names: Vec<_> = registry.iter().map(|(name, _)| *name).collect();
            let mut sorted = names.clone();
            sorted.sort();
            assert_eq!(names, sorted, "guide registry must be sorted");
        }
        let mut names = Vec::new();
        for (name, body) in ACTIONS.iter().chain(INTERNAL) {
            assert_eq!(guide(name), Some(*body));
            assert!(body.trim().len() > 40, "{name} guide is empty");
            assert!(body.ends_with('\n'), "{name} guide must end in a newline");
            names.push(*name);
        }
        names.sort();
        names.dedup();
        assert_eq!(names.len(), ACTIONS.len() + INTERNAL.len());
    }

    #[test]
    fn public_actions_are_the_installed_skills_only() {
        let names: Vec<_> = ACTIONS.iter().map(|(name, _)| *name).collect();
        assert!(!names.contains(&"unblock"));
        assert!(!names.contains(&"work"));
        assert!(!names.contains(&"review"));
        assert!(!names.contains(&"final-audit"));
        assert!(!names.contains(&"work-follow-up"));
        assert!(!names.contains(&"review-follow-up"));
    }

    #[test]
    fn build_roles_are_directed_to_the_binding_requirement_projection() {
        for (name, guide) in [
            ("work", WORK),
            ("review", REVIEW),
            ("unblock", UNBLOCK),
            ("final-audit", FINAL_AUDIT),
        ] {
            assert!(
                guide.contains("binding_requirements"),
                "{name} lacks the complete requirement view"
            );
            assert!(
                guide.contains("historical"),
                "{name} does not distinguish historical goal text"
            );
        }
        for (name, guide) in [("work", WORK), ("review", REVIEW), ("unblock", UNBLOCK)] {
            assert!(
                guide.contains("phase_authority"),
                "{name} lacks phase scope"
            );
        }
    }

    #[test]
    fn templates_are_embedded() {
        assert!(templates::PLAN_JSON.contains("\"schema_version\": 2"));
        assert!(templates::CONFIG_TOML.contains("schema_version = 3"));
        assert!(templates::CONFIG_TOML.contains("model_strength"));
        assert!(templates::CONFIG_TOML.contains("workspace_write"));
    }

    #[test]
    fn build_instructions_keep_the_human_cli_boundary() {
        assert!(BUILD.contains("does not authorize the chat to run any Orchestrate CLI command"));
        assert!(BUILD.contains("never runs `orchestrate build guide`"));
        assert!(BUILD.contains("own the complete Work/Review/Unblock/Audit loop"));
        assert!(!BUILD.contains("`$build` invokes the driver after preparation"));
        let dispatcher = include_str!("../../../skills/build/SKILL.md");
        assert!(dispatcher.contains("`$build` is not authorization"));
        assert!(dispatcher.contains("Do not execute `orchestrate build guide`"));
        assert!(!dispatcher.contains("You must run `orchestrate build guide`"));
    }
}
