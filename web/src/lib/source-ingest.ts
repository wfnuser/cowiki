import type { IngestFileOutcome, SourceItem } from '../api.ts';

export interface FileIngestResult {
  remainingFiles: string[];
  error: string;
  shouldClose: boolean;
}

export type SourceImportKind = 'text' | 'url' | 'file';

export function sourceImportProgressLabel(kind: SourceImportKind, fileCount: number): string {
  if (kind === 'file') {
    return `Reading and extracting ${fileCount} source${fileCount === 1 ? '' : 's'}…`;
  }
  return 'Saving and indexing source…';
}

export function sourceReadyLabel(agentName: string): string {
  return `Ready for ${agentName} to organize`;
}

export function sourceImportStorageLabel(desktop: boolean): string {
  return desktop
    ? 'Saved locally and added to the OKF and search indexes.'
    : 'Added to this Space and its search index.';
}

export function mergeImportedSources(
  current: SourceItem[],
  imported: SourceItem[],
): SourceItem[] {
  const sources = new Map(current.map((source) => [source.filename, source]));
  imported.forEach((source) => sources.set(source.filename, source));
  return [...sources.values()];
}

export function sourceOrganizationTask(sources: SourceItem[]): string {
  const paths = sources
    .map((source) => source.filename.startsWith('sources/')
      ? source.filename
      : `sources/${source.filename}`)
    .map((path) => `- ${path}`)
    .join('\n');
  return [
    'Organize the newly imported OKF Source files below into durable knowledge.',
    'Read each Source, update the appropriate Concepts and indexes, preserve provenance, and do not modify the Source files.',
    paths,
  ].join('\n');
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export function fileIngestResult(outcomes: IngestFileOutcome[]): FileIngestResult {
  const failed = outcomes.filter((outcome) => outcome.error);
  if (failed.length === 0) {
    return { remainingFiles: [], error: '', shouldClose: true };
  }

  return {
    remainingFiles: failed.map((outcome) => outcome.sourcePath),
    error: `${failed.length} of ${outcomes.length} file(s) failed: ${failed
      .map((outcome) => `${fileName(outcome.sourcePath)} (${outcome.error})`)
      .join(', ')}`,
    shouldClose: false,
  };
}
