export interface ReviewPreviewInput {
  path: string;
  old_content: string | null;
  new_content: string | null;
  is_binary?: boolean;
}

export interface ReviewPreviewPane {
  key: 'before' | 'after';
  label: string;
  content: string;
}

export function reviewPreviewPanes(file: ReviewPreviewInput): ReviewPreviewPane[] {
  if (file.is_binary || !file.path.toLowerCase().endsWith('.md')) return [];

  if (file.old_content == null && file.new_content != null) {
    return [{ key: 'after', label: 'Added', content: markdownBody(file.new_content) }];
  }
  if (file.old_content != null && file.new_content == null) {
    return [{ key: 'before', label: 'Deleted', content: markdownBody(file.old_content) }];
  }
  if (file.old_content != null && file.new_content != null) {
    return [
      { key: 'before', label: 'Before', content: markdownBody(file.old_content) },
      { key: 'after', label: 'After', content: markdownBody(file.new_content) },
    ];
  }
  return [];
}

function markdownBody(content: string): string {
  return splitSystemFrontmatter(content).body;
}
import { splitSystemFrontmatter } from './page-frontmatter.ts';
