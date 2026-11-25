import { useState } from 'react';
import { CalendarDays } from 'lucide-react';
import { Popover as PopoverPrimitive } from 'radix-ui';

import { MonthCalendar } from '@/components/ui/date-picker';

function monthOf(key: string): Date {
  const [year, month] = key.split('-').map(Number);
  return new Date(year!, month! - 1, 1);
}

/**
 * Go to a day in Journals, as Obsidian's Calendar does: a month with a dot
 * on every day that has a note. Days without a note (other than today)
 * cannot be picked; opening Journals never creates a file.
 */
export function JournalCalendar({ days, today, onGoTo }: {
  /** `YYYY-MM-DD` of every day with a journal note. */
  days: ReadonlySet<string>;
  today: string;
  onGoTo: (key: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [month, setMonth] = useState(() => monthOf(today));
  return (
    <PopoverPrimitive.Root open={open} onOpenChange={(next) => {
      setOpen(next);
      if (next) setMonth(monthOf(today));
    }}>
      <PopoverPrimitive.Trigger asChild>
        <button
          type="button"
          aria-label="Go to a day"
          title="Go to a day"
          className="flex size-7 items-center justify-center rounded-md text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text"
        >
          <CalendarDays size={16} />
        </button>
      </PopoverPrimitive.Trigger>
      <PopoverPrimitive.Portal>
        <PopoverPrimitive.Content
          align="end"
          sideOffset={6}
          aria-label="Journal calendar"
          className="z-[200] w-60 rounded-lg border border-border bg-panel p-2 text-text shadow-[var(--shadow-popover)]"
        >
          <MonthCalendar
            month={month}
            onMonthChange={setMonth}
            today={today}
            marked={days}
            enabled={(key) => key === today || days.has(key)}
            onChoose={(key) => {
              onGoTo(key);
              setOpen(false);
            }}
          />
        </PopoverPrimitive.Content>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  );
}
