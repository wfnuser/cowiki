import { ArrowUp, LoaderCircle, Play, Square } from 'lucide-react';
import { isSubmitEnter } from '@/lib/ime';
import { useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';

import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

const MAX_HEIGHT = 192;

export function AgentComposer({
  placeholder,
  disabledReason,
  busy,
  stopping,
  resumable,
  meta,
  onSend,
  onStop,
  onResume,
}: {
  placeholder: string;
  /** Set when sending is impossible (closed Change, Agent not ready). */
  disabledReason?: string | null;
  busy: boolean;
  stopping: boolean;
  resumable?: boolean;
  /** A quiet line under the input: which Agent, where it works. */
  meta?: ReactNode;
  onSend: (text: string) => Promise<boolean>;
  onStop?: () => void;
  onResume?: () => void;
}) {
  const [text, setText] = useState('');
  const [sending, setSending] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const blocked = Boolean(disabledReason);
  const canSubmit = !busy && !sending && !blocked && text.trim().length > 0;

  // Grow with the text, up to a limit; then scroll.
  useLayoutEffect(() => {
    const element = textareaRef.current;
    if (!element) return;
    element.style.height = 'auto';
    element.style.height = `${Math.min(element.scrollHeight, MAX_HEIGHT)}px`;
  }, [text]);

  const submit = async () => {
    if (!canSubmit) return;
    setSending(true);
    try {
      if (await onSend(text)) setText('');
    } finally {
      setSending(false);
      textareaRef.current?.focus();
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (isSubmitEnter(event)) {
      event.preventDefault();
      void submit();
    }
  };

  return (
    <div className="shrink-0 px-3 pb-3 pt-1">
      {resumable && !busy && (
        <Button size="xs" variant="outline" className="mb-2 w-full" onClick={onResume}>
          <Play /> Continue this conversation
        </Button>
      )}
      <div
        className={cn(
          'rounded-xl border border-border bg-panel shadow-[var(--shadow-rest)] transition-[border-color,box-shadow] focus-within:border-border-hover focus-within:shadow-[var(--shadow-card)]',
          blocked && 'bg-bg-secondary/60',
        )}
      >
        <textarea
          ref={textareaRef}
          aria-label="Message agent"
          className="block w-full resize-none bg-transparent px-3 pb-1 pt-2.5 text-small leading-[1.55] text-text outline-none placeholder:text-text-faint disabled:cursor-not-allowed"
          style={{ maxHeight: MAX_HEIGHT, minHeight: 44 }}
          placeholder={disabledReason ?? placeholder}
          value={text}
          disabled={blocked}
          rows={1}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={onKeyDown}
        />
        <div className="flex items-center gap-2 py-1.5 pl-3 pr-1.5">
          <div className="min-w-0 flex-1 truncate text-caption text-text-tertiary">{meta}</div>
          {busy ? (
            <button
              type="button"
              className="flex size-7 shrink-0 items-center justify-center rounded-full bg-text text-bg transition-opacity hover:opacity-85 disabled:opacity-50"
              aria-label="Stop the Agent"
              title={stopping ? 'Stopping…' : 'Stop'}
              disabled={stopping || !onStop}
              onClick={onStop}
            >
              {stopping ? <LoaderCircle className="size-3.5 animate-spin" /> : <Square className="size-2.5 fill-current" />}
            </button>
          ) : (
            <button
              type="button"
              className="flex size-7 shrink-0 items-center justify-center rounded-full bg-accent text-on-accent transition-colors hover:bg-accent-hover disabled:bg-bg-active disabled:text-text-faint"
              aria-label="Send message"
              title="Send (Enter)"
              disabled={!canSubmit}
              onClick={() => void submit()}
            >
              {sending ? <LoaderCircle className="size-3.5 animate-spin" /> : <ArrowUp className="size-4" strokeWidth={2.25} />}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
