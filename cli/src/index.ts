import { Command } from 'commander';
import { loadConfig, validateConfig } from './config.js';
import { CowikiClient } from './client.js';
import { printError, printJson } from './output.js';
import { CliError } from './error.js';
import type { GlobalOpts } from './shared.js';
import { registerSetupCommand } from './commands/setup.js';
import { registerKeyCommand } from './commands/key.js';
import { registerIngestCommand } from './commands/ingest.js';
import { registerCompileCommand } from './commands/compile.js';
import { registerWriteCommand } from './commands/write.js';
import { registerSearchCommand } from './commands/search.js';
import { registerReadCommand } from './commands/read.js';
import { registerListCommand } from './commands/list.js';
import { registerWorkspacesCommand } from './commands/workspaces.js';
import { registerSubmitCommand } from './commands/submit.js';
import { registerReviewCommand } from './commands/review.js';

// Re-export for backward compatibility
export type { GlobalOpts } from './shared.js';
export { resolveUserBranch, requireWorkspace } from './shared.js';

// ── Entry Point ─────────────────────────────────────────

const program = new Command();

program
  .name('cowiki')
  .version('0.1.0')
  .description('CLI client for cowiki - collaborative wiki')
  .option('--server <url>', 'Override server URL', process.env.COWIKI_BASE_URL)
  .option('-w, --workspace <slug>', 'Target workspace slug')
  .option('--json', 'Machine-readable JSON output');

// ── Register all commands ──────────────────────────────

registerSetupCommand(program);
registerKeyCommand(program);
registerIngestCommand(program);
registerCompileCommand(program);
registerWriteCommand(program);
registerSearchCommand(program);
registerReadCommand(program);
registerListCommand(program);
registerWorkspacesCommand(program);
registerSubmitCommand(program);
registerReviewCommand(program);

// ── Parse and run ──────────────────────────────────────

async function main() {
  try {
    await program.parseAsync(process.argv);
  } catch (e) {
    if (e instanceof CliError) {
      printError(e.message);
      process.exit(e.exitCode);
    }
    printError(e instanceof Error ? e.message : String(e));
    process.exit(1);
  }
}

main();
