import { Command } from 'commander';
import { loadConfig } from '../config.js';
import { CowikiClient } from '../client.js';
import { printSuccess, printError, printInfo, printJson, printTable } from '../output.js';
import { resolveUserBranch, requireWorkspace } from '../shared.js';
import type { GlobalOpts } from '../shared.js';

export function registerSearchCommand(program: Command): void {
  program
    .command('search')
    .description('Search wiki pages (keyword + semantic)')
    .argument('<query>', 'Search query text')
    .option('--limit <limit>', 'Max results per group', '12')
    .option('--mode <mode>', 'keyword | semantic | all (default)')
    .action(async (query, opts, cmd) => {
      const globalOpts = cmd.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const client = new CowikiClient(config.baseUrl, config.apiKey);
      const ws = requireWorkspace(globalOpts.workspace);
      const limit = parseInt(opts.limit, 10);

      try {
        const response = await client.search(ws, query, limit);

        if (globalOpts.json) {
          printJson(response);
          return;
        }

        const total = response.keyword.length + response.semantic.length;
        if (total === 0) {
          printInfo(`no results for "${query}"`);
          return;
        }

        // Keyword results
        if (response.keyword.length > 0) {
          printSuccess('── Keyword ──');
          printTable(
            ['SLUG', 'TITLE', 'SNIPPET'],
            response.keyword.map((r) => [r.slug, r.title_match ? `★ ${r.title}` : r.title, r.snippet]),
          );
        }

        // Semantic results
        if (response.semantic.length > 0) {
          printSuccess('── Semantic ──');
          printTable(
            ['SLUG', 'TITLE', 'SOURCE', 'SIMILARITY'],
            response.semantic.map((r) => [r.slug, r.title, r.source, (r.similarity * 100).toFixed(1) + '%']),
          );
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });
}
