import { Command } from 'commander';
import { loadConfig } from '../config.js';
import { CowikiClient } from '../client.js';
import { printError, printInfo, printJson, printTable } from '../output.js';
import type { GlobalOpts } from '../shared.js';

export function registerWorkspacesCommand(program: Command): void {
  program
    .command('workspaces')
    .description('List available workspaces')
    .action(async (opts, cmd) => {
      const globalOpts = cmd.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const client = new CowikiClient(config.baseUrl, config.apiKey);

      try {
        const workspaces = await client.listWorkspaces();

        if (globalOpts.json) {
          printJson(workspaces);
        } else if (workspaces.length === 0) {
          printInfo('no workspaces found');
        } else {
          printTable(
            ['NAME', 'SLUG', 'ROLE', 'VISIBILITY'],
            workspaces.map((w) => [w.name, w.slug, w.role, w.visibility]),
          );
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });
}
