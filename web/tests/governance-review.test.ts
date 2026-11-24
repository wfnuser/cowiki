import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { formatCitation, toggleRepair, type GovernanceReview } from '../src/components/review/governance-model.ts';

const { report }: { report: GovernanceReview } = JSON.parse(readFileSync(new URL('./fixtures/governance-cmake.json', import.meta.url), 'utf8'));
const redirect = report.proposals[0];

test('native review keeps base evidence, candidate citation and the offline verdict together', () => {
  const finding = report.findings.find(f => f.code === 'contradicts_verified')!;
  assert.equal(finding.verdict, 'contradiction');
  assert.equal(formatCitation(finding.location!), '[recently-produced-inputs.md:L13-L13]');
  assert.equal(formatCitation(finding.evidence!), '[file-3.28.0.md:L17-L17]');
  assert.ok(finding.candidate_excerpt);
  assert.ok(finding.evidence_excerpt);
  assert.ok(redirect.findingIds.includes(report.findings.find(f => f.code === 'inbound_redirect')!.id));
});

test('proposal selection adds or removes only the chosen ID', () => {
  const selected = toggleRepair(['another-repair'], redirect.id);
  assert.deepEqual(selected, ['another-repair', redirect.id]);
  assert.deepEqual(toggleRepair(selected, redirect.id), ['another-repair']);
});
