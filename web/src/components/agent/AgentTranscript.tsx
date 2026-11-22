import {
  Ban,
  Brain,
  ChevronRight,
  CircleStop,
  FileDiff,
  LoaderCircle,
  Plug,
  Search,
  SquareTerminal,
  Wrench,
  XCircle,
} from 'lucide-react';
import { useEffect, useRef, useState, type ComponentType } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';

import { openExternalUrl } from '@/external-links';
import { fonts } from '@/lib/design';
import {
  transcriptBlocks,
  workSummary,
  type AgentMessageItem,
  type AgentSnapshot,
  type AgentToolItem,
  type ToolKind,
} from '@/lib/agent-session';
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
    <pre
      className="my-2 overflow-x-auto rounded-lg bg-bg-secondary px-3 py-2 text-[12px] leading-relaxed [&_code]:bg-transparent [&_code]:p-0"
      style={{ fontFamily: fonts.mono }}
    >
      {children}
    </pre>
  ),
  code: ({ children, className }) => (
    <code className={cn('rounded-[5px] bg-bg-secondary px-1 py-px text-[0.9em] text-text-secondary', className)} style={{ fontFamily: fonts.mono }}>
      {children}
    </code>
  ),
};

export function AgentMarkdown({ text }: { text: string }) {
  return (
    <div className="agent-markdown min-w-0 break-words text-[13px] leading-[1.6] text-text [&_li]:my-0.5 [&_ol]:my-1.5 [&_ol]:ml-5 [&_ol]:list-decimal [&_p]:my-1.5 [&_p:first-child]:mt-0 [&_ul]:my-1.5 [&_ul]:ml-5 [&_ul]:list-disc [&_li::marker]:text-text-faint [&_strong]:font-semibold [&_h1]:mb-1 [&_h1]:mt-3 [&_h1]:text-[15px] [&_h1]:font-semibold [&_h2]:mb-1 [&_h2]:mt-3 [&_h2]:text-[14px] [&_h2]:font-semibold [&_h3]:mt-2 [&_h3]:font-semibold [&_table]:my-2 [&_table]:text-[12px] [&_td]:border [&_td]:border-border [&_td]:px-1.5 [&_th]:border [&_th]:border-border [&_th]:px-1.5 [&_blockquote]:border-l-2 [&_blockquote]:border-border [&_blockquote]:pl-2 [&_blockquote]:text-text-secondary">
      <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml components={markdownComponents}>
        {text}
      </ReactMarkdown>
    </div>
  );
}

/** Only states worth noticing get a mark; a finished step stays quiet. */
function ToolStatusIcon({ status }: { status: AgentToolItem['status'] }) {
  switch (status) {
    case 'running':
      return <LoaderCircle className="size-3.5 shrink-0 animate-spin text-accent" aria-hidden />;
    case 'failed':
      return <XCircle className="size-3.5 shrink-0 text-red" aria-hidden />;
    case 'declined':
      return <Ban className="size-3.5 shrink-0 text-amber" aria-hidden />;
    case 'stopped':
      return <CircleStop className="size-3.5 shrink-0 text-text-faint" aria-hidden />;
    case 'completed':
      return null;
  }
}

const statusText: Record<AgentToolItem['status'], string> = {
  running: 'running',
  completed: 'done',
  failed: 'failed',
  declined: 'declined',
  stopped: 'stopped',
};

const rowClass = 'flex w-full min-w-0 items-center gap-2 rounded-md px-1.5 py-1 text-left text-[12px] text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text-secondary disabled:cursor-default disabled:hover:bg-transparent';

function ToolStep({ item }: { item: AgentToolItem }) {
  const [open, setOpen] = useState(false);
  const Icon = toolIcons[item.toolKind] ?? Wrench;
  const hasOutput = item.output.trim().length > 0;
  const mono = item.toolKind === 'command';
  return (
    <div className="min-w-0">
      <button
        type="button"
        className={rowClass}
        aria-expanded={hasOutput ? open : undefined}
        disabled={!hasOutput}
        onClick={() => setOpen((value) => !value)}
      >
        <Icon className="size-3.5 shrink-0 text-text-faint" />
        <span className={cn('min-w-0 flex-1 truncate', mono && 'text-[11.5px]')} style={mono ? { fontFamily: fonts.mono } : undefined}>
          {item.name}
        </span>
        <ToolStatusIcon status={item.status} />
        <span className="sr-only">{statusText[item.status]}</span>
        {hasOutput && <ChevronRight className={cn('size-3 shrink-0 text-text-faint transition-transform', open && 'rotate-90')} aria-hidden />}
      </button>
      {open && hasOutput && (
        <div className="mb-1 ml-[26px] mt-0.5 overflow-hidden rounded-md bg-bg-secondary">
          {item.truncated && (
            <p className="px-2.5 pt-1.5 text-[10.5px] text-text-tertiary">Earlier output was truncated.</p>
          )}
          <pre
            className={cn(
              'max-h-64 overflow-auto whitespace-pre-wrap break-words px-2.5 py-2 text-[11px] leading-relaxed text-text-secondary',
              item.toolKind === 'think' && 'whitespace-pre-line italic',
            )}
            style={item.toolKind === 'think' ? undefined : { fontFamily: fonts.mono }}
          >
            {item.output}
          </pre>
        </div>
      )}
    </div>
  );
}

/** A run of tool steps: one quiet line that opens to every step. A single
 * step is shown as its own row. */
function WorkBlock({ steps }: { steps: AgentToolItem[] }) {
  const [open, setOpen] = useState(false);
  if (steps.length === 1) return <ToolStep item={steps[0]!} />;
  const running = steps.findLast((step) => step.status === 'running');
  const summary = workSummary(steps);
  const LeadIcon = running ? null : toolIcons[steps.find((step) => step.toolKind !== 'think')?.toolKind ?? 'think'];
  return (
    <div className="min-w-0">
      <button type="button" className={rowClass} aria-expanded={open} onClick={() => setOpen((value) => !value)}>
        {running
          ? <LoaderCircle className="size-3.5 shrink-0 animate-spin text-accent" aria-hidden />
          : LeadIcon && <LeadIcon className="size-3.5 shrink-0 text-text-faint" />}
        <span className="min-w-0 flex-1 truncate">
          {running ? (
            <span style={running.toolKind === 'command' ? { fontFamily: fonts.mono, fontSize: 11.5 } : undefined}>{running.name}</span>
          ) : summary.text}
          {summary.failed > 0 && <span className="text-red"> · {summary.failed} failed</span>}
          {summary.declined > 0 && <span className="text-amber"> · {summary.declined} declined</span>}
        </span>
        <ChevronRight className={cn('size-3 shrink-0 text-text-faint transition-transform', open && 'rotate-90')} aria-hidden />
      </button>
      {open && (
        <div className="ml-[13px] mt-0.5 flex flex-col border-l border-border-soft pl-1.5">
          {steps.map((step) => <ToolStep key={step.id} item={step} />)}
        </div>
      )}
    </div>
  );
}

function Message({ item, pending }: { item: AgentMessageItem; pending: boolean }) {
  if (item.role === 'user') {
    return (
      <div className="ml-8 max-w-[88%] self-end whitespace-pre-wrap break-words rounded-2xl rounded-br-md bg-accent-soft px-3 py-1.5 text-[13px] leading-[1.55] text-text">
        {item.text}
      </div>
    );
  }
  if (!item.text && pending) return <Working label="Thinking…" />;
  return <AgentMarkdown text={item.text} />;
}

function Working({ label }: { label: string }) {
  return (
    <p className="flex items-center gap-1.5 px-1.5 text-[12px] text-text-tertiary">
      <LoaderCircle className="size-3 animate-spin text-accent" aria-hidden /> {label}
    </p>
  );
}

export function AgentTranscript({ snapshot }: { snapshot: AgentSnapshot }) {
  const endRef = useRef<HTMLDivElement | null>(null);
  const stickToBottom = useRef(true);
  const running = snapshot.status === 'running' || snapshot.status === 'starting';
  const blocks = transcriptBlocks(snapshot.items);
  const last = blocks.at(-1);
  // Something visibly moves while the Agent works between visible steps.
  const quietWork = snapshot.status === 'running'
    && !(last?.type === 'work' && last.steps.some((step) => step.status === 'running'))
    && !(last?.type === 'message' && last.item.role === 'assistant');

  useEffect(() => {
    if (stickToBottom.current) endRef.current?.scrollIntoView({ block: 'end' });
  }, [snapshot.seq]);

  return (
    <div
      className="min-h-0 flex-1 overflow-y-auto px-3 py-4"
      onScroll={(event) => {
        const element = event.currentTarget;
        stickToBottom.current = element.scrollHeight - element.scrollTop - element.clientHeight < 48;
      }}
      aria-live="polite"
      aria-busy={running}
    >
      <div className="flex flex-col gap-3">
        {blocks.length === 0 && snapshot.status !== 'starting' && (
          <p className="py-6 text-center text-[12px] text-text-tertiary">Send a message to start.</p>
        )}
        {blocks.map((block, index) => block.type === 'work'
          ? <WorkBlock key={block.id} steps={block.steps} />
          : <Message key={block.item.id} item={block.item} pending={running && index === blocks.length - 1} />)}
        {snapshot.status === 'starting' && <Working label={`Starting the Agent…`} />}
        {quietWork && <Working label="Working…" />}
        <div ref={endRef} />
      </div>
    </div>
  );
}
