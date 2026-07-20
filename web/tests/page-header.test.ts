import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const mainLayout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');
const design = readFileSync(new URL('../src/lib/design.ts', import.meta.url), 'utf8');
const agentTerminalPanel = readFileSync(
  new URL('../src/components/terminal/AgentTerminalPanel.tsx', import.meta.url),
  'utf8',
);

test('the page header has no unused overflow actions menu', () => {
  assert.equal(mainLayout.includes('aria-label="More actions"'), false);
});

test('the page and Agent panel headers share one visual baseline', () => {
  assert.match(design, /export const APP_HEADER_HEIGHT = 44/);
  assert.match(
    mainLayout,
    /height: APP_HEADER_HEIGHT, minHeight: APP_HEADER_HEIGHT/,
  );
  assert.match(
    agentTerminalPanel,
    /height: APP_HEADER_HEIGHT, minHeight: APP_HEADER_HEIGHT/,
  );
});
