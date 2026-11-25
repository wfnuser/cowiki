import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

import { LivePreviewEditor, type LivePreviewEditorHandle } from '@/components/editor/LivePreviewEditor';
import { ObsidianReaderContext, withObsidianReader } from '@/components/ObsidianReader';
import {
  joinLeadingTitle,
  renderedClick,
  sourceOffsetForClick,
  splitLeadingTitle,
  type RenderedClick,
} from '@/lib/click-to-edit';
import {
  dateKey,
  formatDate,
  fromDateKey,
  journalTimeline,
  type JournalConfig,
  type JournalEntry,
} from '@/lib/journal';
import { useTodayKey } from '@/lib/useTodayKey';
import { obsidianToReaderMarkdown, renderedLineOffset } from '@/lib/obsidian-markdown';
import { applyTaskToggle } from '@/lib/tasks';
import { useI18n } from '@/lib/I18nContext';
import { remarkSoftBreaks } from '@/lib/remark-soft-breaks';

const COMPONENTS = withObsidianReader({});
const PAGE = 7;
const DEFAULT_NOW = () => new Date();
const SCROLL_KEYS = new Set(['ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End', ' ']);

interface JournalsViewProps {
  config: JournalConfig;
  /** Every page path in the Space. */
  paths: string[];
  read: (path: string) => Promise<string>;
  /** Saves only when the file still has `expected`, so another edit is never overwritten. */
  write: (path: string, body: string, expected: string) => Promise<void>;
  /** Creates today's entry from the Space's journal template; returns its path. */
  createToday: () => Promise<string>;
  onOpenPage: (path: string) => void;
  onWikilink?: (target: string) => void;
  loadAsset?: (fromPath: string, src: string) => Promise<string>;
  getPageSlugs?: () => string[];
  now?: () => Date;
  /** A day picked in the Journals calendar: shown and scrolled to (a new nonce repeats it). */
  jumpTo?: { key: string; nonce: number } | null;
}

/**
 * Journals, as in Logseq: one stream you scroll with the wheel, today first,
 * earlier days loading below. Every day is written in place: a click opens
 * the editor where you clicked, an empty day is a bullet waiting for text.
 */
export function JournalsView({ config, paths, now = DEFAULT_NOW, jumpTo = null, ...day }: JournalsViewProps) {
  const todayKey = useTodayKey(now);
  const entries = useMemo(
    () => journalTimeline(config, paths, fromDateKey(todayKey) ?? now()),
    [config, paths, todayKey, now],
  );
  const [shown, setShown] = useState(PAGE);
  const containerRef = useRef<HTMLDivElement>(null);
  const more = useRef<HTMLDivElement>(null);
  const entriesRef = useRef(entries);
  const settledDaysRef = useRef<Set<string>>(new Set());
  const pendingJumpRef = useRef<{ key: string; pending: Set<string>; cancel: () => void } | null>(null);

  useEffect(() => {
    entriesRef.current = entries;
  }, [entries]);

  // A day picked in the calendar may be further down than what has loaded.
  const jumpIndex = jumpTo ? entries.findIndex((entry) => entry.key === jumpTo.key) : -1;
  const visible = Math.max(shown, jumpIndex + 1);

  // Raise `shown` monotonically so jumping back to a recent day never unmounts
  // already-expanded older days.
  useEffect(() => {
    if (jumpIndex + 1 > shown) setShown(jumpIndex + 1);
  }, [jumpIndex, shown]);

  const scrollDayIntoView = useCallback((key: string) => {
    const scope = containerRef.current ?? document;
    scope.querySelector(`[data-journal-day="${CSS.escape(key)}"]`)?.scrollIntoView({ block: 'start' });
  }, []);

  const handleDaySettled = useCallback((key: string) => {
    settledDaysRef.current.add(key);
    const active = pendingJumpRef.current;
    if (!active || !active.pending.has(key)) return;
    active.pending.delete(key);
    // Re-pin the jump target now that an earlier (or target) day's real height is in the DOM.
    scrollDayIntoView(active.key);
    if (active.pending.size === 0) active.cancel();
  }, [scrollDayIntoView]);

  useEffect(() => {
    if (!jumpTo) return;
    pendingJumpRef.current?.cancel();
    const list = entriesRef.current;
    const index = list.findIndex((entry) => entry.key === jumpTo.key);
    if (index < 0) return;

    scrollDayIntoView(jumpTo.key);

    const pending = new Set<string>();
    for (let i = 0; i <= index; i += 1) {
      const key = list[i]!.key;
      if (!settledDaysRef.current.has(key)) pending.add(key);
    }
    if (pending.size === 0) return;

    const scrollHost: EventTarget = containerRef.current?.closest('.scroll-pane') ?? window;
    const onUserScroll = () => cancel();
    const onKeyDown = (event: Event) => {
      if (SCROLL_KEYS.has((event as KeyboardEvent).key)) cancel();
    };
    let timeoutId: number | null = window.setTimeout(() => cancel(), 10_000);
    let active = true;

    const cancel = () => {
      if (!active) return;
      active = false;
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
        timeoutId = null;
      }
      scrollHost.removeEventListener('wheel', onUserScroll);
      scrollHost.removeEventListener('touchstart', onUserScroll);
      scrollHost.removeEventListener('pointerdown', onUserScroll);
      window.removeEventListener('keydown', onKeyDown);
      if (pendingJumpRef.current?.cancel === cancel) pendingJumpRef.current = null;
    };

    pendingJumpRef.current = { key: jumpTo.key, pending, cancel };
    scrollHost.addEventListener('wheel', onUserScroll, { passive: true });
    scrollHost.addEventListener('touchstart', onUserScroll, { passive: true });
    scrollHost.addEventListener('pointerdown', onUserScroll, { passive: true });
    window.addEventListener('keydown', onKeyDown);
    return cancel;
  }, [jumpTo, scrollDayIntoView]);

  useEffect(() => {
    const target = more.current;
    if (!target || visible >= entries.length) return;
    const observer = new IntersectionObserver((seen) => {
      if (seen.some((entry) => entry.isIntersecting)) setShown(visible + PAGE);
    }, { rootMargin: '600px 0px' });
    observer.observe(target);
    return () => observer.disconnect();
  }, [entries.length, visible]);

  return (
    <div ref={containerRef} className="journals">
      {entries.slice(0, visible).map((entry) => (
        <JournalDay
          key={entry.key}
          entry={entry}
          today={entry.key === todayKey}
          onSettled={handleDaySettled}
          {...day}
        />
      ))}
      <div ref={more} aria-hidden style={{ height: 1 }} />
    </div>
  );
}

export type JournalDayProps = Omit<JournalsViewProps, 'config' | 'paths' | 'now' | 'jumpTo'> & {
  entry: JournalEntry;
  today: boolean;
  onSettled?: (key: string) => void;
};

interface EditSession {
  head: string;
  content: string;
  cursor: number;
}

/** One day of the journal, read and written in place (Journals, and Home's today). */
export function JournalDay({
  entry, today, read, write, createToday, onOpenPage, onWikilink, loadAsset, getPageSlugs, onSettled,
}: JournalDayProps) {
  const { t } = useI18n();
  // undefined while loading; null when today's file does not exist yet.
  const [body, setBody] = useState<string | null | undefined>(entry.missing ? null : undefined);
  const [session, setSession] = useState<EditSession | null>(null);
  const [error, setError] = useState('');
  const saved = useRef('');
  const latest = useRef('');
  const timer = useRef<number | null>(null);
  const inFlight = useRef<Promise<boolean> | null>(null);
  const editing = useRef(false);
  const editor = useRef<LivePreviewEditorHandle>(null);
  useEffect(() => { editing.current = !!session; }, [session]);

  const settled = body !== undefined || error !== '';
  useEffect(() => {
    if (settled) onSettled?.(entry.key);
  }, [settled, entry.key, onSettled]);

  useEffect(() => {
    if (entry.missing) return;
    let live = true;
    read(entry.path)
      .then((text) => { if (live && !editing.current) setBody(text); })
      .catch((cause) => { if (live) setError(cause instanceof Error ? cause.message : String(cause)); });
    return () => { live = false; };
  }, [entry.missing, entry.path, read]);

  const flush = useCallback(async (): Promise<boolean> => {
    if (timer.current !== null) { window.clearTimeout(timer.current); timer.current = null; }
    while (inFlight.current) {
      const ok = await inFlight.current;
      if (!ok) return false;
      if (latest.current === saved.current) return true;
    }
    if (latest.current === saved.current) return true;
    const run = async (): Promise<boolean> => {
      try {
        while (latest.current !== saved.current) {
          const next = latest.current;
          await write(entry.path, next, saved.current);
          saved.current = next;
        }
        setError('');
        return true;
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : 'Could not save');
        return false;
      }
    };
    const work = run().finally(() => {
      if (inFlight.current === work) inFlight.current = null;
    });
    inFlight.current = work;
    return work;
  }, [entry.path, write]);

  // Leaving the journal never drops typing.
  useEffect(() => () => { void flush(); }, [flush]);

  const startEdit = async (click: RenderedClick | null) => {
    if (session) return;
    setError('');
    try {
      let full = body;
      if (full === undefined) return;
      if (full === null) {
        await createToday();
        full = await read(entry.path);
        setBody(full);
      }
      const { head, content } = splitLeadingTitle(full);
      saved.current = full;
      latest.current = full;
      const cursor = click ? sourceOffsetForClick(content, click) ?? content.length : content.length;
      setSession({ head, content, cursor });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const finishEdit = async () => {
    if (!session || !(await flush())) return;
    setBody(saved.current);
    setSession(null);
  };

  const toggleTask = async (renderedLine: number) => {
    if (typeof body !== 'string') return;
    const shownText = splitLeadingTitle(body).content.trim();
    const index = renderedLineOffset(body, shownText) + renderedLine - 1;
    const raw = body.split(/\r?\n/)[index];
    if (raw === undefined) return;
    try {
      const updated = applyTaskToggle(body, { line: index, raw }, dateKey(new Date()));
      await write(entry.path, updated, body);
      setBody(updated);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not update the task');
    }
  };

  const label = formatDate(entry.date, 'dddd, MMMM D, YYYY');
  const title = formatDate(entry.date, 'MMM Do, YYYY');
  const shownText = typeof body === 'string' ? splitLeadingTitle(body).content.trim() : '';

  return (
    <section className={today ? 'journal-day is-today' : 'journal-day'} aria-label={label} data-journal-day={entry.key}>
      <h2 className="journal-day-title">
        {body === null ? title : (
          <button type="button" onClick={() => onOpenPage(entry.path)} title="Open as a page">{title}</button>
        )}
      </h2>

      {session ? (
        <div
          className="journal-day-editor"
          onMouseDown={(event) => {
            // The day's empty space below the text writes at its end.
            const target = event.target as HTMLElement;
            if (target.closest('.cm-editor')) return;
            event.preventDefault();
            editor.current?.focusEnd();
          }}
        >
          <LivePreviewEditor
            ref={editor}
            initialDoc={session.content}
            initialCursor={session.cursor}
            onDocChanged={(doc) => {
              latest.current = joinLeadingTitle(session.head, doc);
              if (timer.current !== null) window.clearTimeout(timer.current);
              timer.current = window.setTimeout(() => { void flush(); }, 700);
            }}
            onRequestSave={() => { void flush(); }}
            onBlur={() => { void finishEdit(); }}
            onEscape={() => { void finishEdit(); }}
            onWikilink={async (target) => { if (await flush()) onWikilink?.(target); }}
            loadImage={loadAsset ? (src) => loadAsset(entry.path, src) : undefined}
            getPageSlugs={getPageSlugs}
            className="doc-editor"
            style={{ height: 'auto' }}
          />
        </div>
      ) : (
        <div
          className="journal-day-body prose"
          onClick={(event) => {
            const click = renderedClick(event.nativeEvent, event.currentTarget);
            if (click !== undefined) void startEdit(click);
          }}
        >
          {body === undefined ? (
            <p className="journal-day-placeholder" aria-busy>…</p>
          ) : shownText ? (
            <ObsidianReaderContext.Provider value={{
              onWikilink,
              loadAsset: loadAsset ? (src) => loadAsset(entry.path, src) : undefined,
              onToggleTask: (line) => { void toggleTask(line); },
            }}>
              <div className="doc-body">
                <ReactMarkdown remarkPlugins={[remarkGfm, remarkSoftBreaks]} components={COMPONENTS}>
                  {obsidianToReaderMarkdown(shownText)}
                </ReactMarkdown>
              </div>
            </ObsidianReaderContext.Provider>
          ) : (
            // Like Logseq: an empty day is one bullet, ready for text.
            <div className="journal-day-empty" role="button" tabIndex={0} aria-label={t('Write in this day')}
              onClick={(event) => { event.stopPropagation(); void startEdit(null); }}
              onKeyDown={(event) => { if (event.key === 'Enter') void startEdit(null); }} />
          )}
        </div>
      )}
      {error && <p role="alert" className="journal-day-error">{error}</p>}
    </section>
  );
}
