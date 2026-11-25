import { useEffect, useMemo, useState } from 'react';
import { ArrowUpRight, Columns3, FileText, List as ListIcon, LoaderCircle, Play, Plus, RefreshCw, Undo2 } from 'lucide-react';

import { ViewHeader, ViewIconButton, ViewTabs } from '@/components/ui/view-header';
import { DatePicker, dayLabel } from '@/components/ui/date-picker';
import { WeekNav } from '@/components/WeekNav';
import { listTaskSources } from '@/local-api';
import { addDays, dateKey, fromDateKey, journalDateFromPath, startOfIsoWeek, type JournalConfig } from '@/lib/journal';
import { useTodayKey } from '@/lib/useTodayKey';
import { useI18n } from '@/lib/I18nContext';
import { cn } from '@/lib/utils';
import {
  effectiveDoneDate,
  extractTasks,
  groupTasks,
  newTaskLine,
  tabBoard,
  taskDay,
  taskKey,
  weekBoard,
  type BoardStatus,
  type TaskItem,
  type TaskTab,
  type WeekBoard,
} from '@/lib/tasks';

type Filter = TaskTab;
type Layout = 'list' | 'board';
type Columns = Pick<WeekBoard, 'todo' | 'doing' | 'done'>;

/** Each tab's layout is remembered on this device; This week starts as a board. */
const LAYOUT_KEY = 'cowiki.tasks.layout';
function readLayouts(): Partial<Record<Filter, Layout>> {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? '{}');
    return saved && typeof saved === 'object' ? saved as Partial<Record<Filter, Layout>> : {};
  } catch {
    return {};
  }
}

/** List or board, as a small two-icon switch in the title row. */
function LayoutToggle({ value, onChange }: { value: Layout; onChange: (layout: Layout) => void }) {
  const { t } = useI18n();
  return (
    <div role="group" aria-label={t('Layout')} className="mr-1 flex items-center rounded-md bg-bg-hover p-0.5">
      {(['list', 'board'] as const).map((id) => (
        <button
          key={id}
          type="button"
          aria-pressed={value === id}
          aria-label={id === 'list' ? t('List') : t('Board')}
          title={id === 'list' ? t('List') : t('Board')}
          onClick={() => onChange(id)}
          className={cn(
            'flex size-7 items-center justify-center rounded transition-colors',
            value === id ? 'bg-panel text-text shadow-[var(--shadow-chip)]' : 'text-text-tertiary hover:text-text',
          )}
        >
          {id === 'list' ? <ListIcon className="size-3.5" /> : <Columns3 className="size-3.5" />}
        </button>
      ))}
    </div>
  );
}

const COLUMNS: Array<{ status: BoardStatus; title: string }> = [
  { status: 'todo', title: 'To do' },
  { status: 'doing', title: 'Doing' },
  { status: 'done', title: 'Done' },
];

interface TasksViewProps {
  workspaceSlug: string;
  journal: JournalConfig;
  /** Open the task's page; `line` is the 0-based file line when clicking a specific task. */
  onOpenPage: (path: string, line?: number) => void;
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

function relativeDay(day: string, today: string, locale: 'en' | 'zh-CN', t: ReturnType<typeof useI18n>['t']): string {
  if (day === today) return t('Today');
  const [y, m, d] = day.split('-').map(Number);
  const [ty, tm, td] = today.split('-').map(Number);
  const diff = Math.round((new Date(y, m - 1, d).getTime() - new Date(ty, tm - 1, td).getTime()) / 86_400_000);
  if (diff === -1) return t('Yesterday');
  if (diff < 0) return t('{count} days ago', { count: -diff });
  return dayLabel(day, today, locale);
}

/** Tasks in the order given, grouped by the page they live in. */
function byPage(items: TaskItem[]): Array<[string, TaskItem[]]> {
  const groups = new Map<string, TaskItem[]>();
  for (const item of items) groups.set(item.path, [...(groups.get(item.path) ?? []), item]);
  return [...groups.entries()];
}

export function TasksView({ workspaceSlug, journal, onOpenPage, onToggleTask, onSetTaskStatus, onAddTask }: TasksViewProps) {
  const { locale, t, tl } = useI18n();
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
  const [layouts, setLayouts] = useState(readLayouts);
  const layout: Layout = filter === 'done' ? 'list' : layouts[filter] ?? (filter === 'week' ? 'board' : 'list');
  const setLayout = (next: Layout) => setLayouts((previous) => {
    const all = { ...previous, [filter]: next };
    try { localStorage.setItem(LAYOUT_KEY, JSON.stringify(all)); } catch { /* the choice lasts this session */ }
    return all;
  });
  const today = useTodayKey();
  const todayDate = fromDateKey(today) ?? new Date();
  const tomorrow = dateKey(addDays(todayDate, 1));
  const selectFilter = (next: Filter) => {
    setFilter(next);
    setDraftDue(next === 'upcoming' ? tomorrow : '');
  };
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
      const due = draftDue || (filter === 'upcoming' ? tomorrow : null);
      await onAddTask(newTaskLine(draft, due));
      setDraft('');
      setDraftDue(filter === 'upcoming' ? tomorrow : '');
      refresh();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setAdding(false);
    }
  };

  const columns: Columns | null = filter === 'week' ? board : filter === 'done' ? null : tabBoard(tasks, groups, filter, today);

  const sections: Array<{ title: string | null; items: TaskItem[]; tone?: 'red' }> = filter === 'today'
    ? [
      { title: t('Overdue'), items: groups.overdue, tone: 'red' },
      { title: groups.overdue.length > 0 ? t('Today') : null, items: groups.today },
    ]
    : filter === 'week'
      ? [
        { title: t('Doing'), items: board.doing },
        { title: t('To do'), items: board.todo },
        { title: t('Done'), items: board.done },
      ]
    : filter === 'upcoming'
      ? [{ title: null, items: groups.upcoming }]
      : filter === 'someday'
        ? [{ title: null, items: groups.someday }]
        : [{ title: null, items: groups.done }];

  const tabs: Array<[Filter, string]> = [
    ['today', t('Today')],
    ['week', t('This week')],
    ['upcoming', t('Upcoming')],
    ['someday', t('No date')],
    ['done', t('Done')],
  ];

  /** To do, Doing and Done columns; drag a card, or start or finish it in place. */
  const renderBoard = (cols: Columns) => (
    <div className="grid grid-cols-3 gap-3">
      {COLUMNS.map((column) => {
        const items = cols[column.status];
        const colTitle = tl(column.title);
        return (
          <div
            key={column.status}
            role="list"
            aria-label={colTitle}
            onDragOver={(event) => { event.preventDefault(); setDropTarget(column.status); }}
            onDragLeave={(event) => {
              if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
                setDropTarget((current) => (current === column.status ? null : current));
              }
            }}
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
            <h2 className="mb-1.5 flex items-center gap-1.5 px-1.5 pt-0.5 text-small font-medium text-text-secondary">
              {colTitle}
              <span className="font-normal tabular-nums text-text-faint">{items.length}</span>
            </h2>
            {items.map((task) => {
              const key = taskKey(task);
              const day = column.status === 'done' ? effectiveDoneDate(task) : taskDay(task);
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
                      className="mt-[2px]"
                    />
                    <button
                      type="button"
                      onClick={() => onOpenPage(task.path, task.line)}
                      className={cn(
                        'min-w-0 flex-1 text-left text-small leading-[1.45] hover:underline hover:decoration-border-hover hover:underline-offset-2 [overflow-wrap:anywhere]',
                        column.status === 'done' ? 'text-text-tertiary' : 'text-text',
                      )}
                    >
                      {(task.priority === 'highest' || task.priority === 'high') && (
                        <span title={`${task.priority} priority`} className="mr-1 text-red">!</span>
                      )}
                      {readable(task.text)}
                    </button>
                    {column.status === 'todo' && (
                      <button type="button" onClick={() => { void move(task, 'doing'); }} disabled={pending.has(key)} className="rounded p-0.5 text-text-faint opacity-0 transition-opacity hover:text-text group-hover:opacity-100 focus-visible:opacity-100" aria-label={`Start ${task.text}`} title={t('Start')}>
                        <Play className="size-3" />
                      </button>
                    )}
                    {column.status === 'doing' && (
                      <button type="button" onClick={() => { void move(task, 'todo'); }} disabled={pending.has(key)} className="rounded p-0.5 text-text-faint opacity-0 transition-opacity hover:text-text group-hover:opacity-100 focus-visible:opacity-100" aria-label={`Move ${task.text} back to To do`} title={t('Move back to To do')}>
                        <Undo2 className="size-3" />
                      </button>
                    )}
                  </div>
                  <div className="mt-1 flex gap-2 pl-[23px] text-caption text-text-tertiary">
                    <button type="button" onClick={() => onOpenPage(task.path)} className="max-w-40 truncate hover:text-text-secondary" title={task.path}>
                      {pageLabel(task.path)}
                    </button>
                    {day && <span className={cn(column.status !== 'done' && day < today && 'text-red')}>{relativeDay(day, today, locale, t)}</span>}
                  </div>
                </div>
              );
            })}
          </div>
        );
      })}
    </div>
  );

  return (
    <section style={{ width: layout === 'board' ? 'min(1040px, 100%)' : 'min(860px, 100%)' }}>
      <ViewHeader
        title={t('Tasks')}
        actions={(
          <>
            {filter !== 'done' && <LayoutToggle value={layout} onChange={setLayout} />}
            <ViewIconButton label={t('Refresh')} onClick={refresh} disabled={refreshing}>
              <RefreshCw className={cn('size-3.5', refreshing && 'animate-spin')} />
            </ViewIconButton>
          </>
        )}
        className="mb-4"
      />

      <ViewTabs label={t('Task lists')} tabs={tabs.map(([id, label]) => ({ id, label, count: counts[id] }))} value={filter} onChange={selectFilter} />

      {error && <div role="alert" className="mb-3 text-small text-red">{error}</div>}

      {(filter === 'today' || filter === 'upcoming') && (
        <form onSubmit={add} className="group -mx-2 mb-3 flex items-center gap-2.5 rounded-md px-2 py-1 transition-colors focus-within:bg-bg-hover/60 hover:bg-bg-hover/60">
          <Plus className="size-[15px] shrink-0 text-text-faint" aria-hidden />
          <input
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            placeholder={t('Add a task')}
            aria-label={t('New task')}
            className="h-7 min-w-0 flex-1 bg-transparent text-body text-text outline-none placeholder:text-text-faint focus-visible:outline-none"
          />
          {(draft.trim() || draftDue) && <DatePicker value={draftDue} onChange={setDraftDue} label="Due date" />}
          {draft.trim() && (
            <button type="submit" disabled={adding} className="h-7 shrink-0 rounded-md px-2.5 text-meta font-medium text-accent transition-colors hover:bg-accent-soft disabled:opacity-60">
              {adding ? t('Adding…') : t('Add')}
            </button>
          )}
        </form>
      )}

      {filter === 'week' && !loading && (
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <WeekNav week={week} onOpenWeek={(date) => setWeek(startOfIsoWeek(date))} />
        </div>
      )}

      {loading ? (
        <p className="flex items-center gap-2 py-10 text-small text-text-tertiary"><LoaderCircle className="size-3.5 animate-spin" /> {t('Collecting tasks…')}</p>
      ) : layout === 'board' && columns ? (
        renderBoard(columns)
      ) : sections.every((section) => section.items.length === 0) ? (
        <p className="py-10 text-center text-small text-text-tertiary">
          {filter === 'today' ? t('Nothing due today.') : filter === 'week' ? t('Nothing this week.') : filter === 'done' ? t('Nothing finished yet.') : t('No tasks here.')}
        </p>
      ) : (
        sections.filter((section) => section.items.length > 0).map((section) => (
          <div key={section.title ?? 'all'} className="mb-5">
            {section.title && (
              <h2 className={cn('mb-1.5 text-small font-semibold', section.tone === 'red' ? 'text-red' : 'text-text')}>
                {section.title} <span className="font-normal tabular-nums text-text-faint">{section.items.length}</span>
              </h2>
            )}
            {byPage(section.items).map(([path, items]) => (
              <div key={path} className="mb-3">
                <button
                  type="button"
                  onClick={() => onOpenPage(path)}
                  className="group -mx-2 flex max-w-full items-center gap-1.5 rounded-md px-2 py-1 text-meta text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text-secondary"
                  title={path}
                >
                  <FileText className="size-3.5 shrink-0" aria-hidden />
                  <span className="truncate">{pageLabel(path)}</span>
                  <ArrowUpRight className="size-3 shrink-0 opacity-0 transition-opacity group-hover:opacity-100" aria-hidden />
                </button>
                <ul className="-mx-2 m-0 list-none p-0">
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
                          className="mt-[3.5px]"
                        />
                        <button
                          type="button"
                          onClick={() => onOpenPage(task.path, task.line)}
                          className="min-w-0 flex-1 text-left"
                        >
                          <span className={cn(
                            'block text-body leading-[22px] hover:underline hover:decoration-border-hover hover:underline-offset-2 [overflow-wrap:anywhere]',
                            done ? 'text-text-tertiary' : 'text-text',
                            task.status === 'cancelled' && 'line-through',
                          )}>
                            {task.priority && task.priority !== 'medium' && task.priority !== 'low' && task.priority !== 'lowest' && (
                              <span title={`${task.priority} priority`} className="mr-1 text-red">!</span>
                            )}
                            {readable(task.text)}
                          </span>
                        </button>
                        {task.status === 'doing' && filter !== 'week' && <span className="mt-[3px] flex shrink-0 items-center gap-1 text-meta leading-4 text-blue"><span className="size-1.5 rounded-full bg-current" />{t('Doing')}</span>}
                        {done && effectiveDoneDate(task) ? (
                          <span className="mt-[3px] shrink-0 text-meta leading-4 text-text-faint">{relativeDay(effectiveDoneDate(task)!, today, locale, t)}</span>
                        ) : day && (filter !== 'today' || day !== today) && (
                          <span className={cn('mt-[3px] shrink-0 text-meta leading-4 tabular-nums', !done && day < today ? 'text-red' : 'text-text-tertiary')}>
                            {relativeDay(day, today, locale, t)}
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
