import { useMemo, useState, type ComponentPropsWithoutRef } from 'react';
import { Code2, Eye } from 'lucide-react';
import type { Components } from 'react-markdown';
import { C, fonts } from '../lib/design';
import { isHtmlCodeLanguage, sandboxedHtmlDocument } from '../lib/html-view';

type CodeProps = ComponentPropsWithoutRef<'code'> & {
  node?: unknown;
};

type PreProps = ComponentPropsWithoutRef<'pre'> & {
  node?: unknown;
};

export function HtmlView({ source }: { source: string }) {
  const [mode, setMode] = useState<'preview' | 'source'>('preview');
  const document = useMemo(() => sandboxedHtmlDocument(source), [source]);
  return (
    <section style={{
      margin: '20px 0', overflow: 'hidden', border: `1px solid ${C.line}`,
      borderRadius: 12, background: C.panel,
    }}>
      <div style={{
        height: 42, display: 'flex', alignItems: 'center', gap: 6,
        padding: '0 10px 0 14px', borderBottom: `1px solid ${C.lineSoft}`,
      }}>
        <span style={{ marginRight: 'auto', fontSize: 12, fontWeight: 650, color: C.muted }}>
          HTML View
        </span>
        <ModeButton active={mode === 'preview'} onClick={() => setMode('preview')}>
          <Eye size={13} /> Preview
        </ModeButton>
        <ModeButton active={mode === 'source'} onClick={() => setMode('source')}>
          <Code2 size={13} /> Source
        </ModeButton>
      </div>
      {mode === 'preview' ? (
        <iframe
          title="Sandboxed HTML preview"
          sandbox="allow-scripts"
          referrerPolicy="no-referrer"
          srcDoc={document}
          style={{ width: '100%', height: 440, display: 'block', border: 0, background: C.panel }}
        />
      ) : (
        <pre style={{
          maxHeight: 440, margin: 0, padding: 18, overflow: 'auto',
          background: C.bg, color: C.ink2, fontFamily: fonts.mono, fontSize: 12.5, lineHeight: 1.55,
        }}><code>{source}</code></pre>
      )}
    </section>
  );
}

function ModeButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        display: 'inline-flex', alignItems: 'center', gap: 5, padding: '5px 8px',
        border: 'none', borderRadius: 7, cursor: 'pointer', fontSize: 11.5, fontWeight: 600,
        background: active ? C.accentSoft : 'transparent', color: active ? C.accent : C.faint,
      }}
    >
      {children}
    </button>
  );
}

function HtmlAwareCode({ className, children, node, ...props }: CodeProps) {
  void node;
  if (isHtmlCodeLanguage(className)) {
    return <HtmlView source={String(children).replace(/\n$/, '')} />;
  }
  return <code className={className} {...props}>{children}</code>;
}

function HtmlAwarePre({ children, node, ...props }: PreProps) {
  const hast = node as { children?: Array<{ properties?: { className?: string[] } }> } | undefined;
  const codeClass = hast?.children?.[0]?.properties?.className?.join(' ');
  if (isHtmlCodeLanguage(codeClass)) return <>{children}</>;
  return <pre {...props}>{children}</pre>;
}

export const htmlMarkdownComponents: Components = {
  code: HtmlAwareCode,
  pre: HtmlAwarePre,
};
