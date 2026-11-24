import { Minus, Plus } from "lucide-react"

import { cn } from "@/lib/utils"

/** A small whole-number field with − and + steps, in place of the
 * platform's number spinner (DESIGN_SYSTEM.md, "Form controls"). */
function NumberField({
  value,
  onChange,
  label,
  min = 0,
  max = Number.MAX_SAFE_INTEGER,
  step = 1,
  className,
}: {
  value: number
  onChange: (value: number) => void
  /** Accessible name; also names the − / + buttons. */
  label: string
  min?: number
  max?: number
  step?: number
  className?: string
}) {
  const clamp = (next: number) => Math.min(max, Math.max(min, next))
  const stepButton = "flex h-full w-6 shrink-0 items-center justify-center text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text disabled:pointer-events-none disabled:opacity-40 [&_svg]:size-3"
  return (
    <span
      data-slot="number-field"
      className={cn(
        "inline-flex h-7 items-stretch overflow-hidden rounded-lg border border-border bg-panel transition-colors focus-within:border-accent/60 focus-within:shadow-[var(--shadow-focus)]",
        className
      )}
    >
      <button type="button" className={stepButton} aria-label={`Decrease ${label}`} disabled={value <= min} onClick={() => onChange(clamp(value - step))}>
        <Minus />
      </button>
      <input
        type="number"
        inputMode="numeric"
        aria-label={label}
        className="w-10 min-w-0 bg-transparent text-center text-small tabular-nums text-text outline-none focus-visible:outline-none"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => {
          const parsed = Number.parseInt(event.target.value, 10)
          if (Number.isFinite(parsed)) onChange(clamp(parsed))
        }}
      />
      <button type="button" className={stepButton} aria-label={`Increase ${label}`} disabled={value >= max} onClick={() => onChange(clamp(value + step))}>
        <Plus />
      </button>
    </span>
  )
}

export { NumberField }
