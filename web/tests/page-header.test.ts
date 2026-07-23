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
const versionSwitcher = readFileSync(
  new URL('../src/components/layout/VersionSwitcher.tsx', import.meta.url),
  'utf8',
);

test('the page header has no unused overflow actions menu', () => {
  assert.equal(mainLayout.includes('aria-label="More actions"'), false);
});

test('the page, Space, and Agent panel headers share one visual baseline', () => {
  assert.match(design, /export const APP_HEADER_HEIGHT = 48/);
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

test('long breadcrumbs cannot shrink or wrap the header actions', () => {
  assert.match(mainLayout, /className="app-breadcrumb"/);
  assert.match(mainLayout, /className="app-header-actions"/);
});

test('source ingestion uses a direct accessible plus action beside Sources', () => {
  const headerActions = mainLayout
    .split('{/* Right: actions */}')[1]
    .split('{/* User menu moved to Rail bottom avatar */}')[0];
  const sourcesSection = spacePanel
    .split('{/* Section 1: Sources */}')[1]
    .split('{/* OKF concepts form one arbitrary bundle-relative hierarchy. */}')[0];
  assert.doesNotMatch(headerActions, /Add Source/);
  assert.match(sourcesSection, /aria-label="Add Source"/);
  assert.match(sourcesSection, /title="Add Source"/);
  assert.match(sourcesSection, /onClick=\{onShowIngest\}/);
  assert.match(sourcesSection, /<Plus size=\{13\}/);
  assert.doesNotMatch(sourcesSection, /DropdownMenu|MoreHorizontal/);
});

test('Source titles use a dedicated overflow-safe presentation class', () => {
  assert.match(mainLayout, /className="page-title page-title--compact source-title"/);
});

test('the desktop header exposes the focused local version switcher', () => {
  assert.match(mainLayout, /desktop && activeWorkspace\?\.localPath/);
  assert.match(mainLayout, /change\.status === 'open'/);
  assert.match(versionSwitcher, />\s*WORKING\s*</);
  assert.match(versionSwitcher, />\s*UPSTREAM\s*</);
  assert.match(versionSwitcher, />\s*AGENT CHANGES\s*</);
  assert.match(versionSwitcher, /See All in Reviews/);
  assert.doesNotMatch(versionSwitcher, /CHECKPOINTS|Discarded|Merged/);
});

test('the version switcher centers a real chevron icon instead of a text glyph', () => {
  assert.match(versionSwitcher, /import \{[^}]*ChevronDown[^}]*\} from 'lucide-react'/);
  assert.match(versionSwitcher, /<ChevronDown[^>]*aria-hidden[^>]*\/>/);
  assert.doesNotMatch(versionSwitcher, />⌄</);
});

test('the persistent version trigger does not use a high-attention status dot', () => {
  assert.doesNotMatch(versionSwitcher, /const dot =/);
  assert.doesNotMatch(versionSwitcher, /background: dot/);
});
