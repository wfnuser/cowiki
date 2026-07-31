import type { BrokenLink } from '@/api';

export interface BrokenLinkGroup {
  sourcePath: string;
  sourceTitle: string;
  targets: string[];
}

export type LinkDiagnosticsMode = 'loading' | 'error' | 'clean' | 'broken';

export function linkDiagnosticsMode(state: {
  loading: boolean;
  error: string;
  count: number;
}): LinkDiagnosticsMode {
  if (state.loading) return 'loading';
  if (state.error) return 'error';
  return state.count === 0 ? 'clean' : 'broken';
}

export function brokenLinkSummary(count: number): string {
  if (count === 0) return 'No broken links';
  return `${count} broken ${count === 1 ? 'link' : 'links'}`;
}

export function groupBrokenLinks(links: BrokenLink[]): BrokenLinkGroup[] {
  const groups = new Map<string, BrokenLinkGroup>();
  for (const link of links) {
    const existing = groups.get(link.source_path);
    if (existing) {
      existing.targets.push(link.target);
    } else {
      groups.set(link.source_path, {
        sourcePath: link.source_path,
        sourceTitle: link.source_title,
        targets: [link.target],
      });
    }
  }
  return [...groups.values()];
}
