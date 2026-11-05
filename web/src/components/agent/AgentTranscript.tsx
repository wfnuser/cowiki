import {
  Brain,
  ChevronRight,
  FileDiff,
  LoaderCircle,
  Plug,
  Search,
  SquareTerminal,
  Wrench,
  XCircle,
  CheckCircle2,
  Ban,
  CircleStop,
} from 'lucide-react';
import { useEffect, useRef, useState, type ComponentType } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';

import { openExternalUrl } from '@/external-links';
import { fonts } from '@/lib/design';
import type { AgentItem, AgentSnapshot, AgentToolItem, ToolKind } from '@/lib/agent-session';
import { cn } from '@/lib/utils';

const toolIcons: Record<ToolKind, ComponentType<{ className?: string }>> = {
  command: SquareTerminal,
  file: FileDiff,
  search: Search,
  mcp: Plug,
  think: Brain,
  other: Wrench,
};

// Agent text is untrusted: raw HTML is dropped and links open in the
// system browser (http/https only), never inside CoWiki.
const markdownComponents: Components = {
  a: ({ href, children }) => (
    <a
      href={href}
      className="text-accent underline underline-offset-2"
      onClick={(event) => {
        event.preventDefault();
        if (href && /^https?:\/\//i.test(href)) void openExternalUrl(href).catch(() => undefined);
      }}
    >
      {children}
    </a>
  ),
  img: ({ alt }) => <span className="text-text-tertiary">[{alt || 'image'}]</span>,
  pre: ({ children }) => (
    <pre className="my-2 overflow-x-auto rounded-md bg-bg-secondary p-2 text-[12px] leading-relaxed" style={{ fontFamily: fonts.mono }}>
      {children}
    </pre>
  ),
  code: ({ children, className }) => (
    <code className={cn('rounded bg-bg-secondary px-1 py-0.5 text-[12px]', className)} style={{ fontFamily: fonts.mono }}>
      {children}
    </code>
  ),
};

export function AgentMarkdown({ text }: { text: string }) {
  return (
    <div className="agent-markdown min-w-0 break-words text-[13px] leading-relaxed text-text [&_ol]:ml-5 [&_ol]:list-decimal [&_p]:my-1.5 [&_ul]:ml-5 [&_ul]:list-disc [&_h1]:mt-3 [&_h1]:text-base [&_h1]:font-semibold [&_h2]:mt-3 [&_h2]:text-sm [&_h2]:font-semibold [&_h3]:mt-2 [&_h3]:font-semibold [&_table]:my-2 [&_td]:border [&_td]:border-border [&_td]:px-1.5 [&_th]:border [&_th]:border-border [&_th]:px-1.5 [&_blockquote]:border-l-2 [&_blockquote]:border-border [&_blockquote]:pl-2 [&_blockquote]:text-text-secondary">
      <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml components={markdownComponents}>
        {text}
      </ReactMarkdown>
    </div>
  );
}

function ToolStatusIcon({ item }: { item: AgentToolItem }) {
  switch (item.status) {
    case 'running':
      return <LoaderCircle className="size-3.5 shrink-0 animate-spin text-accent" aria-hidden />;
    case 'completed':
      return <CheckCircle2 className="size-3.5 shrink-0 text-green" aria-hidden />;
    case 'failed':
      return <XCircle className="size-3.5 shrink-0 text-red" aria-hidden />;
    case 'declined':
      return <Ban className="size-3.5 shrink-0 text-amber" aria-hidden />;
    case 'stopped':
      return <CircleStop className="size-3.5 shrink-0 text-text-tertiary" aria-hidden />;
  }
}

const statusText: Record<AgentToolItem['status'], string> = {
  running: 'running',
  completed: 'done',
  failed: 'failed',
  declined: 'declined',
  stopped: 'stopped',
};

function ToolStep({ item }: { item: AgentToolItem }) {
  const [open, setOpen] = useState(false);
  const Icon = toolIcons[item.toolKind] ?? Wrench;
  const hasOutput = item.output.trim().length > 0;
  return (
    <div className="rounded-md border border-border/70 bg-panel/40">
      <button
        type="button"
        className="flex w-full min-w-0 items-center gap-2 px-2 py-1.5 text-left text-xs text-text-secondary hover:bg-bg-hover disabled:cursor-default"
        aria-expanded={hasOutput ? open : undefined}
        disabled={!hasOutput}
        onClick={() => setOpen((value) => !value)}
      >
        <ChevronRight className={cn('size-3 shrink-0 text-text-faint transition-transform', open && 'rotate-90', !hasOutput && 'opacity-0')} aria-hidden />
        <Icon className="size-3.5 shrink-0 text-text-tertiary" />
        <span className="min-w-0 flex-1 truncate" style={item.toolKind === 'command' ? { fontFamily: fonts.mono } : undefined}>
          {item.name}
        </span>
        <ToolStatusIcon item={item} />
        <span className="sr-only">{statusText[item.status]}</span>
      </button>
      {open && hasOutput && (
        <div className="border-t border-border/70">
          {item.truncated && (
            <p className="px-2 pt-1.5 text-[10px] text-text-tertiary">Earlier output was truncated.</p>
          )}
          <pre
            className="max-h-64 overflow-auto whitespace-pre-wrap break-words px-2 py-1.5 text-[11px] leading-relaxed text-text-secondary"
            style={{ fontFamily: fonts.mono }}
          >
            {item.output}
          </pre>
        </div>
      )}
    </div>
  );
}

function Item({ item, last, running }: { item: AgentItem; last: boolean; running: boolean }) {
  if (item.type === 'tool') return <ToolStep item={item} />;
  if (item.role === 'user') {
    return (
      <div className="ml-6 self-end rounded-lg bg-accent-soft px-3 py-2 text-[13px] leading-relaxed text-text whitespace-pre-wrap break-words">
        {item.text}
      </div>
    );
  }
  if (!item.text && last && running) {
    return <p className="text-xs text-text-tertiary">Thinking…</p>;
  }
  return <AgentMarkdown text={item.text} />;
}

export function AgentTranscript({ snapshot }: { snapshot: AgentSnapshot }) {
  const endRef = useRef<HTMLDivElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const stickToBottom = useRef(true);
  const running = snapshot.status === 'running' || snapshot.status === 'starting';

  useEffect(() => {
    if (stickToBottom.current) endRef.current?.scrollIntoView({ block: 'end' });
  }, [snapshot.seq]);

  return (
    <div
      ref={containerRef}
      className="min-h-0 flex-1 overflow-y-auto px-3 py-3"
      onScroll={(event) => {
        const element = event.currentTarget;
        stickToBottom.current = element.scrollHeight - element.scrollTop - element.clientHeight < 48;
      }}
      aria-live="polite"
      aria-busy={running}
    >
      <div className="flex flex-col gap-2">
        {snapshot.items.length === 0 && (
          <p className="py-6 text-center text-xs text-text-tertiary">Send a message to start.</p>
        )}
        {snapshot.items.map((item, index) => (
          <Item key={item.id} item={item} last={index === snapshot.items.length - 1} running={running} />
        ))}
        {snapshot.status === 'starting' && (
          <p className="flex items-center gap-1.5 text-xs text-text-tertiary">
            <LoaderCircle className="size-3 animate-spin" aria-hidden /> Starting the Agent…
          </p>
        )}
        <div ref={endRef} />
      </div>
    </div>
  );
}

