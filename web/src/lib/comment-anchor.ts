import { diffLines } from 'diff';

/**
 * Re-anchoring for document comments.
 *
 * A comment stores a 1-based source line range against the markdown snapshot the
 * author saw (`anchored`). To show it on whatever version a viewer is currently
 * rendering (`current` — their own branch), we line-diff snapshot→current and map
 * the range forward. If any line in the range was edited or removed, the comment
 * is reported `outdated` (GitHub-style) rather than dragged onto changed text.
 *
 * Line numbers here must match `node.position.start.line` from the same markdown
 * string that react-markdown renders (i.e. the frontmatter-stripped body).
 */
export type MappedAnchor =
  | { kind: 'anchored'; startLine: number; endLine: number }
  | { kind: 'outdated' };

/** Build a map old-line → new-line for every line that survived unchanged. */
function survivingLines(anchored: string, current: string): Map<number, number> {
  const map = new Map<number, number>();
  let oldLine = 1;
  let newLine = 1;
  for (const part of diffLines(anchored, current)) {
    const count = part.count ?? part.value.split('\n').length - 1;
    if (part.added) {
      newLine += count;
    } else if (part.removed) {
      oldLine += count;
    } else {
      for (let k = 0; k < count; k++) map.set(oldLine + k, newLine + k);
      oldLine += count;
      newLine += count;
    }
  }
  return map;
}

/** Map a range using a prebuilt surviving-line map (see `buildSnapshotMaps`). */
export function mapWithSurviving(
  surviving: Map<number, number>,
  startLine: number,
  endLine: number,
): MappedAnchor {
  for (let l = startLine; l <= endLine; l++) {
    if (!surviving.has(l)) return { kind: 'outdated' };
  }
  return {
    kind: 'anchored',
    startLine: surviving.get(startLine)!,
    endLine: surviving.get(endLine)!,
  };
}

export function mapAnchor(
  anchored: string,
  current: string,
  startLine: number,
  endLine: number,
): MappedAnchor {
  if (anchored === current) {
    return { kind: 'anchored', startLine, endLine };
  }
  return mapWithSurviving(survivingLines(anchored, current), startLine, endLine);
}

/**
 * Cost is one diff per *distinct snapshot*, not per comment: comments are
 * grouped by `content_hash` and share a single line-diff against the current
 * source. In practice a page has only one or two snapshot versions, so this is
 * a couple of KB-scale diffs.
 *
 * TODO(perf, #97): optional per-line-hash fast path. Store the anchored
 * lines' hashes; at display time hash each current line into a set and, for
 * comments whose line hashes are all still present, skip the diff entirely.
 * Caveat: hashes only answer "does this line still exist" — duplicate lines
 * (blank lines, repeated `---`, identical list items) collide, so the diff is
 * still required to resolve the *new* line number via surrounding context.
 */
export function buildSnapshotMaps(
  snapshots: { content_hash: string; source: string }[],
  current: string,
): Map<string, Map<number, number>> {
  const maps = new Map<string, Map<number, number>>();
  for (const s of snapshots) {
    if (!maps.has(s.content_hash)) {
      maps.set(s.content_hash, survivingLines(s.source, current));
    }
  }
  return maps;
}
