# Final development review of evidence-support rejections

Unit of review is one proposed correction to its cited finding, in the surrounding candidate paragraph. The same 17 proposals were replayed by both M2 arms. Reviewed against the exact packed source line and candidate context before changing generation. These are Codex judgments, without independent human adjudication.

Counts: (a) 1 correct rejection, (b) 15 false rejections of paraphrases, (c) 1 other. The two protected-span rejections were outside this review.

| Case / changed index | Label | Cited source | Review |
|---|---|---|---|
| 1dff1133fcf3d2d2 / 1 | b | target-sources-3.28.0.md L17 | Initial creation and omitted BASE_DIRS map to the source directory; the edit expresses both conditions. |
| 295352ffd7997806 / 1 | b | target-sources-3.28.0.md L18 | The preceding line makes include the base; include/detail is nested under it even on a later call. |
| 295352ffd7997806 / 2 | b | target-sources-3.28.0.md L18 | The prohibition explicitly spans calls, so splitting the declaration cannot make the nested base legal. |
| 30d14f9ec15936e7 / 0 | b | cmake-language-3.28.0.md L19 | The proposed precedence agrees with “outranks” in the evidence. |
| 4a3f0b14cb6afef4 / 1 | b | add-custom-command-3.28.0.md L16 | The preceding sentence supplies 3.28, file-set targets and CMP0154, making the private-set conclusion applicable. |
| 6074e6f24f2b2ec3 / 0 | c | find-package-3.28.0.md L19 | Correct extract of the source’s first clause. Only the trailing explanation and punctuation differ; this is an extract rejected by full-assertion matching, rather than a paraphrase. |
| 63f7850be749e67c / 0 | b | install-3.28.0.md L15 | The rule belongs to Development. The source permits a component-specific install despite EXCLUDE_FROM_ALL. |
| 64da2ca58f9cd0a6 / 1 | b | target-sources-3.28.0.md L15 | The page identifies CXX_MODULES and the non-imported target, so the imported-target exception does not apply. |
| 701076de5c42d6b0 / 1 | b | string-3.28.0.md L17 | The page supplies the 3.26 version condition; the proposal correctly distinguishes the zone name and numeric offset. |
| 79105045a56af900 / 1 | b | install-3.28.0.md L17 | The source makes the two selection options mutually exclusive, as the proposed sentence states. |
| 94650b44be73165b / 1 | b | file-3.28.0.md L13 | The preceding lines supply an existing destination, RESULT and NO_REPLACE; a collision yields that literal. |
| 9afc40d609e9f6b0 / 1 | b | execute-process-3.28.0.md L15 | Expiry leaves a timeout string, as the proposal says. |
| a2760126a47827e9 / 0 | a | cmake-language-3.28.0.md L15 | The replacement removes the false claim but omits the known-and-still-processing requirement central to choosing the directory. It does not repair the eligibility guidance. |
| aeee390db252024e / 0 | b | file-3.28.0.md L17 | The page already supplies version 3.26 and concerns recently produced inputs; the restored Windows-only restriction agrees with the source. |
| bc77217d227af56f / 1 | b | execute-process-3.28.0.md L15 | In context “its” refers to RESULTS_VARIABLE; the source orders those entries by command. |
| d39aa6566a2aaca3 / 1 | b | reference-card.md L15 | Excluding 00 follows directly from the closed 01–53 range. |
| fd929bb8fff3cbf9 / 1 | b | cmake-language-3.28.0.md L17 | Both calls use the same ID and directory; removing both is entailed by cancelling all matches. |
