use cowiki_knowledge::governance::*;

#[test]
fn paired_semantic_controls() {
    for (a, b, r) in [
        (
            "The quota is 4 requests.",
            "The quota is 8 requests.",
            Relation::Contradiction,
        ),
        (
            "Queue capacity is at most 5 jobs.",
            "Queue capacity is at least 5 jobs.",
            Relation::Contradiction,
        ),
        (
            "Policy takes effect 2026-01-01.",
            "Policy takes effect 2026-02-01.",
            Relation::Contradiction,
        ),
        (
            "The gateway must not accept retries.",
            "The gateway must accept retries.",
            Relation::Contradiction,
        ),
        (
            "The queue is enabled.",
            "The queue is disabled.",
            Relation::Contradiction,
        ),
        (
            "The policy window is 24 hours.",
            "The policy window is 1 day.",
            Relation::Entailment,
        ),
        (
            "Scope Europe quota is 5 requests.",
            "Scope Asia quota is 9 requests.",
            Relation::ScopeDifference,
        ),
        (
            "The quota is 5 requests.",
            "Version 2 the quota is 5 requests.",
            Relation::Contradiction,
        ),
        (
            "The disk quota is 5 bytes.",
            "The ledger posts after 9 minutes.",
            Relation::Neutral,
        ),
        (
            "Under the retired policy: quota is 7.",
            "Quota is 7.",
            Relation::ScopeDifference,
        ),
    ] {
        assert_eq!(rule_relation(a, b), r, "{a} / {b}");
    }
}

fn cited(path: &str, text: &str) -> Claim {
    Claim {
        location: Citation {
            path: path.into(),
            snapshot: digest(text),
            first: 1,
            last: 1,
        },
        text: text.into(),
        heading: "Policy".into(),
    }
}
#[test]
fn a_later_cited_verified_decision_can_reinstate_a_rule() {
    let text = "The payment window lasts 4 days.";
    let mut candidate = GovernanceInput {
        path: "note.md".into(),
        request: String::new(),
        base: String::new(),
        candidate: text.into(),
        history: vec![],
    };
    let old = Evidence {
        claim: cited("archive.md", text),
        status: "retired".into(),
        human_verified: false,
        scope: "default".into(),
        version: "v1".into(),
        sources: vec![],
        updated_at: Some("2025-01-01".into()),
        supersedes: None,
    };
    let decision = Evidence {
        claim: cited("decision.md", text),
        status: "active".into(),
        human_verified: true,
        updated_at: Some("2026-01-01".into()),
        supersedes: Some("archive.md".into()),
        ..old.clone()
    };
    let mut packet = EvidenceBundle {
        input: candidate.clone(),
        as_of: "2026-11-23".into(),
        changed: vec![cited("note.md", text)],
        evidence: vec![old, decision],
        retrieval_calls: 2,
        retrieval_tokens: 10,
    };
    let verdicts = RuleConflictDetector.classify(&packet).unwrap();
    assert!(interpret(&packet, &verdicts)
        .unwrap()
        .iter()
        .any(|i| i.code == "reintroduces_retired"));
    candidate.candidate = format!("---\nsources: [decision.md]\n---\n{text}");
    packet.input = candidate;
    assert!(!interpret(&packet, &verdicts)
        .unwrap()
        .iter()
        .any(|i| i.code == "reintroduces_retired"));
    packet.evidence[1].human_verified = false;
    assert!(interpret(&packet, &verdicts)
        .unwrap()
        .iter()
        .any(|i| i.code == "reintroduces_retired"));
}
