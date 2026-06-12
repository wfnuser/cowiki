import { C } from './design';

/**
 * Single source of truth for review-status badge styling + labels.
 * Mirrors the backend `SubmissionStatus` enum (crates/db/src/submissions.rs).
 */
export const statusBadge: Record<string, { bg: string; fg: string; label: string }> = {
  pending: { bg: C.amberSoft, fg: C.amber, label: 'Review needed' },
  approved: { bg: C.greenSoft, fg: C.green, label: 'Approved' },
  rejected: { bg: C.accentSoft, fg: C.accent, label: 'Changes requested' },
  merged: { bg: C.blueSoft, fg: C.blue, label: 'Merged' },
};
