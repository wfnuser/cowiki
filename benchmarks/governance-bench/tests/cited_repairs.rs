#![cfg(feature = "remote-verifier")]
use cowiki_governance_bench::{audit_case_text, Family};
use cowiki_knowledge::governance::{remote::*, *};
use serde_json::{json, Value};

fn packet(old: &str, source: &str) -> EvidenceBundle {
    let input = GovernanceInput {
        path: "page.md".into(),
        base: "Keep this sentence.\r\n".into(),
        candidate: format!("  {old}\r\nKeep this sentence.\r\n"),
        request: "edit".into(),
        history: vec![],
    };
    let evidence = Evidence {
        claim: Claim {
            text: source.into(),
            heading: String::new(),
            location: Citation {
                path: "source.md".into(),
                snapshot: digest(source),
                first: 1,
                last: 1,
            },
        },
        human_verified: true,
        status: "active".into(),
        scope: "default".into(),
        version: "v1".into(),
        updated_at: None,
        sources: vec![],
        supersedes: None,
    };
    EvidenceBundle {
        changed: changed_claims(&input).unwrap(),
        input,
        evidence: vec![evidence],
        retrieval_calls: 2,
        retrieval_tokens: 10,
        as_of: "2026-11-23".into(),
        links: Default::default(),
    }
}
fn repair(before: &str, after: &str) -> Value {
    json!({"candidate":0,"evidence":0,"support":0,"before":before,"after":after,"evidence_quote":after})
}
fn response(p: &EvidenceBundle, repairs: Value) -> Response {
    Response {
        candidate_digest: digest(&p.input.candidate),
        key: String::new(),
        usage: Usage::default(),
        answer: serde_json::from_value(
            json!({"judgments":[{"candidate":0,"evidence":0,"relation":"contradiction"}],
            "findings":[],"edits":[],"repairs":repairs}),
        )
        .unwrap(),
    }
}
fn run(p: &EvidenceBundle, reply: &Response, both: bool) -> MaintenanceResult {
    let family = Family {
        id: "independent".into(),
        split: "dev".into(),
        source_paths: vec![],
        pages: p
            .evidence
            .iter()
            .map(|e| (e.claim.location.path.clone(), e.claim.text.clone()))
            .collect(),
    };
    verified_maintenance_with_audit(p, reply, both, &|text| {
        audit_case_text(&family, &p.input.path, text, &p.as_of)
    })
    .unwrap()
}

#[test]
fn accepted_span_keeps_adjacent_sentences_whitespace_and_original_precondition() {
    let p = packet(
        "The capacity is 8 jobs. Keep the label.",
        "The capacity is 4 jobs.",
    );
    let r = response(&p, json!([repair("8 jobs", "4 jobs")]));
    for both in [false, true] {
        let out = run(&p, &r, both);
        assert_eq!(out.repair_checks[0].outcome, "accepted");
        assert!(out.unresolved.is_empty());
        assert_eq!(
            out.final_text,
            "  The capacity is 4 jobs. Keep the label.\r\nKeep this sentence.\r\n"
        );
        let patch = &out.proposals[0].patch;
        assert_eq!(patch.edits[0].before, "8");
        assert_eq!(
            patch.preview("page.md", &p.input.candidate).unwrap(),
            out.final_text
        );
        assert_eq!(
            out.proposals[0].evidence,
            vec![p.evidence[0].claim.location.clone()]
        );
    }
}

#[test]
fn rejected_attachment_never_erases_the_finding_or_falls_back_to_a_rule_repair() {
    let p = packet("The capacity is 8 jobs.", "The capacity is 4 jobs.");
    for (attempt, reason) in [
        (repair("unseen", "4"), "preimage_span"),
        (repair("8", "6"), "quote_outside_source"),
        (
            repair("8", "4\nchange another line"),
            "outside_finding_span",
        ),
        (
            json!({"candidate":0,"evidence":0,"support":99,"before":"8","after":"4","evidence_quote":"4"}),
            "support_citation",
        ),
        (json!({"candidate":0,"before":"8"}), "malformed_repair"),
    ] {
        let out = run(&p, &response(&p, json!([attempt])), false);
        assert_eq!(out.repair_checks[0].outcome, reason);
        assert_eq!(out.unresolved, out.initial);
        assert!(out.proposals.is_empty());
        assert_eq!(out.final_text, p.input.candidate);
    }
    let mut r = response(&p, json!([repair("8", "4")]));
    r.candidate_digest = digest("different input file");
    assert_eq!(
        run(&p, &r, false).repair_checks[0].outcome,
        "preimage_digest"
    );
    let duplicated = response(&p, json!([repair("8", "4"), repair("8", "5")]));
    assert!(run(&p, &duplicated, false)
        .repair_checks
        .iter()
        .all(|v| v.outcome == "competing_repairs"));
}

#[test]
fn checks_entire_repaired_assertion_and_preserves_independent_content() {
    let misleading = packet(
        "The cache is enabled everywhere.",
        "The cache is disabled only on Linux.",
    );
    let out = run(
        &misleading,
        &response(&misleading, json!([repair("enabled", "disabled")])),
        false,
    );
    assert_eq!(out.repair_checks[0].outcome, "unsupported_replacement");
    let p = packet(
        "Until release 2 the capacity is 8. Keep the label.",
        "The capacity is 4.",
    );
    let r = response(
        &p,
        json!([repair(&p.changed[0].text, "The capacity is 4.")]),
    );
    assert_eq!(
        run(&p, &r, false).repair_checks[0].outcome,
        "unsupported_replacement"
    );
    let p = packet("Mode 8 uses 8 ports.", "Mode 8 uses 4 ports.");
    assert_eq!(
        run(&p, &response(&p, json!([repair("8", "4")])), false).repair_checks[0].outcome,
        "ambiguous_span"
    );
}

#[test]
fn freshly_indexed_scratch_rejects_new_link_and_new_rule_conflict() {
    let p = packet("Read the old policy.", "Read [policy](missing.md).");
    let r = response(
        &p,
        json!([repair("the old policy", "[policy](missing.md)")]),
    );
    assert_eq!(
        run(&p, &r, false).repair_checks[0].outcome,
        "new_structural_issue"
    );
    let mut p = packet("The capacity is 8 jobs.", "The capacity is 4 jobs.");
    let mut alternative = p.evidence[0].clone();
    alternative.claim.location.path = "independent.md".into();
    alternative.claim.text = "The capacity is 8 jobs.".into();
    p.evidence.push(alternative);
    let r = response(&p, json!([repair("8", "4")]));
    assert_eq!(
        run(&p, &r, false).repair_checks[0].outcome,
        "new_rule_issue"
    );
}

#[test]
fn retired_assertion_requires_an_active_linked_correction() {
    let mut p = packet("The capacity is 8 jobs.", "The capacity is 8 jobs.");
    p.evidence[0].status = "retired".into();
    let mut active = p.evidence[0].clone();
    active.claim.location.path = "replacement.md".into();
    active.claim.text = "The capacity is 4 jobs.".into();
    active.status = "active".into();
    active.supersedes = Some("source.md".into());
    p.evidence.push(active);
    let mut r = response(
        &p,
        json!([{"candidate":0,"evidence":0,"support":1,"before":"8","after":"4","evidence_quote":"4"}]),
    );
    r.answer.judgments[0].relation = Relation::Entailment;
    assert_eq!(run(&p, &r, false).repair_checks[0].outcome, "accepted");
    p.evidence[1].supersedes = None;
    assert_eq!(
        run(&p, &r, false).repair_checks[0].outcome,
        "unrelated_source"
    );
}

#[test]
fn verbatim_quote_is_required_in_both_the_original_authority_and_replacement() {
    let p = packet("The capacity is 8 jobs.", "The capacity is 4 jobs.");
    for (quote, reason) in [
        ("four", "quote_outside_source"),
        ("jobs", "quote_outside_replacement"),
        ("", "quote_outside_source"),
    ] {
        let mut edit = repair("8", "4");
        edit["evidence_quote"] = json!(quote);
        let out = run(&p, &response(&p, json!([edit])), false);
        assert_eq!(out.repair_checks[0].outcome, reason);
        assert_eq!(out.unresolved, out.initial);
    }
}
