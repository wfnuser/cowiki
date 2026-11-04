import { journalPagePath, newJournalBody, type JournalConfig } from './journal.ts';
import type { SpacePageAdapter } from '../plugins/space-pages.ts';

/** Shared file convention, not a plugin command: Tasks can add to today even when Daily Notes is disabled. */
export async function ensureDatePage(pages: SpacePageAdapter, config: JournalConfig, date: Date, now = new Date()): Promise<string> {
  const path = journalPagePath(config, date);
  if (await pages.read(path) !== null) return path;
  const template = config.template ? await pages.read(config.template) : null;
  await pages.createOnly(path, newJournalBody(config, date, template, now));
  await pages.refresh();
  return path;
}
