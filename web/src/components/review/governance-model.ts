import type { FileDiff } from '../../api';

export interface GovernanceCitation {
  path: string;
  snapshot: string;
  first: number;
  last: number;
}

export interface GovernanceFinding {
  id: string;
  code: string;
  severity: 'critical' | 'warning' | 'info';
  path: string;
  message: string;
  location: GovernanceCitation | null;
  evidence: GovernanceCitation | null;
  candidate_excerpt: string | null;
  evidence_excerpt: string | null;
  verdict: 'contradiction' | 'entailment' | 'neutral' | 'scope_difference' | 'insufficient' | null;
}

export interface GovernanceProposal {
  id: string;
  rationale: string;
  findingIds: string[];
  diff: FileDiff;
}

export interface GovernanceReview {
  asOf: string;
  findings: GovernanceFinding[];
  proposals: GovernanceProposal[];
  warnings: string[];
  canStage: boolean;
}

export function formatCitation(citation: GovernanceCitation): string {
  return `[${citation.path}:L${citation.first}-L${citation.last}]`;
}

export function toggleRepair(selected: string[], id: string): string[] {
  return selected.includes(id) ? selected.filter(item => item !== id) : [...selected, id];
}
