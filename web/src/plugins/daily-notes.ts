import type { BuiltinPlugin } from './host.ts';
import { ensureDatePage } from '../lib/date-page.ts';
import { journalPagePath } from '../lib/journal.ts';

export const dailyNotesPlugin: BuiltinPlugin = {
  id: 'journal', label: 'Daily journal', description: 'Open or create today’s Markdown journal.',
  enabled: (features) => features.modules.journal,
  setEnabled: (features, enabled) => ({ ...features, modules: { ...features.modules, journal: enabled } }),
  actions: [{ id: 'today', label: 'Today', icon: 'calendar', run: async ({ pages, features, now }) => {
    const date = now();
    const path = await ensureDatePage(pages, features.journal, date, date);
    await pages.open(path);
  } }],
  launch: { action: 'today', when: (preferences) => preferences.openJournalOnStartup },
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
    help: 'Formats use Obsidian/moment tokens such as YYYY-MM-DD or YYYY/MM/YYYY-MM-DD; templates support {{date}}, {{title}} and {{time}}.',
    preview: (features, date) => `Today’s entry: ${journalPagePath(features.journal, date)}`,
    preferences: [{ id: 'journal-startup', label: 'Open today’s journal when CoWiki starts',
      value: (preferences) => preferences.openJournalOnStartup,
      update: (preferences, openJournalOnStartup) => ({ ...preferences, openJournalOnStartup }) }],
  },
};
