//! Hash- and span-anchored verifiable citations with offline relocation.
//!
//! Citations produced by `knowledge_index::hybrid_search` record the source file's
//! SHA-256 digest, 1-based line range, byte span, and snippet hash. When source
//! files evolve, `verify_citation` checks whether each citation is still intact,
//! shifted to a new line span, approximately modified, missing, or unverified —
//! strictly without calling any external model.

use crate::okf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;

/// Minimum Dice token similarity required to classify a modified passage as `Changed`
/// rather than `Missing`.
pub const DEFAULT_CHANGED_SIMILARITY_THRESHOLD: f64 = 0.60;

/// Portable, self-contained citation anchor captured at retrieval time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CitationAnchor {
    pub page_path: String,
    pub file_sha256: String,
    pub snippet_sha256: String,
    pub start_line: usize,
    pub end_line: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    pub excerpt: String,
}

/// Deterministic verification status of a citation against the current workspace on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CitationStatus {
    /// Source file hash is unchanged and the cited span matches verbatim.
    Intact,
    /// Source file changed, but the exact cited excerpt was relocated inside the same file.
    Moved {
        #[serde(rename = "newStartLine")]
        new_start_line: usize,
        #[serde(rename = "newEndLine")]
        new_end_line: usize,
        #[serde(rename = "currentFileSha256")]
        current_file_sha256: String,
    },
    /// Source file changed and the exact excerpt no longer matches, but a high-similarity
    /// modified passage was located at a nearby or shifted span.
    Changed {
        similarity: f64,
        #[serde(rename = "bestStartLine")]
        best_start_line: usize,
        #[serde(rename = "bestEndLine")]
        best_end_line: usize,
        #[serde(rename = "currentFileSha256")]
        current_file_sha256: String,
    },
    /// Source file was removed or the cited passage can no longer be found.
    Missing { reason: String },
    /// Citation could not be checked safely (path outside space, unreadable file, or invalid bounds).
    Unverified { reason: String },
}

/// Result of verifying a single citation anchor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CitationVerification {
    pub page_path: String,
    pub citation: String,
    pub status: CitationStatus,
}

/// Compute lowercase hexadecimal SHA-256 of raw UTF-8 bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Convert a 1-based inclusive `(start_line, end_line)` range into byte offsets `[start, end)`.
pub fn line_span_to_byte_range(
    content: &str,
    start_line: usize,
    end_line: usize,
) -> (usize, usize) {
    if content.is_empty() || start_line == 0 || end_line < start_line {
        return (0, 0);
    }
    let mut line_starts: Vec<usize> = vec![0];
    for (idx, byte) in content.bytes().enumerate() {
        if byte == b'\n' && idx + 1 < content.len() {
            line_starts.push(idx + 1);
        }
    }
    let total_lines = line_starts.len();
    if start_line > total_lines {
        return (content.len(), content.len());
    }
    let byte_start = line_starts[start_line - 1];
    let byte_end = if end_line >= total_lines {
        content.len()
    } else {
        line_starts[end_line]
    };
    (byte_start, byte_end)
}

/// Verify a single citation anchor against the live Markdown file under `space_root`.
pub fn verify_citation(space_root: &Path, anchor: &CitationAnchor) -> CitationVerification {
    let citation = format!(
        "{}#L{}-L{}",
        anchor.page_path, anchor.start_line, anchor.end_line
    );
    if anchor.start_line == 0 || anchor.end_line < anchor.start_line {
        return CitationVerification {
            page_path: anchor.page_path.clone(),
            citation,
            status: CitationStatus::Unverified {
                reason: "invalid line range".to_string(),
            },
        };
    }

    let (_, resolved_path) = match okf::safe_resolve_under_space(space_root, &anchor.page_path) {
        Ok(pair) => pair,
        Err(reason) => {
            return CitationVerification {
                page_path: anchor.page_path.clone(),
                citation,
                status: CitationStatus::Unverified { reason },
            };
        }
    };

    if !resolved_path.exists() {
        return CitationVerification {
            page_path: anchor.page_path.clone(),
            citation,
            status: CitationStatus::Missing {
                reason: "source file not found".to_string(),
            },
        };
    }

    let raw_bytes = match std::fs::read(&resolved_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return CitationVerification {
                page_path: anchor.page_path.clone(),
                citation,
                status: CitationStatus::Unverified {
                    reason: format!("cannot read source file: {err}"),
                },
            };
        }
    };
    let current_text = match String::from_utf8(raw_bytes) {
        Ok(text) => text,
        Err(_) => {
            return CitationVerification {
                page_path: anchor.page_path.clone(),
                citation,
                status: CitationStatus::Unverified {
                    reason: "source file is not valid UTF-8".to_string(),
                },
            };
        }
    };

    let current_hash = sha256_hex(current_text.as_bytes());
    let lines: Vec<&str> = current_text.lines().collect();

    // Fast path: file hash matches and the original span still matches verbatim.
    if !anchor.file_sha256.is_empty() && current_hash == anchor.file_sha256 {
        if let Some(slice_text) = extract_line_window(&lines, anchor.start_line, anchor.end_line) {
            if anchor.excerpt.trim().is_empty()
                || normalize_whitespace(&slice_text) == normalize_whitespace(&anchor.excerpt)
                || sha256_hex(slice_text.trim().as_bytes()) == anchor.snippet_sha256
            {
                return CitationVerification {
                    page_path: anchor.page_path.clone(),
                    citation,
                    status: CitationStatus::Intact,
                };
            }
        }
    }

    let target_norm = normalize_whitespace(&anchor.excerpt);
    if target_norm.is_empty() {
        return CitationVerification {
            page_path: anchor.page_path.clone(),
            citation,
            status: CitationStatus::Unverified {
                reason: "citation anchor has empty excerpt".to_string(),
            },
        };
    }

    // Check whether the original line span is still identical even if another part of the file changed.
    if let Some(orig_window) = extract_line_window(&lines, anchor.start_line, anchor.end_line) {
        if normalize_whitespace(&orig_window) == target_norm {
            return CitationVerification {
                page_path: anchor.page_path.clone(),
                citation,
                status: CitationStatus::Intact,
            };
        }
    }

    // Exact relocation search within the file (preserving line boundaries).
    let span_len = (anchor.end_line - anchor.start_line + 1).max(1);
    let excerpt_lines = anchor.excerpt.lines().count().max(1);
    let candidate_lengths: Vec<usize> = {
        let mut lens = vec![span_len, excerpt_lines];
        if span_len > 1 {
            lens.push(span_len - 1);
        }
        lens.push(span_len + 1);
        lens.sort_unstable();
        lens.dedup();
        lens
    };

    if let Some((moved_start, moved_end)) =
        find_exact_relocation(&lines, &target_norm, &candidate_lengths)
    {
        return CitationVerification {
            page_path: anchor.page_path.clone(),
            citation,
            status: CitationStatus::Moved {
                new_start_line: moved_start,
                new_end_line: moved_end,
                current_file_sha256: current_hash,
            },
        };
    }

    // Approximate token-similarity relocation for edited passages.
    let target_tokens = tokenize_set(&anchor.excerpt);
    if !target_tokens.is_empty() && !lines.is_empty() {
        let mut best_sim = 0.0_f64;
        let mut best_span = (1usize, 1usize);
        let max_win = (span_len + 2).min(lines.len()).max(1);
        let min_win = span_len.saturating_sub(2).max(1).min(lines.len());
        for win in min_win..=max_win {
            if win > lines.len() {
                continue;
            }
            for start_idx in 0..=(lines.len() - win) {
                let window_text = lines[start_idx..start_idx + win].join("\n");
                let win_tokens = tokenize_set(&window_text);
                let sim = dice_similarity(&target_tokens, &win_tokens);
                if sim > best_sim {
                    best_sim = sim;
                    best_span = (start_idx + 1, start_idx + win);
                }
            }
        }
        if best_sim >= DEFAULT_CHANGED_SIMILARITY_THRESHOLD {
            return CitationVerification {
                page_path: anchor.page_path.clone(),
                citation,
                status: CitationStatus::Changed {
                    similarity: (best_sim * 10_000.0).round() / 10_000.0,
                    best_start_line: best_span.0,
                    best_end_line: best_span.1,
                    current_file_sha256: current_hash,
                },
            };
        }
    }

    CitationVerification {
        page_path: anchor.page_path.clone(),
        citation,
        status: CitationStatus::Missing {
            reason: "cited excerpt no longer matches any passage in the source file".to_string(),
        },
    }
}

/// Verify a batch of citation anchors in order.
pub fn verify_citations_batch(
    space_root: &Path,
    anchors: &[CitationAnchor],
) -> Vec<CitationVerification> {
    anchors
        .iter()
        .map(|anchor| verify_citation(space_root, anchor))
        .collect()
}

fn extract_line_window(lines: &[&str], start_line: usize, end_line: usize) -> Option<String> {
    if start_line == 0 || end_line < start_line || start_line > lines.len() {
        return None;
    }
    let clamped_end = end_line.min(lines.len());
    Some(lines[start_line - 1..clamped_end].join("\n"))
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_exact_relocation(
    lines: &[&str],
    target_norm: &str,
    window_lengths: &[usize],
) -> Option<(usize, usize)> {
    for &win in window_lengths {
        if win == 0 || win > lines.len() {
            continue;
        }
        for start_idx in 0..=(lines.len() - win) {
            let candidate = normalize_whitespace(&lines[start_idx..start_idx + win].join("\n"));
            if candidate == target_norm {
                return Some((start_idx + 1, start_idx + win));
            }
        }
    }
    None
}

fn tokenize_set(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter_map(|tok| {
            let low = tok.to_ascii_lowercase();
            if low.len() >= 2 {
                Some(low)
            } else {
                None
            }
        })
        .collect()
}

fn dice_similarity(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let inter = left.intersection(right).count() as f64;
    (2.0 * inter) / ((left.len() + right.len()) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn citation_lifecycle_intact_moved_changed_missing_and_unverified() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("policy.md");
        let original = "# Retention Policy\n\n## Financial Records\n\nPosted financial ledger journals remain immutable in WORM storage for seven years.\n";
        fs::write(&file_path, original).unwrap();

        let excerpt =
            "Posted financial ledger journals remain immutable in WORM storage for seven years.";
        let (b0, b1) = line_span_to_byte_range(original, 5, 5);
        assert_eq!(&original[b0..b1].trim(), &excerpt);

        let anchor = CitationAnchor {
            page_path: "policy.md".to_string(),
            file_sha256: sha256_hex(original.as_bytes()),
            snippet_sha256: sha256_hex(excerpt.as_bytes()),
            start_line: 5,
            end_line: 5,
            byte_start: b0,
            byte_end: b1,
            excerpt: excerpt.to_string(),
        };

        // 1. Intact
        let v1 = verify_citation(dir.path(), &anchor);
        assert_eq!(v1.status, CitationStatus::Intact);

        // 2. Moved (prepend new section shifting lines down)
        let moved = "# Retention Policy\n\n## Scope\n\nApplies to all production ledgers.\n\n## Financial Records\n\nPosted financial ledger journals remain immutable in WORM storage for seven years.\n";
        fs::write(&file_path, moved).unwrap();
        let v2 = verify_citation(dir.path(), &anchor);
        match v2.status {
            CitationStatus::Moved {
                new_start_line,
                new_end_line,
                ..
            } => {
                assert_eq!(new_start_line, 9);
                assert_eq!(new_end_line, 9);
            }
            other => panic!("expected Moved, got {other:?}"),
        }

        // 3. Changed (edit retention duration from seven to ten years)
        let changed = "# Retention Policy\n\n## Financial Records\n\nPosted financial ledger journals remain immutable in WORM storage for ten years.\n";
        fs::write(&file_path, changed).unwrap();
        let v3 = verify_citation(dir.path(), &anchor);
        match v3.status {
            CitationStatus::Changed {
                similarity,
                best_start_line,
                best_end_line,
                ..
            } => {
                assert!(
                    similarity >= 0.80,
                    "expected high similarity, got {similarity}"
                );
                assert_eq!(best_start_line, 5);
                assert_eq!(best_end_line, 5);
            }
            other => panic!("expected Changed, got {other:?}"),
        }

        // 4. Missing (content replaced completely)
        fs::write(
            &file_path,
            "# Retention Policy\n\nCompletely unrelated text.\n",
        )
        .unwrap();
        let v4 = verify_citation(dir.path(), &anchor);
        assert!(matches!(v4.status, CitationStatus::Missing { .. }));

        // 5. Missing (file deleted)
        fs::remove_file(&file_path).unwrap();
        let v5 = verify_citation(dir.path(), &anchor);
        assert!(matches!(v5.status, CitationStatus::Missing { .. }));

        // 6. Unverified (path escape rejected)
        let mut escaped = anchor.clone();
        escaped.page_path = "../outside.md".to_string();
        let v6 = verify_citation(dir.path(), &escaped);
        assert!(matches!(v6.status, CitationStatus::Unverified { .. }));
    }
}
