## Final bounded CMake development iteration

Of the 17 earlier evidence-support rejections, 1 was correct (a), 15 rejected supported paraphrases (b), and 1 rejected a near-verbatim clause (c). The last case failed the earlier full-line overlap check. Each decision is recorded with the CMake case ID and source line. The two model arms shared the same 17 proposals.

The M2 request now asks for a material verbatim quote and requires it in the replacement. The validator checks exact inclusion in both the cited source and replacement and retains its prior grounding, containment, preimage and scratch checks. Single-pass and the deterministic rules were frozen before this comparison.

| Arm | TP/FP/FN | P / R / F1 | Repaired / faulty | Alarms / valid | Unintended / edits |
| --- | --- | --- | --- | --- | --- |
| none | 0/0/41 | — / 0.000 / 0.000 | 0/35 | 0/29 | 0/64 |
| rules | 6/1/35 | 0.857 / 0.146 / 0.250 | 2/35 | 0/29 | 0/64 |
| single-pass | 22/8/19 | 0.733 / 0.537 / 0.620 | 8/35 | 0/29 | 3/64 |
| remote | 22/3/19 | 0.880 / 0.537 / 0.667 | 2/35 | 0/29 | 0/64 |
| both | 22/4/19 | 0.846 / 0.537 / 0.657 | 2/35 | 0/29 | 0/64 |

### Family-slice check

The internal validation families are CMake string and install. The earlier manual rejection review included cases from these families. These development measurements cannot establish independent generalization.

| Arm | Slice | Generation | TP/FP/FN | P / R / F1 | Repaired / faulty | Alarms / valid | Unintended / edits |
| --- | --- | --- | --- | --- | --- | --- | --- |
| remote | tune | Prior | 15/4/15 | 0.789 / 0.500 / 0.612 | 1/25 | 0/23 | 0/48 |
| remote | tune | Verbatim | 16/0/14 | 1.000 / 0.533 / 0.696 | 1/25 | 0/23 | 0/48 |
| remote | validation | Prior | 6/3/5 | 0.667 / 0.545 / 0.600 | 1/10 | 0/6 | 0/16 |
| remote | validation | Verbatim | 6/3/5 | 0.667 / 0.545 / 0.600 | 1/10 | 0/6 | 0/16 |
| both | tune | Prior | 16/4/14 | 0.800 / 0.533 / 0.640 | 1/25 | 0/23 | 0/48 |
| both | tune | Verbatim | 16/0/14 | 1.000 / 0.533 / 0.696 | 1/25 | 0/23 | 0/48 |
| both | validation | Prior | 6/4/5 | 0.600 / 0.545 / 0.571 | 1/10 | 0/6 | 0/16 |
| both | validation | Verbatim | 6/4/5 | 0.600 / 0.545 / 0.571 | 1/10 | 0/6 | 0/16 |

### Validator, failures and cost

- remote: model 2 accepted, 2 protected_span, 12 unsupported_replacement. Case failures: 4 verifier HTTP 502 Bad Gateway, 2 verifier identity or budget mismatch, 1 packet exceeds equal per-case token budget before dispatch.
- both: model 2 accepted, 2 protected_span, 12 unsupported_replacement. Case failures: 4 verifier HTTP 502 Bad Gateway, 2 verifier identity or budget mismatch, 1 packet exceeds equal per-case token budget before dispatch.

The verifier used 63 physical calls, 808,496 measured tokens and 1071.2 seconds for the two partitions. Combined M2 used the same responses with zero additional calls. Of 59 measured turns, 2 exceeded at least one cap (maximum 14,242 total, 924 output). Failures remain scored. Trace outcomes: completed 59, cli_timeout 4. The separate matched empty-input calibration was 12,348 input tokens and 42 output tokens. Single-pass retains its original 12,260-token input calibration.

### Scores by primary challenge

| Primary challenge | Arm | TP/FP/FN | P / R / F1 | Repaired / faulty | Alarms / valid | Unintended / edits |
| --- | --- | --- | --- | --- | --- | --- |
| concurrent_edits | none | 0/0/1 | — / 0.000 / 0.000 | 0/1 | 0/4 | 0/5 |
| concurrent_edits | rules | 0/0/1 | — / 0.000 / 0.000 | 0/1 | 0/4 | 0/5 |
| concurrent_edits | single-pass | 1/0/0 | 1.000 / 1.000 / 1.000 | 0/1 | 0/4 | 0/5 |
| concurrent_edits | remote | 1/0/0 | 1.000 / 1.000 / 1.000 | 0/1 | 0/4 | 0/5 |
| concurrent_edits | both | 1/0/0 | 1.000 / 1.000 / 1.000 | 0/1 | 0/4 | 0/5 |
| duplicate_overlap | none | 0/0/1 | — / 0.000 / 0.000 | 0/1 | 0/0 | 0/1 |
| duplicate_overlap | rules | 0/0/1 | — / 0.000 / 0.000 | 0/1 | 0/0 | 0/1 |
| duplicate_overlap | single-pass | 0/1/1 | 0.000 / 0.000 / 0.000 | 0/1 | 0/0 | 0/1 |
| duplicate_overlap | remote | 0/1/1 | 0.000 / 0.000 / 0.000 | 0/1 | 0/0 | 0/1 |
| duplicate_overlap | both | 0/1/1 | 0.000 / 0.000 / 0.000 | 0/1 | 0/0 | 0/1 |
| entailment | none | 0/0/0 | — / — / — | 0/0 | 0/9 | 0/9 |
| entailment | rules | 0/0/0 | — / — / — | 0/0 | 0/9 | 0/9 |
| entailment | single-pass | 0/0/0 | — / — / — | 0/0 | 0/9 | 2/9 |
| entailment | remote | 0/0/0 | — / — / — | 0/0 | 0/9 | 0/9 |
| entailment | both | 0/0/0 | — / — / — | 0/0 | 0/9 | 0/9 |
| historical_context | none | 0/0/0 | — / — / — | 0/0 | 0/2 | 0/2 |
| historical_context | rules | 0/0/0 | — / — / — | 0/0 | 0/2 | 0/2 |
| historical_context | single-pass | 0/0/0 | — / — / — | 0/0 | 0/2 | 0/2 |
| historical_context | remote | 0/0/0 | — / — / — | 0/0 | 0/2 | 0/2 |
| historical_context | both | 0/0/0 | — / — / — | 0/0 | 0/2 | 0/2 |
| multi_sentence_inference | none | 0/0/5 | — / 0.000 / 0.000 | 0/5 | 0/2 | 0/7 |
| multi_sentence_inference | rules | 0/0/5 | — / 0.000 / 0.000 | 0/5 | 0/2 | 0/7 |
| multi_sentence_inference | single-pass | 2/0/3 | 1.000 / 0.400 / 0.571 | 0/5 | 0/2 | 0/7 |
| multi_sentence_inference | remote | 2/0/3 | 1.000 / 0.400 / 0.571 | 0/5 | 0/2 | 0/7 |
| multi_sentence_inference | both | 2/0/3 | 1.000 / 0.400 / 0.571 | 0/5 | 0/2 | 0/7 |
| neutral | none | 0/0/0 | — / — / — | 0/0 | 0/1 | 0/1 |
| neutral | rules | 0/0/0 | — / — / — | 0/0 | 0/1 | 0/1 |
| neutral | single-pass | 0/0/0 | — / — / — | 0/0 | 0/1 | 0/1 |
| neutral | remote | 0/0/0 | — / — / — | 0/0 | 0/1 | 0/1 |
| neutral | both | 0/0/0 | — / — / — | 0/0 | 0/1 | 0/1 |
| paraphrased_contradiction | none | 0/0/6 | — / 0.000 / 0.000 | 0/6 | 0/0 | 0/6 |
| paraphrased_contradiction | rules | 0/0/6 | — / 0.000 / 0.000 | 0/6 | 0/0 | 0/6 |
| paraphrased_contradiction | single-pass | 4/2/2 | 0.667 / 0.667 / 0.667 | 0/6 | 0/0 | 0/6 |
| paraphrased_contradiction | remote | 4/1/2 | 0.800 / 0.667 / 0.727 | 0/6 | 0/0 | 0/6 |
| paraphrased_contradiction | both | 4/1/2 | 0.800 / 0.667 / 0.727 | 0/6 | 0/0 | 0/6 |
| reintroduced_error | none | 0/0/7 | — / 0.000 / 0.000 | 0/4 | 0/0 | 0/4 |
| reintroduced_error | rules | 0/0/7 | — / 0.000 / 0.000 | 0/4 | 0/0 | 0/4 |
| reintroduced_error | single-pass | 0/2/7 | 0.000 / 0.000 / 0.000 | 2/4 | 0/0 | 0/4 |
| reintroduced_error | remote | 0/1/7 | 0.000 / 0.000 / 0.000 | 0/4 | 0/0 | 0/4 |
| reintroduced_error | both | 0/1/7 | 0.000 / 0.000 / 0.000 | 0/4 | 0/0 | 0/4 |
| release_transition | none | 0/0/0 | — / — / — | 0/0 | 0/9 | 0/9 |
| release_transition | rules | 0/0/0 | — / — / — | 0/0 | 0/9 | 0/9 |
| release_transition | single-pass | 0/0/0 | — / — / — | 0/0 | 0/9 | 0/9 |
| release_transition | remote | 0/0/0 | — / — / — | 0/0 | 0/9 | 0/9 |
| release_transition | both | 0/0/0 | — / — / — | 0/0 | 0/9 | 0/9 |
| revision_regression | none | 0/0/6 | — / 0.000 / 0.000 | 0/3 | 0/0 | 0/3 |
| revision_regression | rules | 0/0/6 | — / 0.000 / 0.000 | 0/3 | 0/0 | 0/3 |
| revision_regression | single-pass | 1/3/5 | 0.250 / 0.167 / 0.200 | 3/3 | 0/0 | 0/3 |
| revision_regression | remote | 1/0/5 | 1.000 / 0.167 / 0.286 | 0/3 | 0/0 | 0/3 |
| revision_regression | both | 1/0/5 | 1.000 / 0.167 / 0.286 | 0/3 | 0/0 | 0/3 |
| scope_condition | none | 0/0/8 | — / 0.000 / 0.000 | 0/8 | 0/1 | 0/9 |
| scope_condition | rules | 1/0/7 | 1.000 / 0.125 / 0.222 | 0/8 | 0/1 | 0/9 |
| scope_condition | single-pass | 8/0/0 | 1.000 / 1.000 / 1.000 | 0/8 | 0/1 | 1/9 |
| scope_condition | remote | 8/0/0 | 1.000 / 1.000 / 1.000 | 0/8 | 0/1 | 0/9 |
| scope_condition | both | 8/0/0 | 1.000 / 1.000 / 1.000 | 0/8 | 0/1 | 0/9 |
| structural_safety | none | 0/0/7 | — / 0.000 / 0.000 | 0/7 | 0/1 | 0/8 |
| structural_safety | rules | 5/1/2 | 0.833 / 0.714 / 0.769 | 2/7 | 0/1 | 0/8 |
| structural_safety | single-pass | 6/0/1 | 1.000 / 0.857 / 0.923 | 3/7 | 0/1 | 0/8 |
| structural_safety | remote | 6/0/1 | 1.000 / 0.857 / 0.923 | 2/7 | 0/1 | 0/8 |
| structural_safety | both | 6/1/1 | 0.857 / 0.857 / 0.857 | 2/7 | 0/1 | 0/8 |
