import { parsePatch } from 'diff';

import type { DiffHunk, DiffLine, FileDiff } from '../api';
import type { CloudPullRequestDiff } from './client';

export function cloudDiffToFileDiffs(diff: CloudPullRequestDiff): FileDiff[] {
  const parsedByPath = new Map(
    parsePatch(diff.patch).map((file) => [cleanPath(file.newFileName ?? file.oldFileName), file]),
  );

  return diff.files.map((file) => {
    const parsed = parsedByPath.get(file.path);
    const [oldContent, newContent] = contentPresence(file.status);
    return {
      path: file.path,
      old_content: file.oldContent === undefined ? oldContent : file.oldContent,
      new_content: file.newContent === undefined ? newContent : file.newContent,
      hunks: parsed?.hunks.map(toDiffHunk) ?? [],
      additions: file.additions,
      deletions: file.deletions,
    };
  });
}

function toDiffHunk(hunk: {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: string[];
}): DiffHunk {
  let oldLine = hunk.oldStart;
  let newLine = hunk.newStart;
  const lines: DiffLine[] = [];

  for (const rawLine of hunk.lines) {
    if (rawLine.startsWith('\\')) continue;
    const prefix = rawLine[0] ?? ' ';
    const text = rawLine.slice(1);
    if (prefix === '+') {
      lines.push({ kind: 'add', old_line: null, new_line: newLine, text });
      newLine += 1;
    } else if (prefix === '-') {
      lines.push({ kind: 'del', old_line: oldLine, new_line: null, text });
      oldLine += 1;
    } else {
      lines.push({ kind: 'ctx', old_line: oldLine, new_line: newLine, text });
      oldLine += 1;
      newLine += 1;
    }
  }

  return {
    header: `@@ -${range(hunk.oldStart, hunk.oldLines)} +${range(hunk.newStart, hunk.newLines)} @@`,
    lines,
  };
}

function contentPresence(status: string): [string | null, string | null] {
  if (status === 'added') return [null, ''];
  if (status === 'deleted') return ['', null];
  return ['', ''];
}

function cleanPath(path: string | undefined): string {
  if (!path || path === '/dev/null') return '';
  return path.replace(/^[ab]\//, '');
}

function range(start: number, lines: number): string {
  return lines === 1 ? String(start) : `${start},${lines}`;
}
