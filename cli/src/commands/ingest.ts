import { Command } from 'commander';
import fs from 'node:fs';
import path from 'node:path';
import { loadConfig } from '../config.js';
import { CowikiClient } from '../client.js';
import { printSuccess, printError, printJson } from '../output.js';
import { resolveUserBranch, requireWorkspace } from '../shared.js';
import type { GlobalOpts } from '../shared.js';
import type { IngestRequest } from '../types.js';

export function registerIngestCommand(program: Command): void {
  program
    .command('ingest')
    .description('Ingest a source document into the wiki')
    .requiredOption('--type <type>', 'Source type: url, text, or file', 'url')
    .option('--content <content>', 'The content (URL, text, or file path)')
    .action(async (opts, cmd) => {
      const globalOpts = cmd.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const ws = requireWorkspace(globalOpts.workspace);
      const client = new CowikiClient(config.baseUrl, config.apiKey);
      const branch = await resolveUserBranch(client);

      const content = resolveContent(opts.type, opts.content);
      const req: IngestRequest = {
        source_type: opts.type,
        content,
        branch,
      };

      try {
        const resp = await client.ingest(ws, req);
        if (globalOpts.json) {
          printJson(resp);
        } else {
          printSuccess(`Ingested: ${resp.filename} (hash: ${resp.content_hash})`);
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });
}

function resolveContent(sourceType: string, contentArg?: string): string {
  if (contentArg !== undefined) {
    switch (sourceType) {
      case 'url': {
        if (!contentArg.startsWith('http://') && !contentArg.startsWith('https://')) {
          printError('URL must start with http:// or https://');
          process.exit(1);
        }
        return contentArg;
      }
      case 'text':
        return contentArg;
      case 'file': {
        // Resolve to absolute path and restrict to cwd
        const resolved = path.resolve(contentArg);
        const cwd = path.resolve(process.cwd());
        if (!resolved.startsWith(cwd + path.sep) && resolved !== cwd) {
          printError(`File must be within the current working directory: ${contentArg}`);
          process.exit(1);
        }
        try {
          return fs.readFileSync(resolved, 'utf-8');
        } catch (e) {
          printError(`Cannot read file '${contentArg}': ${e instanceof Error ? e.message : e}`);
          process.exit(1);
        }
      }
    }
  }

  // Read from stdin
  if (process.stdin.isTTY) {
    printError('No --content provided and no stdin pipe. Provide --content or pipe input.');
    process.exit(1);
  }

  let buf = '';
  try {
    buf = fs.readFileSync(process.stdin.fd, 'utf-8');
  } catch (e) {
    printError(`Failed to read stdin: ${e instanceof Error ? e.message : e}`);
    process.exit(1);
  }

  const trimmed = buf.trim();
  if (!trimmed) {
    printError('Stdin was empty.');
    process.exit(1);
  }
  return trimmed;
}
