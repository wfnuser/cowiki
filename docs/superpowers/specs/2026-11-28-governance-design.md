# Local governance review for OKF Spaces

**Date** 2026-11-28 · **Status** Implemented in #180 for #169. The sections marked **Planned (v2)** describe proposed interfaces.

## 1. Problem, scope and non-goals

An edit can be valid Markdown and still restore a withdrawn instruction, drop a platform restriction or point readers at an obsolete page. OKF v0.3 supplies lifecycle and trust metadata (#167), and the schema-v4 hybrid index locates passages (#168). Governance gives a reviewer the changed claim, the evidence for challenging it and, where the correction is sufficiently narrow, a diff they can stage separately.

The desktop reviews **Current Draft** and **local Agent Changes**. Opening Review costs zero model tokens and requires no API key. A reviewer can defer any finding. Review does not edit Draft, merge, publish, decide new source authority or invent a review date. Cloud merge requests are outside v1. The embedded desktop MCP tool list stays read-only. [Desktop MCP](../../../web/src-tauri/src/mcp.rs)

## 2. Three layers and two snapshots

| Layer | Responsibility |
|---|---|
| Rust knowledge engine | `GovernanceInput` carries path, request, base text, candidate text and ordered revisions. `SnapshotIndex::locate` retrieves evidence, `ConflictDetector::classify` produces relations, and `interpret` turns applicable relations into issues. `audit_space` reports structural findings and redirect proposals. The engine has no Tauri dependency. [Governance module](../../../crates/cowiki-knowledge/src/governance/mod.rs) |
| Desktop Git adapter | `LocalEngine::review_governance` resolves the trees and calls `build_review`. For Draft the base is the latest checkpoint or HEAD. An Agent Change uses its creation base ref. An open candidate is a snapshot of the working tree. A closed change uses its branch tree and is read-only. `history` walks up to 32 first-parent commits, skips repeated blobs and takes the first declared source of each revision. [Git orchestration](../../../web/src-tauri/src/local_engine/agent_changes/governance_review.rs) |
| Review UI | `LocalReviewDetail` opens `GovernanceReviewPanel` lazily, keyed by Space and target. Returning from File changes retains the report and checked repairs. `DiffView` shows the proposed transition. Refresh obtains fresh IDs and clears selections. Leaving a finding for human review makes no staging call. [Review components](../../../web/src/components/review/) |

`build_review` materializes the two Git trees into separate temporary directories. It copies regular Markdown blobs and represents other regular files as empty entries for link lookup. It skips symlinks and admits the reserved raw-source subtree. One in-memory SQLite connection holds two indexes named `base` and `candidate`, rebuilt with `LocalHashProjectionEmbedder`. The structural audit sees the **whole candidate**, including unchanged pages. Semantic checks run on eligible changed concepts and look up evidence **only in base**. Otherwise an edit to a claim and its supposed source in the same change could verify itself. Markdown bytes in Git and the local worktree remain authoritative. The database and excerpts are derived from those bytes. [Schema and hybrid search](../../../crates/cowiki-knowledge/src/knowledge_index.rs)

## 3. Data contracts

### OKF fields

| Frontmatter | Governance interpretation |
|---|---|
| `status` | Defaults to `active`. Valid values also include `draft`, `deprecated` and `retired`. Ordinary and retired retrieval use separate budgets. An active claim can supply a correction. A retired claim can supply evidence of withdrawal. |
| `trust_tier`, `verified`, `generated` | The tiers are `human_verified`, `human_authored`, `agent_draft` and `unverified`. An absent tier derives from `verified` (default false). Absent `generated` derives from the agent-draft tier. Evidence qualifies as human-verified when the flag or tier says so **and** it is neither generated nor agent-draft. |
| `stale_after`, `redirect_to`, `sources` | A valid ISO expiry earlier than the audit date is stale. A redirect names a local destination. Source paths supply provenance for generated pages, historical corrections and explicit reinstatement decisions. |
| `scope`, `version` | Defaults are `default` and `v1`. Conflicting explicit scopes suppress a finding. A version mismatch suppresses active-evidence findings. The retired check can span versions. Changing scope or version makes the candidate claims eligible for comparison again. |
| `updated_at`, optional `supersedes` | The latter is a scalar path read from base YAML when collecting evidence. Reinstatement requires the candidate to cite an active, human-verified decision naming the withdrawn path, matching its scope and matching the new assertion ignoring case. Both dates must be valid, with the decision strictly later than the original and no later than the audit date. |

The parser, evidence lookup and decision guard implement different parts of this contract. [OKF metadata](../../../crates/cowiki-knowledge/src/okf.rs) · [Evidence lookup](../../../crates/cowiki-knowledge/src/governance/mod.rs) · [Decision guard](../../../crates/cowiki-knowledge/src/governance/conflict.rs)

### Relations and issues

A `Verdict` identifies a candidate/evidence pair by zero-based indices. Its five relations are `contradiction` (incompatible assertions), `entailment` (the evidence supports the candidate), `neutral` (no detected implication), `scope_difference` (different applicability or historical context) and `insufficient` (too little comparable evidence). Entailment of a retired claim can therefore raise a critical issue. A `Citation` carries a relative path, SHA-256 of the full Markdown and inclusive one-based line numbers displayed as `[page.md:Lstart-Lend]`.

| Issue code | Desktop severity and trigger |
|---|---|
| `contradicts_verified` | Critical. The candidate contradicts an applicable active, human-verified claim. |
| `reintroduces_retired` | Critical. A current assertion entails a retired claim. Recognized historical quotations and qualifying later decisions are exempt. |
| `revision_regression` | Warning. Adjacent revisions show an earlier assertion and a source-backed correction. The candidate restores that assertion and contradicts the verified source. A retired finding on the same claim takes precedence. |
| `duplicate_overlap` | Info. An entailed claim of at least six words occurs on another active page under the same non-empty heading. |
| `inbound_redirect`, `broken_link` | Warning. A live internal link resolves to a redirect stub or has no destination, respectively. |
| `broken_redirect`, `redirect_cycle` | Critical. A redirect field has no live target, or the target chain revisits a page. |
| `expired_stale_after`, `missing_generated_sources` | Warning. A review date has passed, or a generated concept has an empty source list. Both need human decisions. |
| `unverified_agent_draft` | Info. An agent-draft concept awaits human verification. |

`GovernanceAuditReport` also retains the validator's conformance codes. These are `missing_root_index`, `unreadable_root_index`, `symlink_root_index`, `unreadable_utf8`, `missing_okf_version`, `unsupported_okf_version`, `index_missing_heading`, `unclosed_generated_index_marker`, `invalid_log_format`, `missing_frontmatter`, `invalid_frontmatter`, `missing_required_field`, `invalid_status`, `invalid_trust_tier`, `invalid_stale_after` and `trust_reset_violation`. Validator errors map to critical, warnings to warning. A file-level diagnostic may have no readable span. [Audit and triage](../../../crates/cowiki-knowledge/src/governance/audit.rs)

### Desktop IPC and verifier wire format

- `local_review_governance({spaceSlug, changeId})` accepts a nullable change ID for Draft. The native command supplies today's UTC date. It returns `GovernanceReview { asOf, findings, proposals, warnings, canStage }`. Findings have an ID, code, severity, message, path, optional citations, excerpts and a nullable verdict. Proposals carry an opaque ID, rationale, finding IDs and a file diff. The native selection is excluded from serialization. The adapter reconstructs it at staging.
- `stage_governance_as_new_change({spaceSlug, changeId, asOf, proposalIds})` recomputes the report and resolves the selected IDs. An ID binds the Space, review target, base commit, candidate tree, date and proposal. Success returns a new `AgentChange`. A staging failure followed by successful cleanup returns `{reason}`. Invalid selections, closed or changed sources, and cleanup failures reject the command. [IPC wrappers](../../../web/src/local-api.ts) · [Tauri handlers](../../../web/src-tauri/src/lib.rs)
- The verifier reply has five required arrays and disallows extra properties. `judgments` contains the two indices and relation. A `repairs` entry links the pair to a support evidence index and supplies `before`, `after` and `evidence_quote`. Zero repair entries are valid. `structural` supplies a code, candidate line, source path and source line. The schema also carries `findings` (pair and code) and `edits` (candidate index and before/after) for the comparison protocol. The strict verification request and the single-pass JSON-object request have distinct contracts. [Reply schema](../../../crates/cowiki-knowledge/src/governance/verifier-reply.schema.json) · [Wire requests](../../../crates/cowiki-knowledge/src/governance/remote.rs)

## 4. Repair safety and the coverage trade-off

1. **Prove the destination.** Redirect repair follows at most 16 stubs to an existing eligible concept. Cycles, ambiguous bare wikilinks and destinations that fail to resolve back to the chosen page produce no rewrite. Only the destination bytes change. Labels, fragments, inline and fenced code, and frontmatter survive. [Redirect proposals](../../../crates/cowiki-knowledge/src/governance/redirects.rs) · [Link syntax](../../../crates/cowiki-knowledge/src/link_syntax.rs)
2. **Bind the write to the preview.** `FilePatch::preview` checks path, whole-file SHA-256, byte ranges and exact preimages. Staging combines disjoint selections, rejects overlaps, rechecks the source trees under the mutation lock and uses `AgentExecutionGate` close leases on source and destination. File writes compare the current bytes with the preview. On failure the engine rolls back writes and removes the new worktree, branch and inbox base ref. Cleanup failure surfaces the change ID. The reviewer still controls merge. [File patches](../../../crates/cowiki-knowledge/src/governance/repair.rs) · [Worktree staging](../../../web/src-tauri/src/local_engine/agent_changes.rs)
3. **Preserve trust and unrelated bytes.** If base, candidate or patched text is human-verified, `prepare_review` calls `demote` to set `verified: false`, `trust_tier: agent_draft`, `generated: true` and `updated_at` to the review date. It preserves comments, unrelated YAML and body text, and rejects unsupported complex trust fields. The existing merge-time trust reset still applies. [Staging transitions](../../../crates/cowiki-knowledge/src/governance/staging.rs)

On 160 held-out edits, combined rules and verifier repaired **2/83** faulty edits, with **0/160** unintended changes. Single-pass repaired **18/83**, with **4/160** unintended changes. The desktop rules propose a line restoration only with one unambiguous supported wording and a safe subject match. In the evaluated combined method, a model repair additionally needs a unique preimage within the finding, a verbatim quote in both the applicable verified source and replacement, grounding for the repaired assertion, and no new rule or structural issue on scratch data. A plausible paraphrase can fail these checks. Expanding coverage must retain independent evidence, bounded edits to the reviewed bytes, scratch validation of model repairs, trust demotion and explicit reviewer control. Zero observed unintended changes is a sample result. [Repair validator](../../../crates/cowiki-knowledge/src/governance/remote/cited_repair.rs) · [Evaluation](../../../benchmarks/governance-bench/README.md)

## 5. Offline rules and the remote verifier

`RuleConflictDetector` compares changed claims with retrieved passages. With sufficiently aligned subjects it can recognize changed quantities, equivalent duration units, ISO dates, defaults, negation and modal polarity. Sentence alignment within the same heading can detect a lost version or platform restriction when an independent source confirms the restriction. Historical discussion, weak alignment and unrelated subjects can yield no finding. Paraphrases and deductions across sentences remain weak points. Separate retrieval budgets of 800 and 400 tokens cover non-retired and retired evidence. `protocol::pack` admits at most 12 candidate/evidence pairs. These are local retrieval and packing limits. [Rules](../../../crates/cowiki-knowledge/src/governance/conflict.rs) · [Alignment](../../../crates/cowiki-knowledge/src/governance/alignment.rs) · [Packing](../../../crates/cowiki-knowledge/src/governance/protocol.rs)

The optional Cargo feature `remote-verifier` already provides `RemoteVerifier` for the benchmark, with OpenAI-compatible chat completions, bounded requests, usage accounting and disk record/replay. **Planned (v2)** — let a reviewer opt in to sending a review packet, configure the endpoint and model, store credentials in the OS credential store and enforce token and call ceilings across the *whole review*. On timeout, provider rejection or budget exhaustion, show which claims received only offline checks. Keep remote proposals behind the same validation and staging gates.

## 6. Performance and caching

Median wall time in milliseconds for the native `build_review` path, starting with resolved Git trees. Each size had one warm-up followed by five serial runs. Each run created two fresh directories and rebuilt both indexes.

| Markdown pages per tree | Materialize both | Initialize and index both | Complete review |
|---:|---:|---:|---:|
| 100 | 7 | 209 | 289 |
| 500 | 23 | 657 | 848 |
| 1,000 | 47 | 1,321 | 1,691 |

The synthetic Space had one root index, short verified service pages with one relative link each, and one changed timeout. The largest tree contained 220,495 bytes. Complete review includes structural audit, semantic retrieval, proposal generation and temporary-directory teardown. Fixture creation, working-tree capture, IPC and rendering are outside the timer. These are local Linux x86-64 debug-test measurements on a shared Xeon Platinum 8352V with ext4 and warm OS caches. Longer pages, larger diffs and slower storage change the work. Index rebuilding dominated at all three sizes.

**Planned (v2)** — cache derived results across review sessions per Space under `(base_commit, candidate_tree, as_of)`. Either tree changing invalidates content and citations. A new date invalidates expiry results. Namespace entries by engine/index version and verifier configuration, bound their size and rebind proposal IDs to the requested review target. A cache hit must still revalidate immutable IDs and live worktree preimages at staging. Today only the mounted panel retains a report across tab switches.

## 7. Integration interfaces

- **Ingestion and retrieval (#167, #168).** Producers supply OKF Markdown and source-backed history. The adapter provides the base index, matching root and embedder through `SnapshotIndex`. A missing historical source weakens the regression evidence. It does not grant authority to the candidate.
- **Planned (v2) — the multi-agent coordinator (#170).** After participating writers release their leases, review the combined candidate against the intended merge base, before merge. Present the resulting findings and opaque repair IDs to the reviewer. Reuse the staging command for selected repairs. If another subtask advances either snapshot, review the new pair before proceeding.
- **Planned (v2) — Cloud merge requests.** A service-side adapter in `cowiki-backend` could resolve the target base and proposed merge tree, supply ordered revisions and call the same Rust audit and conflict interfaces. Return findings and diffs under Cloud authorization. An accepted repair would require a reviewed change against the still-current MR head. The desktop IPC and MCP expose no Cloud write path.
