import { Command } from 'commander';
import { loadConfig } from '../config.js';
import { CowikiClient } from '../client.js';
import { printSuccess, printError, printInfo, printJson, printTable } from '../output.js';
import { resolveUserBranch, requireWorkspace } from '../shared.js';
import type { GlobalOpts } from '../shared.js';

export function registerSearchCommand(program: Command): void {
  program
    .command('search')
    .description('Search wiki pages')
    .argument('<query>', 'Search query text')
    .option('--limit <limit>', 'Max results', '10')
    .action(async (query, opts, cmd) => {
      const globalOpts = cmd.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const client = new CowikiClient(config.baseUrl, config.apiKey);
      const branch = await resolveUserBranch(client);
      const limit = parseInt(opts.limit, 10);

      try {
        const results = await client.search(query, limit, branch);

        if (globalOpts.json) {
          printJson(results);
        } else if (results.length === 0) {
          printInfo(`no results for "${query}"`);
        } else {
          printTable(
            ['SLUG', 'TITLE', 'SUMMARY', 'SIMILARITY'],
            results.map((r) => [r.slug, r.title, r.summary, (r.similarity * 100).toFixed(1) + '%']),
          );
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });
}
