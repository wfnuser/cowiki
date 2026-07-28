import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const root = new URL('../../', import.meta.url);

function read(relativePath: string): string {
  return readFileSync(new URL(relativePath, root), 'utf8');
}

test('the shipped Agent skill describes the local Space workflow', () => {
  const skill = read('skills/cowiki-space/SKILL.md');

  assert.match(skill, /Markdown files are the source of truth/i);
  assert.match(skill, /read-only MCP/i);
  assert.match(skill, /edit .*Markdown files directly/i);
  assert.match(skill, /Live mode/i);
  assert.match(skill, /Background mode/i);
  assert.match(skill, /local work requires no account.*server/i);
  assert.match(skill, /arbitrary (?:OKF )?hierarchy/i);
  assert.match(skill, /scripts\/cowiki\.mjs/);
  assert.match(skill, /submit --message/);
  assert.match(skill, /explicit.*submit request/i);
  assert.match(skill, /never.*API key/i);
  assert.match(skill, /do not reproduce.*rebase.*push/i);
  assert.doesNotMatch(skill, /cowiki (?:compile|write)\b/i);
  assert.doesNotMatch(skill, /cowiki review (?:show|approve|reject)\b/i);
  assert.doesNotMatch(skill, /wiki\/entities\/concepts/i);
});

test('desktop Agent prompt matches the same local-first boundary', () => {
  const terminal = read('web/src-tauri/src/terminal.rs');

  assert.match(terminal, /Markdown files are the source of truth/);
  assert.match(terminal, /arbitrary OKF hierarchy/);
  assert.match(terminal, /never requires a CoWiki API key or server/);
  assert.match(terminal, /When the cowiki MCP tools are available/);
  assert.match(terminal, /otherwise use normal file and text-search tools/);
  assert.match(terminal, /HERMES_EPHEMERAL_SYSTEM_PROMPT/);
  assert.match(terminal, /Do not commit, checkout, merge, push/);
  assert.doesNotMatch(terminal, /cowiki (?:setup|compile|write|submit)\b/i);
  assert.doesNotMatch(terminal, /cowiki review (?:show|approve|reject)\b/i);
});

test('embedded MCP remains retrieval-only', () => {
  const mcp = read('web/src-tauri/src/mcp.rs');
  const toolDefinitions = mcp.slice(mcp.indexOf('fn tool_definitions()'), mcp.indexOf('pub fn run_stdio'));

  const tools = [...toolDefinitions.matchAll(/"name": "([^"]+)"/g)].map((match) => match[1]);
  assert.deepEqual(tools, [
    'get_space_context',
    'search_pages',
    'get_page',
    'list_backlinks',
    'list_broken_links',
  ]);
});

test('MCP documentation makes the embedded desktop server authoritative', () => {
  const docs = read('docs/mcp.md');
  const legacy = read('cowiki-mcp-server/README.md');

  assert.match(docs, /embedded in the CoWiki desktop app/i);
  assert.match(docs, /no account, API key, HTTP server, or separate MCP process/i);
  assert.match(docs, /read-only/i);
  assert.match(legacy, /legacy Cloud prototype/i);
  assert.match(legacy, /not for a\s*>?\s*local Space/i);
});

test('retired CLI designs cannot be mistaken for current Agent guidance', () => {
  const retiredDesigns = [
    'docs/spec.md',
    'docs/plans/2026-05-22-cowiki-cli-ralplan.md',
    'docs/superpowers/specs/2026-06-11-cli-skill-redesign.md',
    'docs/superpowers/specs/2026-06-11-login-removal-config-redesign.md',
    'docs/superpowers/specs/2026-06-12-cli-skill-multidir-design.md',
    'docs/superpowers/specs/2026-06-12-pr83-cleanup-design.md',
    'docs/superpowers/specs/2026-06-17-submit-path-awareness-design.md',
  ];

  for (const path of retiredDesigns) {
    assert.match(read(path).slice(0, 600), /historical.*superseded/is, path);
  }
});
