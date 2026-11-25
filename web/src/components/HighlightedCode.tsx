import { useEffect, useState, type ComponentPropsWithoutRef } from 'react';
import {
  cachedParser,
  extractCodeLanguage,
  highlightWithParser,
  loadParser,
  type HighlightSpan,
} from '@/lib/code-highlight';

type CodeProps = ComponentPropsWithoutRef<'code'> & { node?: unknown };

/**
 * Renders fenced code blocks with CodeMirror's Lezer highlighters and
 * `--color-syntax-*` tokens, leaving inline `<code>` untouched.
 */
export function HighlightedCode({ className, children, node, ...props }: CodeProps) {
  void node;
  const language = extractCodeLanguage(className);
  const source = typeof children === 'string'
    ? children
    : Array.isArray(children) && children.every((child) => typeof child === 'string')
      ? children.join('')
      : null;

  const [loaded, setLoaded] = useState<{ language: string; source: string; spans: HighlightSpan[] } | null>(null);

  const syncParser = language ? cachedParser(language) : undefined;
  const syncSpans = language && source !== null && syncParser ? highlightWithParser(source, syncParser) : null;

  useEffect(() => {
    if (!language || source === null || syncParser !== undefined) return;
    let cancelled = false;
    void loadParser(language).then((parser) => {
      if (cancelled || !parser) return;
      setLoaded({ language, source, spans: highlightWithParser(source, parser) });
    });
    return () => { cancelled = true; };
  }, [language, source, syncParser]);

  if (!language || source === null) {
    return <code className={className} {...props}>{children}</code>;
  }

  const spans = syncSpans ?? (loaded && loaded.language === language && loaded.source === source ? loaded.spans : null);
  if (!spans) {
    return <code className={className} {...props}>{children}</code>;
  }

  return (
    <code className={className} {...props}>
      {spans.map((span, index) => (
        span.className
          ? <span key={index} className={span.className}>{span.text}</span>
          : span.text
      ))}
    </code>
  );
}
