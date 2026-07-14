import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/**
 * Find an available text editor.
 * Checks $EDITOR, then vim, nano, vi.
 */
export function findEditor(): string {
  const editor = process.env.EDITOR;
  if (editor && editor.length > 0) {
    return editor;
  }

  for (const cmd of ['vim', 'nano', 'vi']) {
    if (editorExists(cmd)) {
      return cmd;
    }
  }
  return 'vi';
}

function editorExists(cmd: string): boolean {
  const result = spawnSync('which', [cmd], {
    stdio: 'ignore',
  });
  return result.status === 0;
}

/**
 * Open $EDITOR with a template for the given slug, return edited body.
 */
export function editInEditor(slug: string, title?: string): string {
  const editor = findEditor();
  const displayTitle = title || slug;

  const template =
    `---\ntitle: ${displayTitle}\n---\n\n` +
    `# ${displayTitle}\n\nStart writing...\n`;

  // Sanitize slug to prevent path traversal
  const safeSlug = slug.replace(/[^a-zA-Z0-9_-]/g, '_') || 'untitled';
  const tmpPath = path.join(os.tmpdir(), `cowiki-edit-${safeSlug}.md`);
  fs.writeFileSync(tmpPath, template, 'utf-8');

  const result = spawnSync(editor, [tmpPath], {
    stdio: 'inherit',
  });

  if (result.status !== 0) {
    // Best-effort cleanup
    try { fs.unlinkSync(tmpPath); } catch { /* ignore */ }
    throw new Error(`Editor '${editor}' exited with error`);
  }

  const content = fs.readFileSync(tmpPath, 'utf-8');

  // Best-effort cleanup
  try { fs.unlinkSync(tmpPath); } catch { /* ignore */ }

  const body = extractBody(content);
  if (!body) {
    throw new Error('Editor content is empty, aborting.');
  }
  return body;
}

/**
 * Extract body from editor content, stripping YAML frontmatter.
 */
export function extractBody(content: string): string {
  const trimmed = content.trim();
  if (trimmed.startsWith('---\n')) {
    const rest = trimmed.slice(4);
    const closingIdx = rest.indexOf('\n---\n');
    if (closingIdx >= 0) {
      return rest.slice(closingIdx + 5).trim();
    }
    // Malformed frontmatter: return as-is
    return trimmed;
  }
  return trimmed;
}
