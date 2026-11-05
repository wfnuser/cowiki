import { ArrowUp, LoaderCircle, Play, Square } from 'lucide-react';
import { useRef, useState, type KeyboardEvent } from 'react';

import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export function AgentComposer({
  placeholder,
  disabledReason,
  busy,
  stopping,
  resumable,
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
  onSend: (text: string) => Promise<boolean>;
  onStop?: () => void;
  onResume?: () => void;
}) {
  const [text, setText] = useState('');
  const [sending, setSending] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const blocked = Boolean(disabledReason);
  const canSubmit = !busy && !sending && !blocked && text.trim().length > 0;

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
    if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      void submit();
    }
  };

  return (
    <div className="shrink-0 border-t border-border bg-bg px-3 pb-3 pt-2">
      {resumable && !busy && (
        <Button
          size="xs"
          variant="outline"
          className="mb-2 w-full"
          onClick={onResume}
        >
          <Play /> Continue this conversation
        </Button>
      )}
      <div className={cn('rounded-lg border border-border bg-panel focus-within:border-accent/60', blocked && 'opacity-70')}>
        <textarea
          ref={textareaRef}
          aria-label="Message agent"
          className="block max-h-48 min-h-16 w-full resize-none bg-transparent px-3 py-2 text-[13px] leading-relaxed text-text outline-none placeholder:text-text-faint disabled:cursor-not-allowed"
          placeholder={disabledReason ?? placeholder}
          value={text}
          disabled={blocked}
          rows={3}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={onKeyDown}
        />
        <div className="flex items-center justify-end gap-1.5 px-2 pb-2">
          <span className="mr-auto text-[10px] text-text-faint">
            {busy ? 'Working… you can collapse the panel; the task keeps running.' : 'Enter to send · Shift+Enter for a new line'}
          </span>
          {busy ? (
            <Button
              size="xs"
              variant="outline"
              aria-label="Stop task"
              disabled={stopping || !onStop}
              onClick={onStop}
            >
              {stopping ? <LoaderCircle className="animate-spin" /> : <Square />}
              {stopping ? 'Stopping…' : 'Stop'}
            </Button>
          ) : (
            <Button
              size="xs"
              className="bg-accent text-on-accent hover:bg-accent-hover"
              aria-label="Send message"
              disabled={!canSubmit}
              onClick={() => void submit()}
            >
              {sending ? <LoaderCircle className="animate-spin" /> : <ArrowUp />}
              Send
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
