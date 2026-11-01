import { useEffect, useMemo, useState } from 'react';
import { CalendarDays, CheckSquare, LoaderCircle, Play, Plus, RefreshCw } from 'lucide-react';

import { WeekNav } from '@/components/WeekNav';
import { listTaskSources } from '@/local-api';
import { C } from '@/lib/design';
import { addDays, dateKey, journalDateFromPath, startOfIsoWeek, type JournalConfig } from '@/lib/journal';
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
  if (diff === 1) return 'Tomorrow';
  if (diff === -1) return 'Yesterday';
  if (diff < 0) return `${-diff} days ago`;
  if (diff < 7) return new Date(y, m - 1, d).toLocaleDateString(undefined, { weekday: 'long' });
  return day;
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

  const sections: Array<{ title: string; items: TaskItem[]; tone?: 'red' }> = filter === 'today' || filter === 'week'
    ? [
      { title: 'Overdue', items: groups.overdue, tone: 'red' },
      { title: 'Today', items: groups.today },
    ]
    : filter === 'upcoming'
      ? [{ title: 'Upcoming', items: groups.upcoming }]
      : filter === 'someday'
        ? [{ title: 'No date', items: groups.someday }]
        : [{ title: 'Recently done', items: groups.done }];

  return (
    <section style={{ width: filter === 'week' ? 'min(1040px, 100%)' : 'min(760px, 100%)', margin: '0 auto' }}>
      <header style={{ marginBottom: 18 }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 16, marginBottom: 8 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <CheckSquare size={20} color={C.accent} strokeWidth={1.8} />
            <h1 className="page-title page-title--compact" style={{ margin: 0 }}>Tasks</h1>
          </div>
          <button type="button" onClick={refresh} disabled={refreshing} style={ghostButton}>
            <RefreshCw size={13} className={refreshing ? 'animate-spin' : undefined} />
            <span>Refresh</span>
          </button>
        </div>
        <p style={{ margin: 0, color: C.muted, fontSize: 13 }}>
          Checkbox tasks from every page. Checking one here updates the line in its page.
        </p>
      </header>

      <form onSubmit={add} style={addRow}>
        <Plus size={15} color={C.muted} />
        <input
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          placeholder="Add a task to today's journal…"
          aria-label="New task"
          style={{ flex: 1, minWidth: 0, border: 'none', outline: 'none', background: 'transparent', color: C.ink, fontSize: 13.5 }}
        />
        <label style={{ display: 'inline-flex', alignItems: 'center', gap: 5, color: C.muted, fontSize: 12 }}>
          <CalendarDays size={13} />
          <input
            type="date"
            value={draftDue}
            onChange={(event) => setDraftDue(event.target.value)}
            aria-label="Due date"
            style={{ border: 'none', background: 'transparent', color: C.ink2, fontSize: 12 }}
          />
        </label>
        <button type="submit" disabled={!draft.trim() || adding} style={primaryButton}>
          {adding ? 'Adding…' : 'Add'}
        </button>
      </form>

      <nav role="tablist" aria-label="Task lists" style={{ display: 'flex', gap: 4, margin: '16px 0 14px' }}>
        {([
          ['today', 'Today'],
          ['week', 'This week'],
          ['upcoming', 'Upcoming'],
          ['someday', 'No date'],
          ['done', 'Done'],
        ] as Array<[Filter, string]>).map(([id, label]) => (
          <button
            key={id}
            role="tab"
            aria-selected={filter === id}
            onClick={() => setFilter(id)}
            style={{
              ...tabButton,
              background: filter === id ? C.accentSoft : 'transparent',
              color: filter === id ? C.accentInk : C.ink2,
            }}
          >
            {label}
            <span style={{ color: filter === id ? C.accentInk : C.faint, fontSize: 11.5 }}>{counts[id]}</span>
          </button>
        ))}
      </nav>

      {error && (
        <div role="alert" style={{ marginBottom: 12, color: C.red, fontSize: 12.5 }}>{error}</div>
      )}

      {loading ? (
        <div style={emptyState}><LoaderCircle size={15} className="animate-spin" /> Collecting tasks…</div>
      ) : filter === 'week' ? (
        <div>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, marginBottom: 10, flexWrap: 'wrap' }}>
            <WeekNav week={week} onOpenWeek={(date) => setWeek(startOfIsoWeek(date))} />
            <span style={{ fontSize: 12, color: C.muted }}>
              {board.done.length} done · {board.doing.length} doing · {board.todo.length} to do
            </span>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, minmax(0, 1fr))', gap: 10 }}>
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
                  style={{
                    minHeight: 160, padding: 8, borderRadius: 10, background: C.panel,
                    border: `1px ${dropTarget === column.status ? 'dashed' : 'solid'} ${dropTarget === column.status ? C.accent : C.line}`,
                  }}
                >
                  <h2 style={{ margin: '2px 4px 8px', fontSize: 11.5, fontWeight: 650, letterSpacing: '.04em', textTransform: 'uppercase', color: C.muted }}>
                    {column.title} · {items.length}
                  </h2>
                  {items.length === 0 && (
                    <div style={{ padding: '14px 6px', fontSize: 12, color: C.faint, textAlign: 'center' }}>
                      {column.status === 'done' ? 'Nothing finished yet' : 'Drop tasks here'}
                    </div>
                  )}
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
                        style={{
                          marginBottom: 6, padding: '8px 9px', borderRadius: 8, border: `1px solid ${C.lineSoft}`, background: C.bg,
                          cursor: 'grab', opacity: pending.has(key) || dragging === key ? 0.5 : 1,
                        }}
                      >
                        <div style={{ display: 'flex', gap: 7, alignItems: 'flex-start' }}>
                          <input
                            type="checkbox"
                            checked={column.status === 'done'}
                            disabled={pending.has(key)}
                            onChange={() => { void move(task, column.status === 'done' ? 'todo' : 'done'); }}
                            aria-label={column.status === 'done' ? `Reopen ${task.text}` : `Complete ${task.text}`}
                            style={{ marginTop: 3, cursor: 'pointer' }}
                          />
                          <span style={{ flex: 1, minWidth: 0, fontSize: 13, lineHeight: 1.4, color: column.status === 'done' ? C.muted : C.ink, overflowWrap: 'anywhere' }}>
                            {readable(task.text)}
                          </span>
                          {column.status === 'todo' && (
                            <button type="button" onClick={() => { void move(task, 'doing'); }} disabled={pending.has(key)} style={iconButton} aria-label={`Start ${task.text}`} title="Start">
                              <Play size={12} />
                            </button>
                          )}
                        </div>
                        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 4, paddingLeft: 21, fontSize: 11, color: C.muted }}>
                          <button type="button" onClick={() => onOpenPage(task.path)} style={{ ...linkButton, fontSize: 11, maxWidth: 150 }} title={task.path}>
                            {pageLabel(task.path)}
                          </button>
                          {day && <span style={{ color: column.status !== 'done' && day < today ? C.red : C.muted }}>{relativeDay(day, today)}</span>}
                        </div>
                      </div>
                    );
                  })}
                </div>
              );
            })}
          </div>
          <p style={{ margin: '10px 2px 0', fontSize: 11.5, color: C.muted }}>
            Drag a card to change its status. Doing is saved as <code>[/]</code> (Obsidian Tasks) or <code>DOING</code> (Logseq) in the page.
          </p>
        </div>
      ) : sections.every((section) => section.items.length === 0) ? (
        <div style={emptyState}>
          {filter === 'today' ? 'Nothing due today. Add a task above or write `- [ ]` in any page.' : 'No tasks here.'}
        </div>
      ) : (
        sections.filter((section) => section.items.length > 0).map((section) => (
          <div key={section.title} style={{ marginBottom: 18 }}>
            <h2 style={{
              margin: '0 0 6px', fontSize: 11.5, fontWeight: 650, letterSpacing: '.04em', textTransform: 'uppercase',
              color: section.tone === 'red' ? C.red : C.muted,
            }}>
              {section.title} · {section.items.length}
            </h2>
            <ul style={{ listStyle: 'none', margin: 0, padding: 0, border: `1px solid ${C.line}`, borderRadius: 10, background: C.panel }}>
              {section.items.map((task, index) => {
                const key = taskKey(task);
                const day = task.due ?? task.scheduled ?? task.journalDate;
                const done = task.status === 'done' || task.status === 'cancelled';
                return (
                  <li
                    key={key}
                    style={{
                      display: 'flex', alignItems: 'flex-start', gap: 10, padding: '9px 12px',
                      borderTop: index === 0 ? 'none' : `1px solid ${C.lineSoft}`,
                      opacity: pending.has(key) ? 0.55 : 1,
                    }}
                  >
                    <input
                      type="checkbox"
                      checked={done}
                      disabled={pending.has(key)}
                      onChange={() => { void toggle(task); }}
                      aria-label={done ? `Reopen ${task.text}` : `Complete ${task.text}`}
                      style={{ marginTop: 3, cursor: 'pointer' }}
                    />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{
                        fontSize: 13.5, color: done ? C.muted : C.ink, lineHeight: 1.45,
                        textDecoration: task.status === 'cancelled' ? 'line-through' : undefined,
                        overflowWrap: 'anywhere',
                      }}>
                        {task.priority && task.priority !== 'medium' && task.priority !== 'low' && task.priority !== 'lowest' && (
                          <span title={`${task.priority} priority`} style={{ color: C.red, marginRight: 4 }}>!</span>
                        )}
                        {readable(task.text)}
                        {task.status === 'doing' && <span style={pill(C.blueSoft, C.blue)}>doing</span>}
                      </div>
                      <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 8, marginTop: 3, fontSize: 11.5, color: C.muted }}>
                        <button type="button" onClick={() => onOpenPage(task.path)} style={linkButton} title={task.path}>
                          {pageLabel(task.path)}
                        </button>
                        {day && (
                          <span style={{ color: !done && day < today ? C.red : C.muted }}>
                            {task.due ? 'Due ' : task.scheduled ? 'Scheduled ' : ''}{relativeDay(day, today)}
                          </span>
                        )}
                        {task.doneDate && done && <span>Done {relativeDay(task.doneDate, today)}</span>}
                      </div>
                    </div>
                  </li>
                );
              })}
            </ul>
          </div>
        ))
      )}
    </section>
  );
}

function pill(background: string, color: string): React.CSSProperties {
  return {
    marginLeft: 6, padding: '1px 6px', borderRadius: 999, background, color,
    fontSize: 10.5, fontWeight: 650, verticalAlign: 'middle',
  };
}

const ghostButton: React.CSSProperties = {
  display: 'inline-flex', alignItems: 'center', gap: 6, padding: '6px 9px',
  border: `1px solid ${C.line}`, borderRadius: 7, background: C.panel,
  color: C.ink2, fontSize: 12, fontWeight: 620, cursor: 'pointer',
};

const primaryButton: React.CSSProperties = {
  padding: '5px 11px', border: 'none', borderRadius: 7, background: C.accent,
  color: C.onAccent, fontSize: 12, fontWeight: 650, cursor: 'pointer',
};

const addRow: React.CSSProperties = {
  display: 'flex', alignItems: 'center', gap: 10, padding: '8px 10px 8px 12px',
  border: `1px solid ${C.line}`, borderRadius: 10, background: C.panel,
};

const tabButton: React.CSSProperties = {
  display: 'inline-flex', alignItems: 'center', gap: 6, padding: '5px 10px',
  border: 'none', borderRadius: 7, fontSize: 12.5, fontWeight: 620, cursor: 'pointer',
};

const iconButton: React.CSSProperties = {
  display: 'inline-flex', alignItems: 'center', justifyContent: 'center', padding: 3,
  border: 'none', borderRadius: 5, background: 'transparent', color: C.muted, cursor: 'pointer',
};

const linkButton: React.CSSProperties = {
  padding: 0, border: 'none', background: 'transparent', color: C.accent,
  fontSize: 11.5, cursor: 'pointer', maxWidth: 260, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
};

const emptyState: React.CSSProperties = {
  display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 8, minHeight: 140,
  border: `1px dashed ${C.line}`, borderRadius: 11, color: C.muted, fontSize: 12.5, textAlign: 'center', padding: 16,
};
