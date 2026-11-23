import type { BuiltinPlugin } from './host.ts';
import { ensureDatePage } from '../lib/date-page.ts';
import { journalPagePath } from '../lib/journal.ts';

export const dailyNotesPlugin: BuiltinPlugin = {
  id: 'journal', label: 'Daily journal', description: 'Journals: today first, earlier days as you scroll.',
  enabled: (features) => features.modules.journal,
  setEnabled: (features, enabled) => ({ ...features, modules: { ...features.modules, journal: enabled } }),
  actions: [{ id: 'today', label: 'Today', icon: 'calendar', run: async ({ pages, features, now }) => {
    const date = now();
    const path = await ensureDatePage(pages, features.journal, date, date);
    await pages.open(path);
  } }],
  settings: {
    title: 'Journal',
    fields: [
      { id: 'journal-folder', label: 'Folder', placeholder: 'Space root', value: (features) => features.journal.folder,
        update: (features, folder) => ({ ...features, journal: { ...features.journal, folder } }) },
      { id: 'journal-format', label: 'Date format', monospace: true, value: (features) => features.journal.format,
        update: (features, format) => ({ ...features, journal: { ...features.journal, format } }) },
      { id: 'journal-template', label: 'Template page', placeholder: 'Optional, e.g. Templates/Daily.md', value: (features) => features.journal.template ?? '',
        update: (features, template) => ({ ...features, journal: { ...features.journal, template: template.trim() ? template : null } }) },
    ],
    help: 'Dates use Obsidian (moment) tokens such as YYYY-MM-DD. Templates support {{date}}, {{title}} and {{time}}.',
    preview: { label: 'Today’s note:', value: (features, date) => journalPagePath(features.journal, date) },
  },
};
