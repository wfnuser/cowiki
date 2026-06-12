import { Command } from 'commander';
import fs from 'node:fs';
import { loadConfig } from '../config.js';
import { CowikiClient } from '../client.js';
import { printSuccess, printError, printJson } from '../output.js';
import { resolveUserBranch, requireWorkspace } from '../shared.js';
import { editInEditor } from '../utils/editor.js';
import type { GlobalOpts } from '../shared.js';
import type { WritePageRequest } from '../types.js';

export function registerWriteCommand(program: Command): void {
  program
    .command('write')
    .description('Write a wiki page (supports --path for wiki/entities/concepts)')
    .argument('<slug>', 'Page slug')
    .option('--title <title>', 'Page title')
    .option('--body <body>', 'Page body (inline or from stdin)')
    .option('--summary <summary>', 'Change summary')
    .option('--path <path>', 'Target directory: wiki (default), entities, concepts')
    .action(async (slug, opts, cmd) => {
      const globalOpts = cmd.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const ws = requireWorkspace(globalOpts.workspace);
      const client = new CowikiClient(config.baseUrl, config.apiKey);
      const branch = await resolveUserBranch(client);

      const body = resolveBody(slug, opts.title, opts.body);

      const req: WritePageRequest = { slug, body, branch };
      if (opts.path) req.path = opts.path;
      if (opts.title) req.title = opts.title;
      if (opts.summary) req.summary = opts.summary;

      try {
        const resp = await client.writePage(ws, req);
        if (globalOpts.json) {
          printJson(resp);
        } else if (resp.ok) {
          printSuccess(`Page created/updated: ${resp.slug}`);
        } else {
          printError(`Page write returned ok=false for: ${resp.slug}`);
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });
}

function resolveBody(slug: string, title?: string, bodyArg?: string): string {
  // Explicit --body
  if (bodyArg !== undefined) {
    return bodyArg;
  }

  // Try stdin pipe
  if (!process.stdin.isTTY) {
    try {
      const buf = fs.readFileSync(process.stdin.fd, 'utf-8');
      const trimmed = buf.trim();
      if (trimmed) return trimmed;
    } catch {
      // fall through to editor
    }
  }

  // Open $EDITOR
  return editInEditor(slug, title);
}
