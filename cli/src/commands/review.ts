import { Command } from 'commander';
import chalk from 'chalk';
import { loadConfig } from '../config.js';
import { CowikiClient } from '../client.js';
import { printSuccess, printError, printInfo, printJson, printTable } from '../output.js';
import { requireWorkspace } from '../shared.js';
import type { GlobalOpts } from '../shared.js';

export function registerReviewCommand(program: Command): void {
  const reviewCmd = program
    .command('review')
    .description('Review submissions');

  // ── review list ────────────────────────────────

  reviewCmd
    .command('list')
    .description('List review submissions')
    .option('--status <status>', 'Filter by status (pending/approved/rejected)')
    .action(async (opts, cmd) => {
      const globalOpts = (cmd.parent?.parent as Command)?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const ws = requireWorkspace(globalOpts.workspace);
      const client = new CowikiClient(config.baseUrl, config.apiKey);

      try {
        let submissions = await client.listReviews(ws);

        // Client-side status filtering
        if (opts.status) {
          const statusFilter = opts.status.toLowerCase();
          submissions = submissions.filter((s) => s.status.toLowerCase() === statusFilter);
        }

        if (globalOpts.json) {
          printJson(submissions);
        } else if (submissions.length === 0) {
          printInfo('no reviews found');
        } else {
          printTable(
            ['ID', 'USER', 'STATUS', 'SUMMARY', 'CREATED'],
            submissions.map((s) => {
              let statusColored: string;
              switch (s.status) {
                case 'pending': statusColored = chalk.yellow(s.status); break;
                case 'approved': statusColored = chalk.green(s.status); break;
                case 'rejected': statusColored = chalk.red(s.status); break;
                default: statusColored = s.status;
              }
              return [s.id, s.user_id, statusColored, s.summary, s.created_at];
            }),
          );
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });

  // ── review show ────────────────────────────────

  reviewCmd
    .command('show')
    .description('Show review details with diffs')
    .argument('<id>', 'Submission ID')
    .action(async (id, opts, cmd) => {
      const globalOpts = (cmd.parent?.parent as Command)?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const ws = requireWorkspace(globalOpts.workspace);
      const client = new CowikiClient(config.baseUrl, config.apiKey);

      try {
        const detail = await client.getReview(ws, id);

        if (globalOpts.json) {
          printJson(detail);
          return;
        }

        const sub = detail.submission;
        let statusColored: string;
        switch (sub.status) {
          case 'pending': statusColored = chalk.yellow(sub.status); break;
          case 'approved': statusColored = chalk.green(sub.status); break;
          case 'rejected': statusColored = chalk.red(sub.status); break;
          default: statusColored = sub.status;
        }

        console.log(`${chalk.bold('Submission:')} ${sub.id}`);
        console.log(`  User:     ${sub.user_id}`);
        console.log(`  Status:   ${statusColored}`);
        console.log(`  Summary:  ${sub.summary}`);
        console.log(`  Branch:   ${sub.source_branch}`);
        console.log(`  Created:  ${sub.created_at}`);
        if (sub.reviewed_by) console.log(`  Reviewed by: ${sub.reviewed_by}`);
        if (sub.reviewed_at) console.log(`  Reviewed at: ${sub.reviewed_at}`);
        console.log();

        if (!detail.diffs || detail.diffs.length === 0) {
          printInfo('no diffs');
        } else {
          for (const diff of detail.diffs) {
            console.log(`${chalk.yellow('---')} ${diff.path}`);
            if (diff.old_content) {
              for (const line of diff.old_content.split('\n')) {
                console.log(chalk.red(`- ${line}`));
              }
            }
            if (diff.new_content) {
              for (const line of diff.new_content.split('\n')) {
                console.log(chalk.green(`+ ${line}`));
              }
            }
            console.log();
          }
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        if (msg.includes('404')) {
          printError(`review not found: "${id}"`);
        } else {
          printError(msg);
        }
        process.exit(1);
      }
    });

  // ── review approve ─────────────────────────────

  reviewCmd
    .command('approve')
    .description('Approve a submission')
    .argument('<id>', 'Submission ID')
    .action(async (id, opts, cmd) => {
      const globalOpts = (cmd.parent?.parent as Command)?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const ws = requireWorkspace(globalOpts.workspace);
      const client = new CowikiClient(config.baseUrl, config.apiKey);

      try {
        await client.approveReview(ws, id);
        if (globalOpts.json) {
          console.log('{"ok":true}');
        } else {
          printSuccess(`approved submission "${id}"`);
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });

  // ── review reject ──────────────────────────────

  reviewCmd
    .command('reject')
    .description('Reject a submission')
    .argument('<id>', 'Submission ID')
    .action(async (id, opts, cmd) => {
      const globalOpts = (cmd.parent?.parent as Command)?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const ws = requireWorkspace(globalOpts.workspace);
      const client = new CowikiClient(config.baseUrl, config.apiKey);

      try {
        await client.rejectReview(ws, id);
        if (globalOpts.json) {
          console.log('{"ok":true}');
        } else {
          printSuccess(`rejected submission "${id}"`);
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });
}
