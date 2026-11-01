import { useEffect, useRef, useState } from 'react';
import { CalendarDays, ChevronLeft, ChevronRight } from 'lucide-react';

import { C, shadows } from '@/lib/design';
import { adjacentEntry, dateKey, formatDate, fromDateKey, monthGrid } from '@/lib/journal';

interface JournalNavProps {
  /** Day of the journal entry being shown. */
  date: Date;
  /** Date keys (`YYYY-MM-DD`) that already have an entry. */
  entryKeys: ReadonlySet<string>;
  onOpenDate: (date: Date) => void;
  style?: React.CSSProperties;
}

const WEEKDAY_LABELS = ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su'];

export function JournalNav({ date, entryKeys, onOpenDate, style }: JournalNavProps) {
  const [open, setOpen] = useState(false);
  // The browsed month follows the shown day until the person pages away.
  const [browsed, setBrowsed] = useState<{ day: string; month: Date } | null>(null);
  const wrapper = useRef<HTMLDivElement>(null);
  const today = new Date();
  const isToday = dateKey(date) === dateKey(today);
  const month = browsed && browsed.day === dateKey(date)
    ? browsed.month
    : new Date(date.getFullYear(), date.getMonth(), 1);
  const setMonth = (next: Date) => setBrowsed({ day: dateKey(date), month: next });
  const weeks = monthGrid(month, today, entryKeys);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (wrapper.current && !wrapper.current.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') setOpen(false); };
    window.addEventListener('mousedown', close);
    window.addEventListener('keydown', escape);
    return () => {
      window.removeEventListener('mousedown', close);
      window.removeEventListener('keydown', escape);
    };
  }, [open]);

  const go = (target: Date) => {
    setOpen(false);
    onOpenDate(target);
  };
  // Like Obsidian, previous/next step through existing entries instead of
  // creating an empty page for every day passed; the calendar can create one.
  const previous = adjacentEntry(entryKeys, dateKey(date), -1);
  const next = adjacentEntry(entryKeys, dateKey(date), 1);
  const previousDate = previous ? fromDateKey(previous) : null;
  const nextDate = next ? fromDateKey(next) : null;

  return (
    <div ref={wrapper} style={{ position: 'relative', display: 'inline-flex', alignItems: 'center', gap: 2, ...style }}>
      <button
        type="button"
        onClick={() => { if (previousDate) go(previousDate); }}
        disabled={!previousDate}
        title={previous ? `Previous entry (${previous})` : 'No earlier entry'}
        aria-label="Previous entry"
        style={{ ...iconButton, opacity: previousDate ? 1 : 0.35, cursor: previousDate ? 'pointer' : 'default' }}
      >
        <ChevronLeft size={14} />
      </button>
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        aria-haspopup="dialog"
        aria-expanded={open}
        title="Pick a day"
        style={{ ...iconButton, gap: 6, padding: '4px 8px', color: C.ink2, fontWeight: 600 }}
      >
        <CalendarDays size={13} color={C.accent} />
        {formatDate(date, 'ddd, MMM D')}
      </button>
      <button
        type="button"
        onClick={() => { if (nextDate) go(nextDate); }}
        disabled={!nextDate}
        title={next ? `Next entry (${next})` : 'No later entry'}
        aria-label="Next entry"
        style={{ ...iconButton, opacity: nextDate ? 1 : 0.35, cursor: nextDate ? 'pointer' : 'default' }}
      >
        <ChevronRight size={14} />
      </button>
      {!isToday && (
        <button type="button" onClick={() => go(today)} style={{ ...iconButton, padding: '4px 8px', color: C.accent, fontWeight: 650 }}>
          Today
        </button>
      )}
      {open && (
        <div role="dialog" aria-label="Journal calendar" style={popover}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
            <button type="button" aria-label="Previous month" style={iconButton} onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() - 1, 1))}>
              <ChevronLeft size={14} />
            </button>
            <span style={{ fontSize: 12.5, fontWeight: 650, color: C.ink }}>{formatDate(month, 'MMMM YYYY')}</span>
            <button type="button" aria-label="Next month" style={iconButton} onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() + 1, 1))}>
              <ChevronRight size={14} />
            </button>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(7, 30px)', gap: 2 }}>
            {WEEKDAY_LABELS.map((label) => (
              <span key={label} style={{ textAlign: 'center', fontSize: 10.5, color: C.faint, padding: '2px 0' }}>{label}</span>
            ))}
            {weeks.flat().map((cell) => {
              const selected = cell.key === dateKey(date);
              return (
                <button
                  key={cell.key}
                  type="button"
                  onClick={() => { const day = fromDateKey(cell.key); if (day) go(day); }}
                  title={cell.hasEntry ? `${cell.key} — has an entry` : cell.key}
                  style={{
                    position: 'relative', height: 28, border: 'none', borderRadius: 7, cursor: 'pointer',
                    fontSize: 12, fontWeight: cell.isToday ? 700 : 500,
                    color: selected ? C.onAccent : cell.inMonth ? C.ink : C.faint,
                    background: selected ? C.accent : cell.isToday ? C.accentSoft : 'transparent',
                  }}
                >
                  {cell.day}
                  {cell.hasEntry && (
                    <span style={{
                      position: 'absolute', left: '50%', bottom: 3, width: 4, height: 4, marginLeft: -2,
                      borderRadius: 999, background: selected ? C.onAccent : C.accent,
                    }} />
                  )}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

const iconButton: React.CSSProperties = {
  display: 'inline-flex', alignItems: 'center', justifyContent: 'center', padding: 5,
  border: 'none', borderRadius: 7, background: 'transparent', color: C.muted, cursor: 'pointer', fontSize: 12.5,
};

const popover: React.CSSProperties = {
  position: 'absolute', top: 'calc(100% + 6px)', right: 0, zIndex: 50, padding: 10,
  border: `1px solid ${C.line}`, borderRadius: 12, background: C.panel,
  boxShadow: shadows.raised,
};
