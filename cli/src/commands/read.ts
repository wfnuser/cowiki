import { Command } from 'commander';
import { spawn } from 'node:child_process';
import chalk from 'chalk';
import { loadConfig } from '../config.js';
import { CowikiClient } from '../client.js';
import { printError, printJson } from '../output.js';
import { resolveUserBranch, requireWorkspace } from '../shared.js';
import type { GlobalOpts } from '../shared.js';

export function registerReadCommand(program: Command): void {
  program
    .command('read')
    .description('Read a wiki page (supports --dir for wiki/entities/concepts)')
    .argument('<slug>', 'Page slug to read')
    .option('--no-pager', 'Print directly to stdout instead of using a pager')
    .option('--dir <dir>', 'Directory: wiki (default), entities, concepts, or subdir like wiki/messi')
    .action(async (slug, opts, cmd) => {
      const globalOpts = cmd.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const ws = requireWorkspace(globalOpts.workspace);
      const client = new CowikiClient(config.baseUrl, config.apiKey);
      const branch = await resolveUserBranch(client);

      try {
        const page = await client.getPage(ws, slug, branch, opts.dir);

        if (globalOpts.json) {
          printJson(page);
          return;
        }

        const fullOutput =
          `\n  ${chalk.bold(page.title)}\n` +
          `  slug: ${page.slug} | branch: ${page.branch}\n\n` +
          page.body;

        if (opts.noPager) {
          console.log(fullOutput);
        } else {
          const pager = process.env.PAGER || 'less -R';
          const [pagerCmd, ...pagerArgs] = pager.split(/\s+/);

          const child = spawn(pagerCmd, pagerArgs, {
            stdio: ['pipe', 'inherit', 'inherit'],
          });

          child.stdin!.write(fullOutput);
          child.stdin!.end();

          await new Promise<void>((resolve, reject) => {
            child.on('close', (code) => {
              if (code === 0 || code === null) resolve();
              else reject(new Error(`pager exited with code ${code}`));
            });
            child.on('error', reject);
          });
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        // Friendly 404
        if (msg.includes('404')) {
          printError(`page not found: "${slug}" on branch "${branch}"`);
        } else {
          printError(msg);
        }
        process.exit(1);
      }
    });
}
