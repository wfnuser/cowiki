import { Command } from 'commander';
import { loadConfig } from '../config.js';
import { CowikiClient } from '../client.js';
import { printError, printInfo, printJson, printTable } from '../output.js';
import { resolveUserBranch, requireWorkspace } from '../shared.js';
import type { GlobalOpts } from '../shared.js';

export function registerListCommand(program: Command): void {
  program
    .command('list')
    .description('List wiki pages')
    .action(async (opts, cmd) => {
      const globalOpts = cmd.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const ws = requireWorkspace(globalOpts.workspace);
      const client = new CowikiClient(config.baseUrl, config.apiKey);
      const branch = await resolveUserBranch(client);

      try {
        const pages = await client.listPages(ws, branch);

        if (globalOpts.json) {
          printJson(pages);
        } else if (pages.length === 0) {
          printInfo(`no pages on branch "${branch}"`);
        } else {
          printTable(
            ['SLUG', 'TITLE', 'BRANCH'],
            pages.map((p) => [p.slug, p.title, p.branch]),
          );
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });
}
