import { LanguageDescription } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import type { Parser } from '@lezer/common';
import { highlightCode, tagHighlighter, tags as t } from '@lezer/highlight';

export interface HighlightSpan {
  text: string;
  className?: string;
}

/**
 * Same tag-to-token mapping as `editorHighlighting` in `components/editor/theme.ts`,
 * using the `--color-syntax-*` CSS classes in `index.css` so reader and editor
 * code blocks match in both light and dark themes.
 */
const readerHighlighter = tagHighlighter([
  { tag: [t.keyword, t.operator], class: 'tok-keyword' },
  { tag: [t.string, t.special(t.string)], class: 'tok-string' },
  { tag: t.comment, class: 'tok-comment' },
  { tag: [t.number, t.bool, t.atom, t.propertyName, t.attributeName], class: 'tok-literal' },
  { tag: [t.typeName, t.className, t.namespace], class: 'tok-type' },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], class: 'tok-fn' },
  { tag: t.tagName, class: 'tok-tag' },
]);

const parserCache = new Map<string, Parser | null>();
const pendingLoads = new Map<string, Promise<Parser | null>>();

function canonicalName(language: string): string {
  return language.trim().toLowerCase();
}

export function extractCodeLanguage(className: string | undefined): string | null {
  if (!className) return null;
  const match = /\blanguage-([^\s]+)/.exec(className);
  return match ? match[1] : null;
}

export function cachedParser(language: string): Parser | null | undefined {
  return parserCache.get(canonicalName(language));
}

export function loadParser(language: string): Promise<Parser | null> {
  const key = canonicalName(language);
  if (!key) return Promise.resolve(null);
  if (parserCache.has(key)) return Promise.resolve(parserCache.get(key) ?? null);
  const existing = pendingLoads.get(key);
  if (existing) return existing;

  const desc = LanguageDescription.matchLanguageName(languages, key, true);
  if (!desc) {
    parserCache.set(key, null);
    return Promise.resolve(null);
  }

  const promise = desc.load()
    .then((support) => {
      const parser = support.language.parser;
      parserCache.set(key, parser);
      return parser;
    })
    .catch(() => {
      parserCache.set(key, null);
      return null;
    })
    .finally(() => {
      pendingLoads.delete(key);
    });
  pendingLoads.set(key, promise);
  return promise;
}

export function highlightWithParser(code: string, parser: Parser): HighlightSpan[] {
  const tree = parser.parse(code);
  const spans: HighlightSpan[] = [];
  const push = (text: string, className?: string) => {
    if (!text) return;
    const prev = spans[spans.length - 1];
    if (prev && prev.className === className) {
      prev.text += text;
    } else {
      spans.push(className ? { text, className } : { text });
    }
  };
  highlightCode(
    code,
    tree,
    readerHighlighter,
    (text, classes) => push(text, classes || undefined),
    () => push('\n'),
  );
  return spans;
}

export async function highlightCodeBlock(code: string, language: string): Promise<HighlightSpan[] | null> {
  const parser = await loadParser(language);
  if (!parser) return null;
  return highlightWithParser(code, parser);
}
