# Issue 169 evaluation report

## Held-out results

The five frozen methods evaluated the same 160 CPython documentation edits from 16 source families. There were 83 faulty edits, 77 valid controls and 95 labeled findings. The method commit was `2ffe816` and the pre-dispatch freeze was `df70a36`. Every case, including budget refusals and failed calls, is in its original denominator.

Detection requires agreement on the issue code and both citation ranges. Percentages below show the point estimate and 95% family-cluster bootstrap interval. Each of 10,000 draws resampled complete families with replacement. The primary contrast uses the same draw for combined M2 and single-pass. A dash marks an undefined denominator. [Registered protocol](../../analysis/PROTOCOL.md)

| Method | TP / FP / FN | Precision % | Recall % | F1 % | Repaired / faulty | Alarms / valid | Unintended / edits |
| --- | --- | --- | --- | --- | --- | --- | --- |
| No maintenance | 0 / 0 / 95 | — [—, —] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0/83 · 0.0 [0.0, 0.0]% | 0/77 · 0.0 [0.0, 0.0]% | 0/160 · 0.0 [0.0, 0.0]% |
| Rules only | 5 / 1 / 90 | 83.3 [50.0, 100.0] | 5.3 [1.9, 9.5] | 9.9 [3.7, 17.1] | 2/83 · 2.4 [0.0, 6.1]% | 0/77 · 0.0 [0.0, 0.0]% | 0/160 · 0.0 [0.0, 0.0]% |
| Single-pass | 58 / 35 / 37 | 62.4 [52.6, 72.6] | 61.1 [52.1, 69.7] | 61.7 [53.2, 69.8] | 18/83 · 21.7 [14.3, 28.6]% | 6/77 · 7.8 [2.9, 12.7]% | 4/160 · 2.5 [0.6, 5.0]% |
| Verifier only | 62 / 10 / 33 | 86.1 [79.7, 92.4] | 65.3 [59.3, 70.8] | 74.3 [68.4, 79.1] | 2/83 · 2.4 [0.0, 6.1]% | 1/77 · 1.3 [0.0, 3.9]% | 0/160 · 0.0 [0.0, 0.0]% |
| Combined M2 | 62 / 11 / 33 | 84.9 [78.9, 90.8] | 65.3 [59.3, 70.8] | 73.8 [68.3, 78.4] | 2/83 · 2.4 [0.0, 6.1]% | 1/77 · 1.3 [0.0, 3.9]% | 0/160 · 0.0 [0.0, 0.0]% |

Combined M2 detected 62 of 95 findings and made one false alarm on 77 valid edits. It repaired 2 of 83 faulty edits, both structural. Single-pass repaired 18 of 83. In each M2 arm the validator accepted one model proposal, rejected 48 for insufficient replacement support, eight for protected spans and one for unrelated evidence. An accepted proposal does not ensure an acceptable final document. Adding rules to the shared verifier produced one additional false positive and left its true-positive count unchanged.

### Combined M2 minus single-pass

| Outcome | M2 numerator/denominator | Single-pass numerator/denominator | Difference, percentage points [95% interval] | Defined paired draws |
| --- | --- | --- | --- | --- |
| f1 | 124/168 | 116/188 | 12.1 [6.2, 18.1] | 10,000/10,000 |
| repair | 2/83 | 18/83 | -19.3 [-25.6, -12.8] | 10,000/10,000 |
| false alarm | 1/77 | 6/77 | -6.5 [-12.2, -1.2] | 10,000/10,000 |
| unintended | 0/160 | 4/160 | -2.5 [-5.0, -0.6] | 10,000/10,000 |

F1 numerators and denominators use 2TP and 2TP+FP+FN. The repair, alarm and unintended-change differences use percentages of faulty, valid and all edits respectively. Negative differences therefore favor M2 for alarms and unintended changes.

### Where the methods detected issues

Detection F1 by labeled issue code is below. The full appendix also gives precision, recall, all edit-level outcomes and cost by primary challenge, every numerator and denominator, and paired differences within each stratum. Some edits have multiple finding codes. [Full tables and 95% intervals](held-out/TABLES.md) · [Estimates CSV](held-out/intervals.csv) · [Paired contrasts CSV](held-out/paired-differences.csv)

| Finding code | Gold findings | No maintenance F1 % [95% interval] | Rules only F1 % [95% interval] | Single-pass F1 % [95% interval] | Verifier only F1 % [95% interval] | Combined M2 F1 % [95% interval] |
| --- | --- | --- | --- | --- | --- | --- |
| broken_link | 1 | 0.0 [0.0, 0.0] | 100.0 [100.0, 100.0] | 0.0 [0.0, 0.0] | 66.7 [0.0, 100.0] | 66.7 [0.0, 100.0] |
| broken_redirect | 1 | 0.0 [0.0, 0.0] | 100.0 [100.0, 100.0] | 100.0 [100.0, 100.0] | 100.0 [100.0, 100.0] | 100.0 [100.0, 100.0] |
| contradicts_verified | 73 | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 78.5 [68.6, 87.2] | 84.2 [77.3, 90.4] | 84.2 [77.3, 90.4] |
| duplicate_overlap | 2 | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] |
| expired_stale_after | 1 | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] |
| inbound_redirect | 2 | 0.0 [0.0, 0.0] | 100.0 [100.0, 100.0] | 100.0 [100.0, 100.0] | 100.0 [100.0, 100.0] | 100.0 [100.0, 100.0] |
| missing_generated_sources | 1 | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 100.0 [100.0, 100.0] | 100.0 [100.0, 100.0] | 100.0 [100.0, 100.0] |
| redirect_cycle | 1 | 0.0 [0.0, 0.0] | 100.0 [100.0, 100.0] | 100.0 [100.0, 100.0] | 100.0 [100.0, 100.0] | 100.0 [100.0, 100.0] |
| reintroduces_retired | 7 | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] |
| revision_regression | 6 | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] |

### Calls, tokens and time

Elapsed wall time was 2839.4 s (47.3 min), from 13:02:38 UTC to 13:49:58 UTC. The two record workers shared the two-slot shim. Combined M2 replayed the verifier-only response for each request and ran the combined pipeline locally. The logical cost of that response belongs to both methods; the physical cost was incurred once. Arm wall times overlap.

| Arm | Logical calls | Physical attempts / with usage | Logical tokens (input + output) | Measured provider input + output | Included Codex input overhead (logical / measured physical, estimated) | Arm wall, s |
| --- | --- | --- | --- | --- | --- | --- |
| No maintenance | 0 | 0 / 0 | 0 + 0 | 0 + 0 | 0 / 0 | 0.60 |
| Rules only | 0 | 0 / 0 | 0 + 0 | 0 + 0 | 0 / 0 | 0.75 |
| Single-pass | 160 | 160 / 157 | 2,067,720 + 31,138 | 2,058,019 + 28,834 | 1,961,600 / 1,924,820 | 2262.45 |
| Verifier only | 160 | 160 / 158 | 2,107,563 + 71,573 | 2,100,925 + 70,037 | 1,975,680 / 1,950,984 | 2836.50 |
| Combined M2 | 160 | 0 / 0 | 2,107,563 + 71,573 | 0 + 0 | 1,975,680 / 0 | 0.90 |

Across 320 physical attempts, 315 supplied usage totaling 4,257,815 model tokens, including an estimated 3,875,804 Codex input-overhead tokens. The matched empty-input estimates are 12,260 tokens for single-pass and 12,348 for quoted M2. Provider-reported cached input was 2,818,176 tokens and is already included. All 315 completed turns passed the no-tools check. Five calls timed out without provider usage. Ten M2 responses exceeded the output cap (maximum 1,249), and single-pass had no measured cap overruns. Each arm remained within the one-call cap. The supervisor completed without a crash, quota pause or restart. Budget or trace failures remained scored. Tokens consumed by attempts lacking provider usage are unknown; benchmark logical totals include explicitly estimated costs where applicable. [Cost breakdown, caps, failures and latencies](held-out/cost.json) · [Sanitized turn metadata](held-out/turns.jsonl) · [Run events](held-out/events.jsonl)

- Single-pass had 22 case failures (19 × invalid line edit, 3 × verifier HTTP 502 Bad Gateway). Cost estimates were used on 3 cases.
- Verifier only had 12 case failures (2 × verifier HTTP 502 Bad Gateway, 10 × verifier identity or budget mismatch). Cost estimates were used on 2 cases.
- Combined M2 had 12 case failures (2 × verifier HTTP 502 Bad Gateway, 10 × verifier identity or budget mismatch). Cost estimates were used on 2 cases.

### Error analysis

The frozen selection favored an erroneous alert or edit, a missed finding, and detection without repair, with diverse families where available. These examples illustrate mechanisms; the tables give their frequencies.

| Arm | Case ID | What happened |
|---|---|---|
| Single-pass | `02d5d918a31480dc` | A valid note distinguished the withdrawn dataclass checklist from the current unhashability check. Single-pass expanded the current sentence without issuing a finding. The edit counted as one unintended change. |
| Single-pass | `012ace7b708a7a4e` | For 100 percentile intervals, it corrected the cut-point count from 100 to 99. Its two one-line findings missed the registered two-line span, and its final wording differed from the acceptable texts. This is a meaningful numerical correction that the exact metric missed. |
| Single-pass | `02808c2b56f2a18c` | It correctly cited the CSV reader/writer distinction and restored the reader’s CR/LF behavior. Its defensible sentence differed from the registered final text, so detection earned credit and repair earned zero. |
| Verifier only | `8d0596a19b116c18` | It flagged the live link to the supplied tempfile documentation as broken. The missing-looking example on the preceding line was literal code. This was a false alarm on a valid edit. |
| Verifier only | `07103a4814bfaaff` | It identified the unsafe inference that a blocking get must return immediately after a positive queue-size observation. It cited only the conclusion at line 17. Gold also required the observation at line 16. The support check rejected the replacement. |
| Verifier only | `02808c2b56f2a18c` | The CSV conflict and its evidence line matched exactly. The replacement failed the support check, leaving the claim unrepaired. |
| Combined M2 | `8d0596a19b116c18` | Combined M2 retained the verifier’s broken-link finding on the valid live reference. The final document stayed unchanged, illustrating a false alarm without an unintended edit. |
| Combined M2 | `07103a4814bfaaff` | The rules added no finding covering both the queue observation and its conclusion. The shared verifier response still cited only line 17, and the unsupported patch stayed unapplied. |
| Combined M2 | `02808c2b56f2a18c` | The rules and shared verifier produced a correctly located CSV finding. The support rejection stood and no fallback corrected the reader claim. |

## Limitations

- Rules caught 0/24 findings in the paraphrased-contradiction cases. Each M2 model arm caught 23/24, and single-pass caught 20/24. All five methods scored 0/7 exact retired-claim findings and 0/6 exact revision-regression findings. This test provides no affirmative detection evidence for the retired-claim guard on CPython.
- A paired two-line inference can be diagnosed as two plausible one-line findings and score an exact recall of zero for that finding. The percentile and queue cases below the results tables illustrate this mismatch. The reported scores retain the registered span requirement.
- The benchmark uses constructed edits to two public documentation corpora. Only CPython is held out. Sixteen document families offer limited precision, particularly for rare issue codes. Percentile intervals can be zero-width or undefined in sparse strata. Multiple per-type intervals are descriptive.
- The independent builder authored gold labels and acceptable texts. Confirmation from both human reviewers was unavailable for this report. A metadata search of the separate audit worksheet after method settings were fixed surfaced one generic source line about file modes; it exposed no case ID or verdict and informed no change.
- Exact citation ranges and a finite list of acceptable complete texts can penalize a defensible diagnosis or a semantically equivalent repair. Unintended-change counts cover the registered protected text and valid-edit criterion. Human acceptance and other changes to meaning were not measured.
- Rules can miss paraphrased contradictions and implicit loss of scope. The verbatim-quote M2 validator can still reject a correct correction or accept text different from the registered target. Reviewing all 17 earlier CMake rejections also limits the independence of the two internal validation families.
- Each admitted case had one model sample per distinct prompt. The M2 arms share outputs; the experiment measures the frozen pipelines with their budgets and failures. Provider-side model versioning, prompt caching, concurrent load and unknown usage on failed traces limit cost and timing portability. Empty-message overhead estimates include the schema and wrapper.

The method, data, verifier configuration and scorer stayed at the pre-dispatch freeze. The statistical tables use the saved five reports. [Freeze record](held-out/../freeze.json) · [Final CMake development check](held-out/../development/SUMMARY.md)

## Verification

The supervisor recorded one start and one completion for each method. The five result archives preserve its complete reports. All 160 cases had the same packet hash in all five arms. Before publication, the analysis reconciled per-case counts against the Rust output, checked the frozen inputs and the 252 pre-existing cache records, and recorded 320 new cache hashes. The Rust formatting, all-targets/all-features Clippy and locked tests passed for both affected manifests. [Artifact checksums](held-out/SHA256SUMS) · [Cache additions](held-out/cache-delta.sha256)
