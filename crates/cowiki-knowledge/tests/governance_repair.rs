use cowiki_knowledge::governance::*;
fn packet(candidate: &str, supported: &str) -> EvidenceBundle {
    let input = GovernanceInput {
        path: "note.md".into(),
        base: String::new(),
        candidate: format!(" {candidate}\r\nKeep the invoice number.\r\n"),
        request: String::new(),
        history: vec![],
    };
    let changed = changed_claims(&input).unwrap();
    let evidence = Evidence {
        claim: Claim {
            heading: "".into(),
            text: supported.into(),
            location: Citation {
                path: "source.md".into(),
                snapshot: digest(supported),
                first: 1,
                last: 1,
            },
        },
        status: "active".into(),
        human_verified: true,
        scope: "default".into(),
        version: "v1".into(),
        sources: vec![],
        updated_at: None,
        supersedes: None,
    };
    EvidenceBundle {
        input,
        changed,
        evidence: vec![evidence],
        as_of: "2026-11-23".into(),
        links: Default::default(),
        retrieval_calls: 2,
        retrieval_tokens: 30,
    }
}
#[test]
fn narrow_repairs_preserve_neighbor_and_require_exact_preimage() {
    for (bad, good) in [
        ("The capacity is 8 jobs.", "The capacity is 4 jobs."),
        (
            "The capacity is 4 jobs.",
            "Version 2 the capacity is 4 jobs.",
        ),
    ] {
        let p = packet(bad, good);
        let result = maintain(&p, &RuleConflictDetector).unwrap();
        assert_eq!(result.rounds, 1);
        assert!(result.unresolved.is_empty());
        assert_eq!(
            result.final_text,
            format!(" {good}\r\nKeep the invoice number.\r\n")
        );
        assert!(result.proposals[0]
            .patch
            .preview("note.md", "concurrent change")
            .is_err());
        assert!(result.proposals[0]
            .patch
            .preview("another.md", &p.input.candidate)
            .is_err());
    }
}
#[test]
fn unresolved_when_sources_disagree() {
    let mut p = packet("The capacity is 8 jobs.", "The capacity is 4 jobs.");
    let mut other = p.evidence[0].clone();
    other.claim.location.path = "other.md".into();
    other.claim.text = "The capacity is 6 jobs.".into();
    p.evidence.push(other);
    let result = maintain(&p, &RuleConflictDetector).unwrap();
    assert_eq!(result.rounds, 0);
    assert_eq!(result.unresolved.len(), 2);
}
