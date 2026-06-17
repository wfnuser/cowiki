import { Command } from 'commander';
import { confirm } from '@inquirer/prompts';
import { loadConfig } from '../config.js';
import { CowikiClient } from '../client.js';
import { printSuccess, printError, printInfo, printJson, printWarning } from '../output.js';
import { resolveUserBranch, requireWorkspace } from '../shared.js';
import type { GlobalOpts } from '../shared.js';
import type { SubmitRequest, PageMeta } from '../types.js';

const KNOWN_DIRS = ['wiki', 'entities', 'concepts'];

/** Flatten a page tree to a list of repo paths, skipping folders and _index */
function flattenPaths(pages: PageMeta[]): string[] {
  const paths: string[] = [];
  for (const p of pages) {
    if (p.kind === 'folder' && p.children && p.children.length > 0) {
      paths.push(...flattenPaths(p.children));
    } else if (p.kind !== 'folder') {
      paths.push(p.path);
    }
  }
  return paths;
}

export function registerSubmitCommand(program: Command): void {
  program
    .command('submit')
    .description('Submit pages for review')
    .argument('[slugs...]', 'Page slugs to submit')
    .option('--all', 'Submit all pages on the branch')
    .option('--dir <dir>', 'Content directory (wiki, entities, concepts). Default: wiki', 'wiki')
    .option('-y, --yes', 'Skip confirmation prompt')
    .action(async (slugs, opts, cmd) => {
      const globalOpts = cmd.parent?.optsWithGlobals() as GlobalOpts;
      const config = loadConfig(globalOpts?.server)
      const ws = requireWorkspace(globalOpts.workspace);
      const client = new CowikiClient(config.baseUrl, config.apiKey);
      const branch = await resolveUserBranch(client);

      const dir = opts.dir || 'wiki';

      // Resolve page paths
      let pagePaths: string[];
      if (opts.all) {
        const pages = await client.listPages(ws, branch, dir === 'all' ? 'all' : dir);
        if (pages.length === 0) {
          printInfo(`no pages on branch "${branch}" in ${dir}`);
          return;
        }
        pagePaths = flattenPaths(pages);
      } else if (!slugs || slugs.length === 0) {
        printError('No slugs specified. Provide slugs or use --all.');
        process.exit(1);
      } else {
        // Convert slugs to paths, respecting --dir
        pagePaths = slugs.map((slug: string) => {
          // If slug already starts with a known dir, pass through as-is
          if (KNOWN_DIRS.some(d => slug.startsWith(`${d}/`))) {
            return slug;
          }
          return `${dir}/${slug}`;
        });
      }

      // Show summary
      printInfo(
        `Submitting ${pagePaths.length} page(s) from branch "${branch}": ${pagePaths.join(', ')}`,
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

      const req: SubmitRequest = { branch, paths: pagePaths };

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
                `  ${dup.new_path} ↔ ${dup.existing_path} (${(dup.similarity * 100).toFixed(1)}%)`,
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
