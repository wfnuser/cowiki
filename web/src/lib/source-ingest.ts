import type { IngestFileOutcome } from '../api.ts';

export interface FileIngestResult {
  remainingFiles: string[];
  error: string;
  shouldClose: boolean;
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
