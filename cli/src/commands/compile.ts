import { Command } from 'commander';
import ora from 'ora';
import { loadConfig } from '../config.js';
import { CowikiClient } from '../client.js';
import { printSuccess, printError, printInfo, printJson, printTable } from '../output.js';
import { resolveUserBranch, requireWorkspace } from '../shared.js';
import type { GlobalOpts } from '../shared.js';

export function registerCompileCommand(program: Command): void {
  program
    .command('compile')
    .description('Compile sources into wiki pages')
    .option('--timeout <seconds>', 'Timeout in seconds', '120')
    .action(async (opts, cmd) => {
      const globalOpts = cmd.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const ws = requireWorkspace(globalOpts.workspace);
      const client = new CowikiClient(config.baseUrl, config.apiKey);
      const branch = await resolveUserBranch(client);

      const spinner = ora({ text: 'Compiling sources...', spinner: 'dots' }).start();

      let cancelled = false;
      const sigintHandler = () => {
        cancelled = true;
        spinner.stopAndPersist({ text: 'Compilation request sent, server may still be processing' });
      };
      process.once('SIGINT', sigintHandler);

      try {
        const controller = new AbortController();
        const timeoutMs = parseInt(opts.timeout, 10) * 1000;
        const timeoutId = setTimeout(() => controller.abort(), timeoutMs);

        // Call compile
        const compilePromise = client.compile(ws, { branch });

        // Race against timeout
        const result = await Promise.race([
          compilePromise,
          new Promise<never>((_, reject) => {
            const onAbort = () => reject(new Error('TIMEOUT'));
            controller.signal.addEventListener('abort', onAbort, { once: true });
          }),
        ]);

        clearTimeout(timeoutId);
        spinner.stop();

        if (cancelled) return;

        if (globalOpts.json) {
          printJson(result.pages);
        } else if (result.pages.length === 0) {
          printInfo('No sources to compile or all already compiled.');
          if (result.skipped > 0) {
            printInfo(`${result.skipped} source(s) skipped (already compiled).`);
          }
        } else {
          printTable(
            ['SLUG', 'TITLE', 'SUMMARY'],
            result.pages.map((p) => [p.slug, p.title, p.summary]),
          );
          if (result.skipped > 0) {
            printInfo(`${result.skipped} source(s) skipped (already compiled).`);
          }
        }
      } catch (e) {
        spinner.stop();
        const msg = e instanceof Error ? e.message : String(e);
        if (msg === 'TIMEOUT') {
          printInfo('Compilation still in progress, check server later');
        } else {
          printError(msg);
          process.exit(1);
        }
      } finally {
        process.removeListener('SIGINT', sigintHandler);
      }
    });
}
