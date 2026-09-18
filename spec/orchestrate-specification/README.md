# Orchestrate — Consolidated specification package

**Version:** 0.1, September 17, 2026  
**Status:** Proposed product specification; no Orchestrate implementation or live qualification is claimed.

```text
Discovery → finalized public artifact → Consensus → Agreement
                                                    ↓
                                                   Build
                                                    ↓
                                            exact implementation
                                                    ↓
                                                   Audit
```

## Start here

[ORCHESTRATE-SPEC.md](ORCHESTRATE-SPEC.md) is the primary product specification. It contains the goal, scope, authority model, 94 numbered requirements, 188 explicit acceptance statements, public artifact contracts, lifecycle rules, phase behavior, failure handling, and release criteria.

[VERIFICATION.md](VERIFICATION.md) maps those requirements to 147 concrete test cases, four test layers, six detailed regression recipes, a no-change journey, and honest qualification rules.

[TECHNICAL-GUIDANCE.md](TECHNICAL-GUIDANCE.md) provides subordinate implementation guidance for a new Rust project: crate boundaries, source snapshots, public manifests, bounded execution, thin host skills, and the test harness.

[requirements.json](requirements.json) is a generated machine-readable index of requirements, acceptance statements, and cases. The product Markdown is authoritative.

## Important scope decisions

The user-approved foundations remain one new Rust product/CLI, explicit thin phase skills, external orchestration storage, frozen committed source, identifiable host/model/effort metadata, and the four-stage pipeline. Build execution is deferred; the external handoff is specified and testable now.

Section 2.2 of the product spec clearly labels the new draft’s policy resolutions, including complete three-slot participation for an eligible Agreement, explicit adoption, descriptive confidence, and bounded retries. These are proposed specification choices, not retrospective claims of separate user approval.

Input exclusion is required. Enforced host-level peer access restriction is a separately qualified capability. A valid artifact is not proof of correct reasoning; the suite separately tests mechanics, host compatibility, and semantic quality.

No old repository, schema, score, workflow, or test harness has compatibility authority over this new product. Do not implement Taskledger, Flow, or a universal framework simply because their source was reviewed.

## Document checks versus application tests

`DOCUMENT-VALIDATION.json` records package consistency checks such as unique IDs and complete references. It does not record application tests. Every product test/model trial remains NOT_EXECUTED until actual implementation qualification.
