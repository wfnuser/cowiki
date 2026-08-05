import { AlertCircle, CheckCircle2, Info, TriangleAlert } from 'lucide-react';
import type { ReactNode } from 'react';

import { cn } from '@/lib/utils';

type FeedbackTone = 'error' | 'warning' | 'success' | 'neutral';

const toneStyles: Record<FeedbackTone, string> = {
  error: 'border-red/20 bg-red-soft text-red',
  warning: 'border-amber/20 bg-amber-soft text-amber',
  success: 'border-green/20 bg-green-soft text-green',
  neutral: 'border-border bg-secondary text-text-secondary',
};

const toneIcons = {
  error: AlertCircle,
  warning: TriangleAlert,
  success: CheckCircle2,
  neutral: Info,
};

export function InlineFeedback({
  title,
  description,
  details,
  action,
  tone = 'error',
  compact = false,
  className,
}: {
  title: string;
  description?: ReactNode;
  details?: string;
  action?: ReactNode;
  tone?: FeedbackTone;
  compact?: boolean;
  className?: string;
}) {
  const Icon = toneIcons[tone];
  return (
    <div
      role={tone === 'error' ? 'alert' : 'status'}
      className={cn(
        'rounded-lg border text-left',
        compact ? 'px-2.5 py-2 text-xs' : 'px-3 py-2.5 text-sm',
        toneStyles[tone],
        className,
      )}
    >
      <div className="flex items-start gap-2">
        <Icon className="mt-0.5 size-4 shrink-0" />
        <div className="min-w-0 flex-1">
          <div className="font-medium leading-5">{title}</div>
          {description && <p className="mt-0.5 text-current/80">{description}</p>}
          {details && (
            <details className="mt-1.5 text-[11px] text-current/70">
              <summary className="cursor-pointer select-none">Technical details</summary>
              <pre className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap break-words font-mono">{details}</pre>
            </details>
          )}
        </div>
        {action && <div className="shrink-0">{action}</div>}
      </div>
    </div>
  );
}
