import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const mainLayout = readFileSync(new URL('../src/pages/MainLayout.tsx', import.meta.url), 'utf8');
const design = readFileSync(new URL('../src/lib/design.ts', import.meta.url), 'utf8');
const agentTerminalPanel = readFileSync(
  new URL('../src/components/terminal/AgentTerminalPanel.tsx', import.meta.url),
  'utf8',
);
const spacePanel = readFileSync(
  new URL('../src/components/layout/SpacePanel.tsx', import.meta.url),
  'utf8',
);

test('the page header has no unused overflow actions menu', () => {
  assert.equal(mainLayout.includes('aria-label="More actions"'), false);
});

test('the page, Space, and Agent panel headers share one visual baseline', () => {
  assert.match(design, /export const APP_HEADER_HEIGHT = 44/);
  assert.match(
    mainLayout,
    /height: APP_HEADER_HEIGHT, minHeight: APP_HEADER_HEIGHT/,
  );
  assert.match(
    agentTerminalPanel,
    /height: APP_HEADER_HEIGHT, minHeight: APP_HEADER_HEIGHT/,
  );
  assert.match(
    spacePanel,
    /height: APP_HEADER_HEIGHT, minHeight: APP_HEADER_HEIGHT/,
  );
});

test('Agent tabs and header actions share the centered control line', () => {
  assert.match(
    agentTerminalPanel,
    /className="flex shrink-0 items-center border-b/,
  );
  assert.match(
    agentTerminalPanel,
    /className="flex min-w-0 flex-1 items-center gap-0\.5/,
  );
  assert.equal(agentTerminalPanel.includes('className="mb-1.5 shrink-0'), false);
  assert.equal(agentTerminalPanel.includes('className="mb-1.5 ml-1'), false);
});
