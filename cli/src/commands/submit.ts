import { Command } from 'commander';
import { confirm } from '@inquirer/prompts';
import { loadConfig } from '../config.js';
import { CowikiClient } from '../client.js';
import { printSuccess, printError, printInfo, printJson, printWarning } from '../output.js';
import { resolveUserBranch, requireWorkspace } from '../shared.js';
import type { GlobalOpts } from '../shared.js';
import type { SubmitRequest, PageMeta } from '../types.js';

export function registerSubmitCommand(program: Command): void {
  program
    .command('submit')
    .description('Submit pages for review')
    .argument('[slugs...]', 'Page slugs to submit')
    .option('--all', 'Submit all pages on the branch')
    .option('--dir <dir>', 'Directory: wiki (default), entities, concepts, all, or subdir like wiki/research')
    .option('-y, --yes', 'Skip confirmation prompt')
    .action(async (slugs, opts, cmd) => {
      const globalOpts = cmd.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const ws = requireWorkspace(globalOpts.workspace);
      const client = new CowikiClient(config.baseUrl, config.apiKey);
      const branch = await resolveUserBranch(client);

      // Resolve page slugs — the listing is a tree (folders have children),
      // so flatten it to collect every leaf page slug.
      let pageSlugs: string[];
      if (opts.all) {
        const pages = await client.listPages(ws, branch, opts.dir);
        if (pages.length === 0) {
          printInfo(`no pages on branch "${branch}"${opts.dir ? ` in dir "${opts.dir}"` : ''}`);
          return;
        }
        pageSlugs = flattenSlugs(pages);
      } else if (!slugs || slugs.length === 0) {
        printError('No slugs specified. Provide slugs or use --all.');
        process.exit(1);
      } else {
        pageSlugs = slugs;
      }

      // Show summary
      printInfo(
        `Submitting ${pageSlugs.length} page(s) from branch "${branch}": ${pageSlugs.join(', ')}`,
      );

      // Confirm
      if (!opts.yes) {
        const proceed = await confirm({
          message: 'Proceed?',
          default: false,
        });
        if (!proceed) {
          printInfo('Cancelled.');
          return;
        }
      }

      const req: SubmitRequest = { branch, page_slugs: pageSlugs };

      try {
        const resp = await client.submit(ws, req);

        if (globalOpts.json) {
          printJson(resp);
        } else {
          printSuccess(`Submission created: ${resp.submission_id} — ${resp.summary}`);

          if (resp.duplicates && resp.duplicates.length > 0) {
            console.error();
            printWarning('Duplicate warnings:');
            for (const dup of resp.duplicates) {
              console.error(
                `  ${dup.new_slug} ↔ ${dup.existing_slug} (${(dup.similarity * 100).toFixed(1)}%)`,
              );
            }
          }
        }
      } catch (e) {
        printError(e instanceof Error ? e.message : String(e));
        process.exit(1);
      }
    });
}

/** Flatten a page tree into a flat list of slugs (pages only, not synthetic folders). */
function flattenSlugs(pages: PageMeta[]): string[] {
  const slugs: string[] = [];
  for (const p of pages) {
    if (p.kind === 'folder') {
      // Only include the folder's _index if it has children with real content.
      // A folder with no children and no backing _index.md is synthetic — skip it.
      if (p.children && p.children.length > 0) {
        slugs.push(...flattenSlugs(p.children));
      }
    } else {
      slugs.push(p.slug);
    }
  }
  return slugs;
}
