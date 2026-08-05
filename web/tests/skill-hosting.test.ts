import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const viteConfig = readFileSync(new URL('../vite.config.ts', import.meta.url), 'utf8');
const skill = readFileSync(new URL('../../skills/cowiki-space/SKILL.md', import.meta.url), 'utf8');
const command = readFileSync(
  new URL('../../skills/cowiki-space/scripts/cowiki.mjs', import.meta.url),
  'utf8',
);

test('the web build exposes the canonical CoWiki skill and its bundled scripts', () => {
  assert.match(viteConfig, /cowikiSkillPlugin/);
  assert.match(viteConfig, /skill\.md/);
  assert.match(viteConfig, /skills\/cowiki-space/);
  assert.match(viteConfig, /text\/markdown/);
  assert.match(viteConfig, /generateBundle/);
});

test('a remotely read skill installs the complete bundle before Cloud work', () => {
  assert.match(skill, /If you are reading this file from a URL/);
  assert.match(skill, /npx skills add https:\/\/github\.com\/wfnuser\/cowiki --skill cowiki-space -g -y/);
  assert.match(skill, /ask the user for permission/i);
});

test('the skill checks once for updates before Cloud mutations without blocking local work', () => {
  assert.match(skill, /Before the first Cloud-changing command in an Agent session/);
  assert.match(skill, /npx skills check/);
  assert.match(skill, /npx skills update cowiki-space -g -y/);
  assert.match(skill, /Never update silently/);
  assert.match(skill, /Local-only work must not perform this check/);
});

test('the skill defaults ordinary users to the production Cloud API', () => {
  assert.match(
    command,
    /const DEFAULT_SERVER = process\.env\.COWIKI_CLOUD_URL \|\| 'https:\/\/api\.cowiki\.app';/,
  );
  assert.doesNotMatch(command, /https:\/\/cloud\.cowiki\.app/);
});
