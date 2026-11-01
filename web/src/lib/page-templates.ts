/**
 * Templates for new pages are ordinary pages in the Space's templates folder
 * (Obsidian's Templates folder works as is), so people add, edit and delete
 * them like any note. They use {{title}}, {{date}} and {{time}} like Obsidian
 * core templates. The starter templates below are only written into that
 * folder when someone asks for them.
 */

import { renderTemplate, withTitle } from './journal.ts';

export interface StarterTemplate {
  id: string;
  name: string;
  type: string;
  body: string;
}

const STARTERS: StarterTemplate[] = [
  {
    id: 'learning-plan',
    name: 'Learning plan',
    type: 'Plan',
    body: [
      '# {{title}}',
      '',
      '## Goal',
      '',
      'What I want to be able to do, and by when.',
      '',
      '## Resources',
      '',
      '- ',
      '',
      '## Plan',
      '',
      '- [ ] ',
      '',
      '## Progress',
      '',
      'Level: **read** (seen it) → **recall** (can explain without notes) → **apply** (used it to solve something).',
      '',
      '| Date | What I did | Level |',
      '| --- | --- | --- |',
      '| {{date}} |  | read |',
      '',
      '## Practice',
      '',
      'Link practice logs, small projects or explanations here.',
      '',
      '## Open questions',
      '',
    ].join('\n'),
  },
  {
    id: 'practice-log',
    name: 'Practice log',
    type: 'Practice',
    body: [
      '# {{title}}',
      '',
      '**Date:** {{date}} · **Topic:** [[ ]] · **Level reached:** read / recall / apply',
      '',
      '## Task',
      '',
      'The exercise, project or explanation I attempted.',
      '',
      '## Attempt',
      '',
      '## Result',
      '',
      '## Feedback',
      '',
      '## Mistakes',
      '',
      '- ',
      '',
      '## Next',
      '',
      '- [ ] Practise again',
      '',
    ].join('\n'),
  },
  {
    id: 'meeting',
    name: 'Meeting notes',
    type: 'Meeting',
    body: [
      '# {{title}}',
      '',
      '**Date:** {{date}} {{time}} · **People:** ',
      '',
      '## Agenda',
      '',
      '## Notes',
      '',
      '## Decisions',
      '',
      '## Action items',
      '',
      '- [ ] ',
      '',
    ].join('\n'),
  },
];

function yamlString(value: string): string {
  return `"${value.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
}

/** Starter templates as files: frontmatter keeps the page type for pages made from them. */
export const STARTER_TEMPLATES: StarterTemplate[] = STARTERS.map((template) => ({
  ...template,
  body: `---\ntype: ${template.type}\ntitle: ${yamlString(template.name)}\n---\n\n${template.body}`,
}));

export function starterTemplatePath(folder: string, template: StarterTemplate): string {
  const clean = folder.replace(/^\/+|\/+$/g, '') || 'Templates';
  return `${clean}/${template.name}.md`;
}

function hasFrontmatter(body: string): boolean {
  return /^---\r?\n[\s\S]*?\r?\n---(\r?\n|$)/.test(body);
}

/** Template pages: the Space's templates folder plus the folder of the journal template. */
export function templatePagePaths(
  paths: Iterable<string>,
  features: { templates: { folder: string }; journal: { template: string | null } },
): string[] {
  const folders = new Set<string>();
  if (features.templates.folder) folders.add(features.templates.folder.toLowerCase());
  const journalFolder = features.journal.template?.split('/').slice(0, -1).join('/');
  if (journalFolder) folders.add(journalFolder.toLowerCase());
  return [...paths]
    .filter((path) => /\.md$/i.test(path))
    .filter((path) => {
      const lower = path.toLowerCase();
      return [...folders].some((folder) => lower.startsWith(`${folder}/`)) && !/(^|\/)(index|log)\.md$/.test(lower);
    })
    .sort((left, right) => left.localeCompare(right));
}

/**
 * Body of a new page. Without frontmatter of its own, the page gets a `type`
 * and its title so it shows correctly in the tree.
 */
export function newPageBody(
  title: string,
  template: { body: string; type?: string } | null,
  now = new Date(),
): string {
  const frontmatter = (type: string) => `---\ntype: ${type}\ntitle: ${yamlString(title)}\ndescription: ""\n---\n\n`;
  if (!template) return frontmatter('Note');
  const rendered = renderTemplate(template.body, now, title, now);
  return hasFrontmatter(rendered) ? withTitle(rendered, title) : `${frontmatter(template.type ?? 'Note')}${rendered}`;
}
