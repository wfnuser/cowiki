/**
 * Templates for new pages: a few built-in layouts for learning and meetings,
 * plus the Space's own template pages (Obsidian's Templates folder works as is).
 * Templates use {{title}}, {{date}} and {{time}} like Obsidian core templates.
 */

import { renderTemplate } from './journal.ts';

export interface BuiltInTemplate {
  id: string;
  name: string;
  type: string;
  body: string;
}

export const BUILT_IN_TEMPLATES: BuiltInTemplate[] = [
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

function hasFrontmatter(body: string): boolean {
  return /^---\r?\n[\s\S]*?\r?\n---(\r?\n|$)/.test(body);
}

/** Template pages in the Space: a top-level Templates folder plus configured template folders. */
export function templatePagePaths(paths: Iterable<string>, configuredTemplates: Array<string | null> = []): string[] {
  const folders = new Set(['templates']);
  for (const template of configuredTemplates) {
    const folder = template?.split('/').slice(0, -1).join('/');
    if (folder) folders.add(folder.toLowerCase());
  }
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
  return hasFrontmatter(rendered) ? rendered : `${frontmatter(template.type ?? 'Note')}${rendered}`;
}
