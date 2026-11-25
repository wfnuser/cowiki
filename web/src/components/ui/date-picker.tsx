import { CalendarDays, ChevronLeft, ChevronRight, X } from "lucide-react"
import { Popover as PopoverPrimitive } from "radix-ui"
import { useState } from "react"

import { addDays, dateKey, startOfIsoWeek } from "@/lib/journal"
import { cn } from "@/lib/utils"

function parse(key: string): Date {
  const [year, month, day] = key.split("-").map(Number)
  return new Date(year!, month! - 1, day!)
}

/** "Today", "Tomorrow", "Mon 24 Nov": short, never the platform's format. */
export function dayLabel(key: string, today: string): string {
  if (key === today) return "Today"
  if (key === dateKey(addDays(parse(today), 1))) return "Tomorrow"
  const date = parse(key)
  const sameYear = date.getFullYear() === parse(today).getFullYear()
  return date.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short", ...(sameYear ? {} : { year: "numeric" }) })
}

/** A month of days, Monday first: the date picker's grid and the Journals
 * calendar. `marked` days get a dot; `enabled` (when given) limits which
 * days can be chosen. */
export function MonthCalendar({
  month,
  onMonthChange,
  today,
  onChoose,
  selected = "",
  marked,
  enabled,
}: {
  /** The first day of the month shown. */
  month: Date
  onMonthChange: (month: Date) => void
  today: string
  onChoose: (key: string) => void
  selected?: string
  marked?: ReadonlySet<string>
  enabled?: (key: string) => boolean
}) {
  const gridStart = startOfIsoWeek(month)
  const days = Array.from({ length: 42 }, (_, index) => addDays(gridStart, index))
  return (
    <>
      <div className="flex items-center justify-between px-1 pb-1">
        <span className="text-small font-medium">{month.toLocaleDateString(undefined, { month: "long", year: "numeric" })}</span>
        <span className="flex">
          <button type="button" aria-label="Previous month" onClick={() => onMonthChange(new Date(month.getFullYear(), month.getMonth() - 1, 1))} className="flex size-6 items-center justify-center rounded-md text-text-tertiary hover:bg-bg-hover hover:text-text">
            <ChevronLeft className="size-3.5" />
          </button>
          <button type="button" aria-label="Next month" onClick={() => onMonthChange(new Date(month.getFullYear(), month.getMonth() + 1, 1))} className="flex size-6 items-center justify-center rounded-md text-text-tertiary hover:bg-bg-hover hover:text-text">
            <ChevronRight className="size-3.5" />
          </button>
        </span>
      </div>
      <div className="grid grid-cols-7 text-center text-caption text-text-faint">
        {["M", "T", "W", "T", "F", "S", "S"].map((day, index) => <span key={index} className="py-1">{day}</span>)}
      </div>
      <div className="grid grid-cols-7 gap-y-0.5">
        {days.map((date) => {
          const key = dateKey(date)
          const outside = date.getMonth() !== month.getMonth()
          const isSelected = key === selected
          const allowed = enabled ? enabled(key) : true
          return (
            <button
              key={key}
              type="button"
              aria-label={date.toDateString()}
              aria-pressed={isSelected}
              disabled={!allowed}
              onClick={() => onChoose(key)}
              className={cn(
                "relative mx-auto flex size-7 items-center justify-center rounded-md text-meta tabular-nums transition-colors",
                isSelected ? "bg-accent font-medium text-on-accent" : "enabled:hover:bg-bg-hover",
                !isSelected && (outside || !allowed) && "text-text-faint",
                !isSelected && key === today && "font-semibold text-accent",
                !allowed && "cursor-default"
              )}
            >
              {date.getDate()}
              {marked?.has(key) && !isSelected && (
                <span aria-hidden className="absolute bottom-0.5 left-1/2 size-1 -translate-x-1/2 rounded-full bg-text-tertiary" />
              )}
            </button>
          )
        })}
      </div>
    </>
  )
}

/** A quiet date chip with quick choices and a month grid, in place of the
 * platform's date input (DESIGN_SYSTEM.md, "Form controls"). */
function DatePicker({
  value,
  onChange,
  label = "Date",
  className,
}: {
  /** `YYYY-MM-DD`, or "" for no date. */
  value: string
  onChange: (value: string) => void
  label?: string
  className?: string
}) {
  const today = dateKey(new Date())
  const [open, setOpen] = useState(false)
  const [month, setMonth] = useState(() => {
    const base = parse(value || today)
    return new Date(base.getFullYear(), base.getMonth(), 1)
  })
  const choose = (key: string) => {
    onChange(key)
    setOpen(false)
  }
  const quick: Array<[string, string]> = [
    ["Today", today],
    ["Tomorrow", dateKey(addDays(parse(today), 1))],
    ["Next week", dateKey(addDays(startOfIsoWeek(parse(today)), 7))],
  ]
  return (
    <PopoverPrimitive.Root open={open} onOpenChange={(next) => {
      setOpen(next)
      if (next) {
        const base = parse(value || today)
        setMonth(new Date(base.getFullYear(), base.getMonth(), 1))
      }
    }}>
      <PopoverPrimitive.Trigger asChild>
        <button
          type="button"
          aria-label={value ? `${label}: ${dayLabel(value, today)}` : label}
          className={cn(
            "inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-meta transition-colors hover:bg-bg-hover",
            value ? "text-text-secondary" : "text-text-tertiary",
            className
          )}
        >
          <CalendarDays className="size-3.5" aria-hidden />
          {value ? dayLabel(value, today) : label}
        </button>
      </PopoverPrimitive.Trigger>
      <PopoverPrimitive.Portal>
        <PopoverPrimitive.Content
          align="end"
          sideOffset={6}
          className="z-[200] w-60 rounded-lg border border-border bg-panel p-2 text-text shadow-[var(--shadow-popover)] data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95"
        >
          <div className="flex flex-col">
            {quick.map(([name, key]) => (
              <button key={name} type="button" onClick={() => choose(key)} className="flex items-center justify-between rounded-md px-2 py-1.5 text-small hover:bg-bg-hover">
                {name}
                <span className="text-meta text-text-tertiary">{dayLabel(key, "0000-01-01").replace(/,? \d{4}$/, "")}</span>
              </button>
            ))}
            {value && (
              <button type="button" onClick={() => choose("")} className="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-small text-text-secondary hover:bg-bg-hover">
                <X className="size-3.5" aria-hidden /> No date
              </button>
            )}
          </div>
          <div className="my-2 h-px bg-border-soft" />
          <MonthCalendar month={month} onMonthChange={setMonth} selected={value} today={today} onChoose={choose} />
        </PopoverPrimitive.Content>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  )
}

export { DatePicker }
