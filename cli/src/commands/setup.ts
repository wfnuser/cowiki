import { Command } from 'commander';
import chalk from 'chalk';
import { input, password, confirm } from '@inquirer/prompts';
import { loadConfig, writeEnvFile, DEFAULT_ENV_PATH } from '../config.js';
import { CowikiClient } from '../client.js';
import { printSuccess, printError, printInfo, printWarning } from '../output.js';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { GlobalOpts } from '../shared.js';

export function registerSetupCommand(program: Command): void {
  program
    .command('setup')
    .description('Interactive configuration wizard')
    .option('--api-key <key>', 'Set API key directly (non-interactive, for scripting/tests)')
    .option('--env-path <path>', 'Path to .env file', DEFAULT_ENV_PATH)
    .action(async (opts, cmd) => {
      const globalOpts = cmd.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server);
      const envPath = path.resolve(opts.envPath.replace(/^~/, os.homedir()));

      // ── Non-interactive mode: --api-key ──
      if (opts.apiKey) {
        const serverUrl = globalOpts?.server || config.baseUrl;

        // Validate key
        const tempClient = new CowikiClient(serverUrl, opts.apiKey);
        try {
          const user = await tempClient.getMe();
          writeEnvFile({
            COWIKI_BASE_URL: serverUrl,
            COWIKI_API_KEY: opts.apiKey,
          }, envPath);
          printSuccess(`API key configured — connected as ${user.name} (${user.id})`);
          printInfo(`Saved to ${envPath}`);
        } catch {
          printError(
            `Failed to validate API key. Make sure the server is running at ${serverUrl} and the key is correct.`,
          );
          process.exit(1);
        }
        return;
      }

      // ── Interactive mode ──

      // Check if .env already exists
      if (fs.existsSync(envPath)) {
        printWarning('A .env file already exists.');
        const overwrite = await confirm({
          message: 'Overwrite existing configuration?',
          default: false,
        });
        if (!overwrite) {
          printInfo('Setup cancelled.');
          return;
        }
      }

      // 1. Server URL
      const serverUrl = await input({
        message: 'cowiki server URL:',
        default: config.baseUrl,
      });

      // 2. API key
      const hasKey = await confirm({
        message: 'Do you have an API key?',
        default: false,
      });

      let apiKey = '';
      if (hasKey) {
        apiKey = await password({
          message: 'Enter your API key:',
        });

        // Validate key
        const tempClient = new CowikiClient(serverUrl, apiKey);
        try {
          const user = await tempClient.getMe();
          printSuccess(`Key validated — connected as ${user.name}`);
        } catch {
          printError('API key validation failed. You can re-run setup to try again.');
          process.exit(1);
        }
      }

      // 3. Write .env
      writeEnvFile({
        COWIKI_BASE_URL: serverUrl,
        COWIKI_API_KEY: apiKey,
      }, envPath);

      printSuccess(`Configuration saved to ${envPath}`);

      if (!apiKey) {
        printInfo('No API key set. Visit the cowiki website to get an API key, then run "cowiki setup --api-key <key>".');
      }

      console.log('');
      printInfo('Tip: Pass -w <slug> on workspace-scoped commands. Run "cowiki workspaces" to list available workspaces.');
    });
}
