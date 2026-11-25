import assert from 'node:assert/strict';
import test from 'node:test';

import { extractCodeLanguage, highlightCodeBlock } from '../src/lib/code-highlight.ts';

test('extractCodeLanguage reads language-* class names and ignores plain inline code', () => {
  assert.equal(extractCodeLanguage('language-ts'), 'ts');
  assert.equal(extractCodeLanguage('hljs language-rust extra'), 'rust');
  assert.equal(extractCodeLanguage(undefined), null);
  assert.equal(extractCodeLanguage(''), null);
});

test('highlightCodeBlock tokenises JS/TS, Rust, Python and Bash with CodeMirror highlighters', async () => {
  const js = await highlightCodeBlock("const x = 'hi'; // note", 'js');
  assert.ok(js);
  assert.equal(js.map((s) => s.text).join(''), "const x = 'hi'; // note");
  assert.ok(js.some((s) => s.text === 'const' && s.className === 'tok-keyword'));
  assert.ok(js.some((s) => s.text === "'hi'" && s.className === 'tok-string'));
  assert.ok(js.some((s) => s.text === '// note' && s.className === 'tok-comment'));

  const rust = await highlightCodeBlock('fn main() -> usize { 42 }', 'rust');
  assert.ok(rust);
  assert.ok(rust.some((s) => s.text === 'fn' && s.className === 'tok-keyword'));
  assert.ok(rust.some((s) => s.text === '42' && s.className === 'tok-literal'));

  const py = await highlightCodeBlock('def greet():\n  return "ok"', 'python');
  assert.ok(py);
  assert.equal(py.map((s) => s.text).join(''), 'def greet():\n  return "ok"');
  assert.ok(py.some((s) => s.text === 'def' && s.className === 'tok-keyword'));

  const sh = await highlightCodeBlock('# comment\necho "hello"', 'bash');
  assert.ok(sh);
  assert.equal(sh.map((s) => s.text).join(''), '# comment\necho "hello"');
  assert.ok(sh.some((s) => s.className === 'tok-comment'));
});

test('highlightCodeBlock returns null for unknown languages', async () => {
  assert.equal(await highlightCodeBlock('plain text', 'not-a-real-lang-xyz'), null);
});
