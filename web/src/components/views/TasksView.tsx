import { useEffect, useMemo, useState } from 'react';
import { ArrowUpRight, FileText, LoaderCircle, Play, Plus, RefreshCw } from 'lucide-react';

import { ViewHeader, ViewIconButton, ViewTabs } from '@/components/ui/view-header';
import { DatePicker, dayLabel } from '@/components/ui/date-picker';
import { WeekNav } from '@/components/WeekNav';
import { listTaskSources } from '@/local-api';
import { addDays, dateKey, journalDateFromPath, startOfIsoWeek, type JournalConfig } from '@/lib/journal';
import { cn } from '@/lib/utils';
import {
  extractTasks,
  groupTasks,
  newTaskLine,
  taskDay,
  taskKey,
  weekBoard,
  type BoardStatus,
  type TaskItem,
} from '@/lib/tasks';

type Filter = 'today' | 'week' | 'upcoming' | 'someday' | 'done';

const COLUMNS: Array<{ status: BoardStatus; title: string }> = [
  { status: 'todo', title: 'To do' },
  { status: 'doing', title: 'Doing' },
  { status: 'done', title: 'Done' },
];

interface TasksViewProps {
  workspaceSlug: string;
  journal: JournalConfig;
  onOpenPage: (path: string) => void;
  /** Toggle a task in its page; resolves when the file is written. */
  onToggleTask: (task: TaskItem) => Promise<void>;
  /** Set a task's status from the board. */
  onSetTaskStatus: (task: TaskItem, status: BoardStatus) => Promise<void>;
  /** Append a task line to today's journal entry (creating it if needed). */
  onAddTask: (line: string) => Promise<void>;
}

function pageLabel(path: string): string {
  return path.split('/').at(-1)?.replace(/\.md$/i, '') ?? path;
}

/** Task text as read: `[[Page|alias]]` shows as its alias or page name. */
function readable(text: string): string {
  return text.replace(/\[\[([^\]|]+?)(?:\\?\|([^\]]+))?\]\]/g, (_match, page: string, alias?: string) => (alias ?? page).trim());
}

function relativeDay(day: string, today: string): string {
  if (day === today) return 'Today';
  const [y, m, d] = day.split('-').map(Number);
  const [ty, tm, td] = today.split('-').map(Number);
  const diff = Math.round((new Date(y, m - 1, d).getTime() - new Date(ty, tm - 1, td).getTime()) / 86_400_000);
  if (diff === -1) return 'Yesterday';
  if (diff < 0) return `${-diff} days ago`;
  return dayLabel(day, today);
}

/** Tasks in the order given, grouped by the page they live in. */
function byPage(items: TaskItem[]): Array<[string, TaskItem[]]> {
  const groups = new Map<string, TaskItem[]>();
  for (const item of items) groups.set(item.path, [...(groups.get(item.path) ?? []), item]);
  return [...groups.entries()];
}

export function TasksView({ workspaceSlug, journal, onOpenPage, onToggleTask, onSetTaskStatus, onAddTask }: TasksViewProps) {
  const [tasks, setTasks] = useState<TaskItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState('');
  const [filter, setFilter] = useState<Filter>('today');
  const [pending, setPending] = useState<Set<string>>(new Set());
  const [draft, setDraft] = useState('');
  const [draftDue, setDraftDue] = useState('');
  const [adding, setAdding] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);
  const [week, setWeek] = useState(() => startOfIsoWeek(new Date()));
  const [dragging, setDragging] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<BoardStatus | null>(null);
  const today = dateKey(new Date());
  const refresh = () => setReloadKey((key) => key + 1);

  // Reload whenever the Space, journal settings or reload key change; a
  // newer load cancels an older one so results never arrive out of order.
  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      setRefreshing(true);
      listTaskSources(workspaceSlug)
        .then((sources) => {
          if (cancelled) return;
          setTasks(sources.flatMap((source) => {
            const day = journalDateFromPath(journal, source.path);
            return extractTasks(source.path, source.content, day ? dateKey(day) : null);
          }));
          setPending(new Set());
          setError('');
        })
        .catch((caught: unknown) => {
          if (!cancelled) setError(caught instanceof Error ? caught.message : String(caught));
        })
        .finally(() => {
          if (!cancelled) {
            setLoading(false);
            setRefreshing(false);
          }
        });
    }, 0);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [workspaceSlug, journal, reloadKey]);

  useEffect(() => {
    const onFocus = () => setReloadKey((key) => key + 1);
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, []);

  const groups = useMemo(() => groupTasks(tasks, today), [tasks, today]);
  const board = useMemo(() => weekBoard(tasks, dateKey(week), dateKey(addDays(week, 6))), [tasks, week]);
  const counts: Record<Filter, number> = {
    today: groups.overdue.length + groups.today.length,
    week: board.todo.length + board.doing.length + board.done.length,
    upcoming: groups.upcoming.length,
    someday: groups.someday.length,
    done: groups.done.length,
  };

  // The row stays pending until the reload that follows the write clears it.
  const toggle = async (task: TaskItem) => {
    setPending((previous) => new Set(previous).add(taskKey(task)));
    try {
      await onToggleTask(task);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
    refresh();
  };

  const move = async (task: TaskItem, status: BoardStatus) => {
    if (task.status === status) return;
    setPending((previous) => new Set(previous).add(taskKey(task)));
    try {
      await onSetTaskStatus(task, status);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
    refresh();
  };

  const add = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!draft.trim() || adding) return;
    setAdding(true);
    try {
      await onAddTask(newTaskLine(draft, draftDue || null));
      setDraft('');
      setDraftDue('');
      refresh();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setAdding(false);
    }
  };

  const sections: Array<{ title: string | null; items: TaskItem[]; tone?: 'red' }> = filter === 'today'
    ? [
      { title: 'Overdue', items: groups.overdue, tone: 'red' },
      { title: groups.overdue.length > 0 ? 'Today' : null, items: groups.today },
    ]
    : filter === 'upcoming'
      ? [{ title: null, items: groups.upcoming }]
      : filter === 'someday'
        ? [{ title: null, items: groups.someday }]
        : [{ title: null, items: groups.done }];

  const tabs: Array<[Filter, string]> = [
    ['today', 'Today'],
    ['week', 'This week'],
    ['upcoming', 'Upcoming'],
    ['someday', 'No date'],
    ['done', 'Done'],
  ];

  return (
    <section style={{ width: filter === 'week' ? 'min(1040px, 100%)' : 'min(860px, 100%)' }}>
      <ViewHeader
        title="Tasks"
        actions={(
          <ViewIconButton label="Refresh" onClick={refresh} disabled={refreshing}>
            <RefreshCw className={cn('size-3.5', refreshing && 'animate-spin')} />
          </ViewIconButton>
        )}
        className="mb-4"
      />

      <ViewTabs label="Task lists" tabs={tabs.map(([id, label]) => ({ id, label, count: counts[id] }))} value={filter} onChange={setFilter} />

      {error && <div role="alert" className="mb-3 text-[12.5px] text-red">{error}</div>}

      {filter !== 'week' && filter !== 'done' && (
        <form onSubmit={add} className="group mb-2 flex items-center gap-2 rounded-md px-2 py-1 transition-colors focus-within:bg-bg-hover/60 hover:bg-bg-hover/60">
          <Plus className="size-4 shrink-0 text-text-faint" aria-hidden />
          <input
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            placeholder="Add a task"
            aria-label="New task"
            className="h-7 min-w-0 flex-1 bg-transparent text-[13.5px] text-text outline-none placeholder:text-text-faint focus-visible:outline-none"
          />
          {(draft.trim() || draftDue) && <DatePicker value={draftDue} onChange={setDraftDue} label="Due date" />}
          {draft.trim() && (
            <button type="submit" disabled={adding} className="h-7 shrink-0 rounded-md px-2.5 text-[12px] font-medium text-accent transition-colors hover:bg-accent-soft disabled:opacity-60">
              {adding ? 'Adding…' : 'Add'}
            </button>
          )}
        </form>
      )}

      {loading ? (
        <p className="flex items-center gap-2 py-10 text-[12.5px] text-text-tertiary"><LoaderCircle className="size-3.5 animate-spin" /> Collecting tasks…</p>
      ) : filter === 'week' ? (
        <div>
          <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
            <WeekNav week={week} onOpenWeek={(date) => setWeek(startOfIsoWeek(date))} />
          </div>
          <div className="grid grid-cols-3 gap-3">
            {COLUMNS.map((column) => {
              const items = board[column.status];
              return (
                <div
                  key={column.status}
                  role="list"
                  aria-label={column.title}
                  onDragOver={(event) => { event.preventDefault(); setDropTarget(column.status); }}
                  onDragLeave={() => setDropTarget((current) => (current === column.status ? null : current))}
                  onDrop={(event) => {
                    event.preventDefault();
                    setDropTarget(null);
                    const key = event.dataTransfer.getData('text/plain') || dragging;
                    const task = tasks.find((item) => taskKey(item) === key);
                    setDragging(null);
                    if (task) void move(task, column.status);
                  }}
                  className={cn(
                    'min-h-40 rounded-lg p-1.5 transition-colors',
                    dropTarget === column.status ? 'bg-accent-soft/60 ring-1 ring-inset ring-accent/40' : 'bg-bg-secondary/70',
                  )}
                >
                  <h2 className="mb-1.5 flex items-center gap-1.5 px-1.5 pt-0.5 text-[12.5px] font-medium text-text-secondary">
                    {column.title}
                    <span className="font-normal tabular-nums text-text-faint">{items.length}</span>
                  </h2>
                  {items.map((task) => {
                    const key = taskKey(task);
                    const day = column.status === 'done' ? task.doneDate : taskDay(task);
                    return (
                      <div
                        key={key}
                        role="listitem"
                        draggable={!pending.has(key)}
                        onDragStart={(event) => { event.dataTransfer.setData('text/plain', key); setDragging(key); }}
                        onDragEnd={() => { setDragging(null); setDropTarget(null); }}
                        className={cn(
                          'group mb-1.5 cursor-grab rounded-md border border-border-soft bg-panel px-2.5 py-2 shadow-[var(--shadow-rest)] transition-opacity',
                          (pending.has(key) || dragging === key) && 'opacity-50',
                        )}
                      >
                        <div className="flex items-start gap-2">
                          <input
                            type="checkbox"
                            checked={column.status === 'done'}
                            disabled={pending.has(key)}
                            onChange={() => { void move(task, column.status === 'done' ? 'todo' : 'done'); }}
                            aria-label={column.status === 'done' ? `Reopen ${task.text}` : `Complete ${task.text}`}
                            className="mt-[3px]"
                          />
                          <span className={cn('min-w-0 flex-1 text-[13px] leading-[1.45] [overflow-wrap:anywhere]', column.status === 'done' ? 'text-text-tertiary' : 'text-text')}>
                            {readable(task.text)}
                          </span>
                          {column.status === 'todo' && (
                            <button type="button" onClick={() => { void move(task, 'doing'); }} disabled={pending.has(key)} className="rounded p-0.5 text-text-faint opacity-0 transition-opacity hover:text-text group-hover:opacity-100" aria-label={`Start ${task.text}`} title="Start">
                              <Play className="size-3" />
                            </button>
                          )}
                        </div>
                        <div className="mt-1 flex gap-2 pl-[23px] text-[11px] text-text-tertiary">
                          <button type="button" onClick={() => onOpenPage(task.path)} className="max-w-40 truncate hover:text-text-secondary" title={task.path}>
                            {pageLabel(task.path)}
                          </button>
                          {day && <span className={cn(column.status !== 'done' && day < today && 'text-red')}>{relativeDay(day, today)}</span>}
                        </div>
                      </div>
                    );
                  })}
                </div>
              );
            })}
          </div>
        </div>
      ) : sections.every((section) => section.items.length === 0) ? (
        <p className="py-10 text-center text-[12.5px] text-text-tertiary">
          {filter === 'today' ? 'Nothing due today.' : filter === 'done' ? 'Nothing finished yet.' : 'No tasks here.'}
        </p>
      ) : (
        sections.filter((section) => section.items.length > 0).map((section) => (
          <div key={section.title ?? 'all'} className="mb-5">
            {section.title && (
              <h2 className={cn('mb-1 px-2 text-[12.5px] font-medium', section.tone === 'red' ? 'text-red' : 'text-text-secondary')}>
                {section.title} <span className="font-normal tabular-nums text-text-faint">{section.items.length}</span>
              </h2>
            )}
            {byPage(section.items).map(([path, items]) => (
              <div key={path} className="mb-3">
                <button
                  type="button"
                  onClick={() => onOpenPage(path)}
                  className="group flex max-w-full items-center gap-1.5 rounded-md px-2 py-1 text-[12px] text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text-secondary"
                  title={path}
                >
                  <FileText className="size-3.5 shrink-0" aria-hidden />
                  <span className="truncate">{pageLabel(path)}</span>
                  <ArrowUpRight className="size-3 shrink-0 opacity-0 transition-opacity group-hover:opacity-100" aria-hidden />
                </button>
                <ul className="m-0 list-none p-0">
                  {items.map((task) => {
                    const key = taskKey(task);
                    const day = task.due ?? task.scheduled ?? task.journalDate;
                    const done = task.status === 'done' || task.status === 'cancelled';
                    return (
                      <li key={key} className={cn('flex items-start gap-2.5 rounded-md px-2 py-1.5 transition-colors hover:bg-bg-hover/60', pending.has(key) && 'opacity-55')}>
                        <input
                          type="checkbox"
                          checked={done}
                          disabled={pending.has(key)}
                          onChange={() => { void toggle(task); }}
                          aria-label={done ? `Reopen ${task.text}` : `Complete ${task.text}`}
                          className="mt-[3px]"
                        />
                        <span className={cn(
                          'min-w-0 flex-1 text-[13.5px] leading-[1.5] [overflow-wrap:anywhere]',
                          done ? 'text-text-tertiary' : 'text-text',
                          task.status === 'cancelled' && 'line-through',
                        )}>
                          {task.priority && task.priority !== 'medium' && task.priority !== 'low' && task.priority !== 'lowest' && (
                            <span title={`${task.priority} priority`} className="mr-1 text-red">!</span>
                          )}
                          {readable(task.text)}
                        </span>
                        {task.status === 'doing' && <span className="mt-0.5 flex shrink-0 items-center gap-1 text-[11.5px] text-blue"><span className="size-1.5 rounded-full bg-current" />Doing</span>}
                        {done && task.doneDate ? (
                          <span className="mt-0.5 shrink-0 text-[11.5px] text-text-faint">{relativeDay(task.doneDate, today)}</span>
                        ) : day && (filter !== 'today' || day !== today) && (
                          <span className={cn('mt-0.5 shrink-0 text-[11.5px]', !done && day < today ? 'text-red' : 'text-text-tertiary')}>
                            {relativeDay(day, today)}
                          </span>
                        )}
                      </li>
                    );
                  })}
                </ul>
              </div>
            ))}
          </div>
        ))
      )}
    </section>
  );
}
