import { CalendarRange, ChevronLeft, ChevronRight } from 'lucide-react';

import { C } from '@/lib/design';
import { addDays, dateKey, formatDate, isoWeek, startOfIsoWeek } from '@/lib/journal';

interface WeekNavProps {
  /** Monday of the week being shown. */
  week: Date;
  onOpenWeek: (date: Date) => void;
}

/** Previous/next week navigation shown while a weekly review page is open. */
export function WeekNav({ week, onOpenWeek }: WeekNavProps) {
  const monday = startOfIsoWeek(week);
  const sunday = addDays(monday, 6);
  const isThisWeek = dateKey(startOfIsoWeek(new Date())) === dateKey(monday);
  return (
    <div style={{ display: 'inline-flex', alignItems: 'center', gap: 2 }}>
      <button type="button" onClick={() => onOpenWeek(addDays(monday, -7))} aria-label="Previous week" title="Previous week" style={iconButton}>
        <ChevronLeft size={14} />
      </button>
      <span style={{ ...iconButton, gap: 6, padding: '4px 8px', color: C.ink2, fontWeight: 600, cursor: 'default' }}>
        <CalendarRange size={13} color={C.accent} />
        Week {isoWeek(monday).week} · {formatDate(monday, 'MMM D')} – {formatDate(sunday, monday.getMonth() === sunday.getMonth() ? 'D' : 'MMM D')}
      </span>
      <button type="button" onClick={() => onOpenWeek(addDays(monday, 7))} aria-label="Next week" title="Next week" style={iconButton}>
        <ChevronRight size={14} />
      </button>
      {!isThisWeek && (
        <button type="button" onClick={() => onOpenWeek(new Date())} style={{ ...iconButton, padding: '4px 8px', color: C.accent, fontWeight: 650 }}>
          This week
        </button>
      )}
    </div>
  );
}

const iconButton: React.CSSProperties = {
  display: 'inline-flex', alignItems: 'center', justifyContent: 'center', padding: 5,
  border: 'none', borderRadius: 7, background: 'transparent', color: C.muted, cursor: 'pointer', fontSize: 12.5,
};
