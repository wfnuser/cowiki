/**
 * Turn a read-only Obsidian vault scan into the Space settings and the
 * Markdown import report written into the imported copy.
 */
import {
  DEFAULT_JOURNAL_CONFIG,
  journalConfigFromObsidian,
  journalEntryKeys,
  parseDate,
  type JournalConfig,
} from './journal.ts';
import { DEFAULT_SPACE_FEATURES, withSpaceFeatures, type SpaceFeatures } from './space-features.ts';

/** The fields of the desktop vault scan this module relies on. */
export interface VaultScan {
  vaultPath: string;
  vaultName: string;
  markdownFiles: number;
  attachmentFiles: number;
  otherFiles: number;
  folders: number;
  totalBytes: number;
  attachmentTypes: Record<string, number>;
  wikilinks: number;
  markdownLinks: number;
  embeds: number;
  headingLinks: number;
  blockLinks: number;
  blockIds: number;
  unresolvedLinks: number;
  unresolvedExamples: string[];
  tags: number;
  taskLines: number;
  filesWithFrontmatter: number;
  invalidFrontmatter: string[];
  callouts: number;
  dataviewBlocks: number;
  tasksQueryBlocks: number;
  templaterFiles: string[];
  excalidrawFiles: number;
  canvasFiles: number;
  duplicateNames: Array<{ name: string; paths: string[] }>;
  duplicateNameCount: number;
  reservedNames: string[];
  nonUtf8Files: string[];
  topLevelSourcesFolder: boolean;
  symlinksSkipped: number;
  largeFiles: number;
  largeFileBytes: number;
  largeFileExamples: string[];
  markdownPaths: string[];
  dailyNotes: unknown | null;
  templatesFolder: string | null;
  attachmentFolder: string | null;
  linkFormat: string | null;
  communityPlugins: string[];
}

export type JournalSource = 'obsidian-settings' | 'detected' | 'default';

export interface ImportPlan {
  journal: JournalConfig;
  journalSource: JournalSource;
  journalEntries: number;
  features: SpaceFeatures;
}

/** Most common folder of `YYYY-MM-DD.md` files, when no settings exist. */
function detectJournalFolder(paths: string[]): string | null {
  const counts = new Map<string, number>();
  for (const path of paths) {
    const slash = path.lastIndexOf('/');
    const name = path.slice(slash + 1).replace(/\.md$/i, '');
    if (parseDate(name, 'YYYY-MM-DD')) {
      const folder = slash < 0 ? '' : path.slice(0, slash);
      counts.set(folder, (counts.get(folder) ?? 0) + 1);
    }
  }
  let best: string | null = null;
  let bestCount = 0;
  for (const [folder, count] of counts) {
    if (count > bestCount) {
      best = folder;
      bestCount = count;
    }
  }
  return bestCount >= 3 ? best : null;
}

export function planImport(scan: VaultScan): ImportPlan {
  let journal: JournalConfig;
  let journalSource: JournalSource;
  const fromSettings = scan.dailyNotes ? journalConfigFromObsidian(scan.dailyNotes) : null;
  if (fromSettings) {
    journal = fromSettings;
    journalSource = 'obsidian-settings';
  } else {
    const detected = detectJournalFolder(scan.markdownPaths);
    if (detected !== null) {
      journal = { folder: detected, format: 'YYYY-MM-DD', template: null };
      journalSource = 'detected';
    } else {
      journal = { ...DEFAULT_JOURNAL_CONFIG };
      journalSource = 'default';
    }
  }
  return {
    journal,
    journalSource,
    journalEntries: journalEntryKeys(journal, scan.markdownPaths).length,
    features: {
      ...DEFAULT_SPACE_FEATURES,
      modules: { ...DEFAULT_SPACE_FEATURES.modules },
      journal,
      // Obsidian's core Templates folder stays the place for templates.
      templates: { folder: scan.templatesFolder?.replace(/^\/+|\/+$/g, '') || DEFAULT_SPACE_FEATURES.templates.folder },
    },
  };
}

export function importSpaceConfig(plan: ImportPlan, scan: VaultScan): Record<string, unknown> {
  return {
    ...withSpaceFeatures({}, plan.features),
    import: {
      from: 'obsidian',
      vaultName: scan.vaultName,
      attachmentFolder: scan.attachmentFolder,
      templatesFolder: scan.templatesFolder,
    },
  };
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function list(items: string[], more = 0): string[] {
  const lines = items.map((item) => `  - \`${item.replaceAll('`', "'")}\``);
  if (more > 0) lines.push(`  - …and ${more} more`);
  return lines;
}

export interface ImportWarning {
  level: 'info' | 'warning';
  text: string;
}

/** Things worth knowing before importing, shown in the dialog and the report. */
export function importWarnings(scan: VaultScan): ImportWarning[] {
  const warnings: ImportWarning[] = [];
  if (scan.unresolvedLinks > 0) {
    warnings.push({ level: 'info', text: `${scan.unresolvedLinks} links point to notes or files that do not exist in the vault (they were already unresolved in Obsidian).` });
  }
  if (scan.duplicateNameCount > 0) {
    warnings.push({ level: 'info', text: `${scan.duplicateNameCount} note names are used in more than one folder; [[Name]] resolves to the note in the same folder, then the shortest path, as in Obsidian.` });
  }
  if (scan.invalidFrontmatter.length > 0) {
    warnings.push({ level: 'warning', text: `${scan.invalidFrontmatter.length} notes have frontmatter that is not valid YAML; CoWiki adds a new properties block above it instead of reading it.` });
  }
  if (scan.reservedNames.length > 0) {
    warnings.push({ level: 'warning', text: `${scan.reservedNames.length} notes are named index.md or log.md, which CoWiki uses for folder listings and change logs; their content is kept but may gain a generated listing (log.md is archived under .cowiki/legacy).` });
  }
  if (scan.topLevelSourcesFolder) {
    warnings.push({ level: 'warning', text: 'A top-level "sources" folder is treated as CoWiki raw sources and moved under .cowiki/sources in the imported copy.' });
  }
  if (scan.dataviewBlocks + scan.tasksQueryBlocks > 0) {
    warnings.push({ level: 'info', text: `${scan.dataviewBlocks} Dataview and ${scan.tasksQueryBlocks} Tasks query blocks are shown as code; use the Tasks view for task lists.` });
  }
  if (scan.templaterFiles.length > 0) {
    warnings.push({ level: 'info', text: `${scan.templaterFiles.length}+ notes contain Templater code (<% %>), which is kept as text and not executed.` });
  }
  if (scan.canvasFiles + scan.excalidrawFiles > 0) {
    warnings.push({ level: 'info', text: `${scan.canvasFiles} Canvas and ${scan.excalidrawFiles} Excalidraw files are copied but not rendered yet.` });
  }
  if (scan.nonUtf8Files.length > 0) {
    warnings.push({ level: 'warning', text: `${scan.nonUtf8Files.length} notes are not UTF-8; their original bytes are preserved under .cowiki/legacy.` });
  }
  if (scan.symlinksSkipped > 0) {
    warnings.push({ level: 'warning', text: `${scan.symlinksSkipped} symbolic links are skipped for safety.` });
  }
  if (scan.largeFiles > 0) {
    warnings.push({ level: 'info', text: `${scan.largeFiles} files of 5 MB or more (${formatBytes(scan.largeFileBytes)}, such as PDFs or recordings) are copied and open normally, but are left out of version history to keep the Space small.` });
  }
  return warnings;
}

export function importReportMarkdown(scan: VaultScan, plan: ImportPlan, importedAt: Date, destination: string): string {
  const date = `${importedAt.getFullYear()}-${String(importedAt.getMonth() + 1).padStart(2, '0')}-${String(importedAt.getDate()).padStart(2, '0')}`;
  const journalWhere = plan.journal.folder ? `\`${plan.journal.folder}/\`` : 'the Space root';
  const journalHow = {
    'obsidian-settings': 'from the vault\'s Daily Notes settings',
    detected: 'detected from date-named notes',
    default: 'CoWiki default (no daily notes found)',
  }[plan.journalSource];
  const attachments = Object.entries(scan.attachmentTypes)
    .sort((left, right) => right[1] - left[1])
    .map(([extension, count]) => `${extension} ${count}`)
    .join(', ') || 'none';
  const warnings = importWarnings(scan);
  const lines = [
    '---',
    'type: Note',
    'title: "CoWiki Import Report"',
    `description: "Obsidian vault ${scan.vaultName.replaceAll('"', "'")} imported on ${date}"`,
    '---',
    '',
    '# CoWiki Import Report',
    '',
    `Imported **${scan.vaultName}** on ${date}. The original vault at \`${scan.vaultPath}\` was only read; this Space is a copy at \`${destination}\`.`,
    '',
    '## What was copied',
    '',
    '| Item | Count |',
    '| --- | --- |',
    `| Markdown notes | ${scan.markdownFiles} |`,
    `| Attachments | ${scan.attachmentFiles} (${attachments}) |`,
    `| Other files | ${scan.otherFiles} |`,
    `| Folders | ${scan.folders} |`,
    `| Total size | ${formatBytes(scan.totalBytes)} |`,
    '',
    'Folder structure, Markdown, attachments, frontmatter properties and `.obsidian` settings are kept as they were. The import is recorded as the first commit in History; CoWiki\'s format normalization (adding `type`/`title` properties where missing and a folder listing `index.md` per folder) is a separate second commit you can inspect.',
    '',
    '## Links and syntax',
    '',
    `- Wikilinks: ${scan.wikilinks} (${scan.embeds} embeds, ${scan.headingLinks} heading links, ${scan.blockLinks} block links); Markdown links: ${scan.markdownLinks}.`,
    `- Unresolved links: ${scan.unresolvedLinks}.`,
    `- Tags: ${scan.tags}; callouts: ${scan.callouts}; block IDs: ${scan.blockIds}; task lines: ${scan.taskLines}.`,
    '- `[[Name]]`, `[[Name|alias]]`, `[[Name#Heading]]` and `![[image.png]]` work in reading and editing views. Block links open the page (scrolling to the block is not supported yet); note embeds show as links.',
    '',
    '## Journal',
    '',
    `- Location ${journalWhere}, format \`${plan.journal.format}\` (${journalHow}); ${plan.journalEntries} existing entries recognised.`,
    plan.journal.template ? `- New entries use the template \`${plan.journal.template}\`.` : '- New entries use CoWiki\'s Plan / Notes / Review layout.',
    '- Change this under Modules & settings in the sidebar.',
    '',
  ];
  if (scan.communityPlugins.length > 0) {
    lines.push('## Obsidian plugins', '', `Enabled community plugins: ${scan.communityPlugins.map((plugin) => `\`${plugin}\``).join(', ')}. Plugin-specific syntax is kept as text.`, '');
  }
  if (warnings.length > 0) {
    lines.push('## Check these', '');
    for (const warning of warnings) lines.push(`- ${warning.level === 'warning' ? '⚠️' : 'ℹ️'} ${warning.text}`);
    lines.push('');
  }
  const details: Array<[string, string[], number]> = [
    ['Unresolved link examples', scan.unresolvedExamples, Math.max(0, scan.unresolvedLinks - scan.unresolvedExamples.length)],
    ['Notes with invalid frontmatter', scan.invalidFrontmatter, 0],
    ['Notes named index.md / log.md', scan.reservedNames, 0],
    ['Notes with Templater code', scan.templaterFiles, 0],
    ['Non-UTF-8 notes', scan.nonUtf8Files, 0],
    ['Large files left out of version history (listed in .gitignore)', scan.largeFileExamples, Math.max(0, scan.largeFiles - scan.largeFileExamples.length)],
  ];
  for (const [title, items, more] of details) {
    if (items.length === 0) continue;
    lines.push(`### ${title}`, '', ...list(items, more), '');
  }
  if (scan.duplicateNames.length > 0) {
    lines.push('### Note names used in several folders', '');
    for (const duplicate of scan.duplicateNames) {
      lines.push(`- **${duplicate.name}**: ${duplicate.paths.map((path) => `\`${path}\``).join(', ')}`);
    }
    if (scan.duplicateNameCount > scan.duplicateNames.length) {
      lines.push(`- …and ${scan.duplicateNameCount - scan.duplicateNames.length} more`);
    }
    lines.push('');
  }
  lines.push('## Retry', '', 'If the import stopped part-way, import the same vault into the same folder again; files are copied again and the vault stays untouched.', '');
  return lines.join('\n');
}

/** Folder name for the imported copy: the vault name, made filesystem-safe. */
export function importFolderName(vaultName: string): string {
  const clean = vaultName.replace(/[/\\:*?"<>|]/g, '-').trim() || 'Obsidian';
  return `${clean} (CoWiki)`;
}
