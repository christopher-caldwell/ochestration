//! Opt-in guide/model evaluation entry point.  It intentionally never runs in the deterministic suite.

#[test]
#[ignore = "requires explicitly selected external model providers and manual artifact review"]
fn semantic_evaluation_environment_is_explicit() {
    for name in [
        "ORCHESTRATE_DISCOVERY_PROVIDER",
        "ORCHESTRATE_CONSENSUS_PROVIDER",
        "ORCHESTRATE_AUDIT_PROVIDER",
    ] {
        let provider = std::env::var(name)
            .unwrap_or_else(|_| panic!("set {name} to run semantic evaluations"));
        assert!(
            std::path::Path::new(&provider).is_absolute(),
            "{name} must be an absolute provider path"
        );
        println!("{name}={provider}");
    }
    println!(
        "Run the documented fixture categories in evals/README.md and inspect generated public artifacts manually."
    );
}
