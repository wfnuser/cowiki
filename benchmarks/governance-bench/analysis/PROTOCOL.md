# Issue 169 final experiment

The final development decision used the CMake repair rejections. The pre-existing six-family tuning and two-family validation assignments stay fixed. The manual review included both partitions, so validation is an internal development diagnostic. CPython is the only external test. Its cases, sources and labels were locked before detector iteration.

## Five paired arms

Every arm receives the same base, edit and retrieved evidence packet for a case. The runner asserts the sealed manifests, executable, environment and cache prefix before starting. The two concurrent record workers run single-pass and M2 verifier-only. Combined M2 replays exactly the verifier-only responses and adds deterministic rules. No maintenance and rules run offline. Count failed, refused, over-budget and over-admission cases in all quality denominators; record available usage and any estimates. A completed cached failure is a terminal outcome. On a provider quota pause or process interruption, keep completed responses and reports and resume the same outstanding arms. A finished experiment is write-once. The per-arm process wall clock includes retrieval, validation and scoring. Concurrent arms overlap. The combined replay wall measures only the extra execution with the shared responses available.

## Outcomes and uncertainty

The primary contrast is combined M2 minus single-pass. Detection uses micro-averaged exact finding-code, candidate range and evidence range matches. Repair success is acceptable final text per faulty edit. A false alarm is any finding on a valid edit. An unintended change is a valid unaccepted edit or a lost protected substring, divided by all edits. Show TP/FP/FN and the raw numerators and denominators. Cost is logical calls, provider input and output tokens, and wall time. Report physical calls separately. The empty-message input measurements for each wire format estimate included Codex overhead. They do not identify precisely how many tokens are billable or served from a provider cache.

Draw the same number of source families as observed, with replacement, carrying their complete sets of cases into each sample. Preserve the sampled family multiplicities across all five arms and all issue strata. Aggregate case counts before computing ratios. Use 10,000 draws with Python's Random(16920261207), and the linearly interpolated 2.5th and 97.5th percentiles. For paired differences compute both metrics on each shared draw, then subtract. Omit draws with undefined denominators for that metric and disclose how many were defined. An undefined point estimate is unavailable. Small or degenerate intervals in sparse strata have limited interpretation. There is no multiplicity adjustment.

Primary challenge types partition the edits for case rates and cost. Individual issue codes can occur together and have detection P/R/F1 only. Report per-case seconds from the measured pass (including cache replay time for combined M2), with separate cold-arm process wall times. Select error examples using the frozen routine: aim for a false alarm/unintended edit, a missed finding, and detection without a successful repair; use distinct families where possible, then order by case ID. Read text for the selected examples only after all five arms finish.

## Reproduce the published statistics without invoking a model

From the repository root, unpack the five archived JSON reports into a scratch directory, preserving their method filenames. Run the following with that directory as REPORTS and a new directory as OUT.

```sh
python3 benchmarks/governance-bench/analysis/cluster_report.py --reports "$REPORTS" --data benchmarks/governance-bench/data/held-out --out "$OUT" --single-overhead 12260 --m2-overhead 12348
python3 benchmarks/governance-bench/analysis/render_tables.py "$OUT"
```

The process-level wall totals and provider usage come from the saved execution and trace metadata. Statistical analysis needs no model, token, cache entry, or benchmark invocation.
