import { Command } from 'commander';
import chalk from 'chalk';
import { loadConfig } from '../config.js';
import { CowikiClient } from '../client.js';
import { printSuccess, printError, printJson, printTable } from '../output.js';
import type { GlobalOpts } from '../shared.js';

export function registerKeyCommand(program: Command): void {
  const keyCmd = program
    .command('key')
    .description('Manage API keys');

  // ── key generate ──────────────────────────────────

  keyCmd
    .command('generate')
    .description('Generate a new API key')
    .requiredOption('--name <name>', 'A name/label for this key')
    .action(async (opts, cmd) => {
      const globalOpts = cmd.parent?.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const client = new CowikiClient(config.baseUrl, config.apiKey);

      try {
        const resp = await client.createKey(opts.name);

        if (globalOpts.json) {
          printJson(resp);
        } else {
          printSuccess(`Key created: ${resp.name}`);
          console.log(chalk.yellow('  Raw key (store this now — it won\'t be shown again!):'));
          console.log(`  ${resp.raw_key}`);
          console.log('');
          console.log(`  ID:        ${resp.id}`);
          console.log(`  Prefix:    ${resp.key_prefix}`);
          console.log(`  Created:   ${resp.created_at}`);
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });

  // ── key list ──────────────────────────────────────

  keyCmd
    .command('list')
    .description('List all API keys')
    .action(async (opts, cmd) => {
      const globalOpts = cmd.parent?.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const client = new CowikiClient(config.baseUrl, config.apiKey);

      try {
        const keys = await client.listKeys();

        if (globalOpts.json) {
          printJson(keys);
        } else if (keys.length === 0) {
          printSuccess('No API keys found.');
        } else {
          printTable(
            ['NAME', 'PREFIX', 'LAST USED', 'CREATED'],
            keys.map((k) => [
              k.name,
              k.key_prefix,
              k.last_used_at || '-',
              k.created_at,
            ]),
          );
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });

  // ── key revoke ────────────────────────────────────

  keyCmd
    .command('revoke')
    .description('Revoke an API key')
    .argument('<id>', 'Key ID to revoke')
    .action(async (id, opts, cmd) => {
      const globalOpts = cmd.parent?.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const client = new CowikiClient(config.baseUrl, config.apiKey);

      try {
        await client.revokeKey(id);
        printSuccess(`Key ${id} revoked`);
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });
}
