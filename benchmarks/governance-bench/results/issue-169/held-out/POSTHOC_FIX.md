# Regression guard diagnostic

**Post-hoc, after seeing held-out; not the registered result.** The registered reports at `f14b3b3` are final. This diagnostic uses the product fix at `95b240a`. [Registered tables](TABLES.md) · [Registered report](../REPORT_NOTES.md)

## Root cause

**(i) Product evidence handling and (iv) a genuine detector limit.** All 13 cases supplied two correctly ordered revisions of the candidate path, and all 13 referenced source passages reached the raw index result with the expected lifecycle metadata. The benchmark passes `case.history` into the production lookup. We found no adapter field, path, revision-order, or label error. Source strings in all 13 staged restorations differ from their earlier revision strings. Here “exact” describes how the scorer matches the code and two citations. [Case adapter](../../../src/lib.rs) · [Snapshot lookup](../../../../../crates/cowiki-knowledge/src/governance/mod.rs)

- **Retired claims.** All seven labels cite both the assertion at L13 and its explicit withdrawal notice at L15 of the same retired page. Production emitted single-line citations. Four registered M2 responses identified the right code, candidate line and retired page, but cited only L13. In the other three cases the retired assertion had fallen out of the packed evidence altogether. The offline classifier judged each candidate/old-assertion pair neutral. For example, `168158349d31fc3b` describes accepting 10.0 because it has no fractional part, while the old claim describes automatic conversion of integral floats.
- **Revision regressions.** Five registered M2 responses found the current-source contradiction. The sixth lacked the correction source in the packed input. The revision guard also requires entailment between the candidate and the older assertion, and between the intervening correction and its cited authority. Both tests still use the deterministic sentence matcher in the model arms. On the six labeled pairs the former returned neutral five times and insufficient once. The latter returned neutral six times. For example, the enum edit says an import name “is gone,” while the old revision instructs readers to rename imports. A real contradiction alone cannot establish which prior error has returned. [Relation and history checks](../../../../../crates/cowiki-knowledge/src/governance/conflict.rs)
- **Packing.** In the four missing-source cases all relevant lines were present before packing. The ordinary relevance order, combined with repeated complete frontmatter in the immutable revisions, left too little space for those sources. One packet contained no evidence at all. The 4,096-token conservative content bound includes the reserved completion and differs from the provider’s measured total-token ceiling. [Packing and admission](../../../../../crates/cowiki-knowledge/src/governance/protocol.rs)

| Case ID | Target | Registered packet and verifier observation | Product packet after fix |
|---|---|---|---|
| `168158349d31fc3b` | Retired | Assertion present, finding cited L13 only | Assertion present |
| `345bec170138f1a3` | Retired | Assertion dropped | Assertion recovered |
| `3931d4e3b6364f3a` | Retired | Entire evidence list empty | Current authority recovered, retired assertion still omitted |
| `67ea3054494cb3aa` | Retired | Assertion present, finding cited L13 only | Assertion present |
| `8039b7999ee5ddce` | Retired | Assertion dropped | Assertion recovered |
| `d8971081ad47b34a` | Retired | Assertion present, finding cited L13 only | Assertion present |
| `d8d4986c6a8268eb` | Retired | Assertion present, finding cited L13 only | Assertion present |
| `0f980348c0cf9ff1` | Revision | Current contradiction detected, history entailment missed | Correction source present |
| `5b5a34c44b2ba016` | Revision | Current contradiction detected, history entailment missed | Correction source present |
| `88b3af68ade37110` | Revision | Correction source dropped | Correction source recovered |
| `acb26091e4064cd3` | Revision | Current contradiction detected, history entailment missed | Correction source present |
| `bc26679c631e07af` | Revision | Current contradiction detected, history entailment missed | Correction source present |
| `f2f9e640f63838e9` | Revision | Current contradiction detected, history entailment missed | Correction source present |

## Product changes

On a retired page, the reviewer citation now includes an immediately attached withdrawal notice from the same section. Classification still uses the assertion text. When ordinary packing omits a retrieved revision source, the packer reserves a place for named verified or retired sources. Under budget pressure it shares byte-identical historical frontmatter with the base in the wire representation. Local revisions retain their complete text. The call, token and judgment-pair ceilings are unchanged.

The relevant passage reached the final packet in 6/7 retired and 6/6 revision cases, compared with 4/7 and 5/6 in the registered input trace. For the remaining retired case, the two top active and retired assertion entries together required 4,180 units under the 4,096 admission bound. The product retained the higher-ranked active evidence. The new synthetic tests cover an exact rollback with bulky history, distinct historical scope metadata, and a cited archival notice. [Regression tests](../../../../../crates/cowiki-knowledge/tests/governance_evidence.rs)

## Diagnostic outcomes

| Method | Cases with a matching cache entry | Retired exact matches / replayable gold | Revision exact matches / replayable gold | New model calls |
|---|---:|---:|---:|---:|
| Rules, full diagnostic | Offline, 160/160 evaluated | 0/7 | 0/6 | 0 |
| Verifier only | 149/160 | 2/2 (5 unavailable) | 0/5 (1 unavailable) | 0 |
| Combined M2 | 149/160 | 2/2 (5 unavailable) | 0/5 (1 unavailable) | 0 |
| Single-pass | 149/160 | 2/2 (5 unavailable) | 0/5 (1 unavailable) | 0 |

For the full diagnostic universe, two of seven retired matches were observed on cache replay. The five missing responses are **unmeasured**. The replayable subset is selected by request identity. It cannot estimate fresh-model recall for all seven. Stored terminal failures remain in the replayable denominators, including one single-pass timeout on a revision case. Rules evaluated all 160 edits, with 5 TP, 1 FP and 90 FN overall. The diagnostic provides no evidence of improved paraphrase recognition by rules.

Each invocation used the release benchmark built with `--locked --all-features`, `--split held-out --verifier MODE --cache replay`, and an output file outside the repository under `posthoc_169/`. The three model invocations returned status 1 as designed for case failures. All four reports recorded zero HTTP calls. The shim ledger held 577 entries both before and after. Eleven request keys per model arm lacked an exact cached response. The registered outputs, input data, cache records and scoring rules were left intact.
