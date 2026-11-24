import type { ReactNode } from "react"

import { cn } from "@/lib/utils"

/** The one title row every tool view uses (Tasks, History, Reviews, Members…).
 * Serif is for content people write; views get a quiet sans title
 * (DESIGN_SYSTEM.md, "View layout"). */
function ViewHeader({
  title,
  description,
  leading,
  actions,
  className,
}: {
  title: ReactNode
  /** One short line, only when it says something the title does not. */
  description?: ReactNode
  /** A back button or a small mark before the title. */
  leading?: ReactNode
  actions?: ReactNode
  className?: string
}) {
  return (
    <header className={cn("mb-6", className)}>
      <div className="flex min-h-8 items-center gap-2">
        {leading}
        <h1 className="view-title min-w-0 flex-1 truncate">{title}</h1>
        {actions && <div className="flex shrink-0 items-center gap-1">{actions}</div>}
      </div>
      {description && <p className="mt-1 text-[13px] leading-5 text-text-tertiary">{description}</p>}
    </header>
  )
}

/** A group heading inside a view: sentence case, never small caps. */
function ViewSectionHeading({ children, meta, className }: { children: ReactNode; meta?: ReactNode; className?: string }) {
  return (
    <div className={cn("mb-1.5 flex items-baseline justify-between gap-3", className)}>
      <h2 className="text-[13px] font-semibold leading-5 text-text">{children}</h2>
      {meta != null && <span className="text-[12px] tabular-nums text-text-tertiary">{meta}</span>}
    </div>
  )
}

/** Nothing to show: one short sentence, no box. */
function ViewEmpty({ children, className }: { children: ReactNode; className?: string }) {
  return <p className={cn("flex items-center justify-center gap-2 py-10 text-center text-[13px] text-text-tertiary", className)}>{children}</p>
}

/** A small icon-only action for a view header. */
function ViewIconButton({ label, onClick, disabled, children }: { label: string; onClick?: () => void; disabled?: boolean; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      title={label}
      className="flex size-8 items-center justify-center rounded-md text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text disabled:opacity-60"
    >
      {children}
    </button>
  )
}

/** Line tabs under a view title: the open tab is underlined in ink. */
function ViewTabs<T extends string>({
  label,
  tabs,
  value,
  onChange,
  className,
}: {
  label: string
  tabs: ReadonlyArray<{ id: T; label: string; count?: number }>
  value: T
  onChange: (id: T) => void
  className?: string
}) {
  return (
    <nav role="tablist" aria-label={label} className={cn("mb-4 flex gap-5 border-b border-border-soft", className)}>
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          role="tab"
          aria-selected={value === tab.id}
          onClick={() => onChange(tab.id)}
          className={cn(
            "-mb-px flex items-center gap-1.5 border-b-2 pb-2 pt-1 text-[13px] transition-colors",
            value === tab.id ? "border-text font-medium text-text" : "border-transparent text-text-tertiary hover:text-text-secondary"
          )}
        >
          {tab.label}
          {tab.count != null && tab.count > 0 && <span className="text-[11.5px] font-normal tabular-nums text-text-faint">{tab.count}</span>}
        </button>
      ))}
    </nav>
  )
}

export { ViewEmpty, ViewHeader, ViewIconButton, ViewSectionHeading, ViewTabs }
