import { ArrowLeft } from 'lucide-react';
import { C } from '@/lib/design';

/**
 * Shared "back to the review list" control. This is a text-style chrome
 * control, not a shadcn Button — wrapping Button forced padding/icon overrides
 * that still drifted from the original 12.5px / 14px-icon row.
 */
export function ReviewBackButton({ onClick, children }: { onClick: () => void; children?: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 6,
        padding: 0,
        border: 0,
        background: 'transparent',
        color: C.muted,
        fontSize: 12.5,
        cursor: 'pointer',
      }}
    >
      <ArrowLeft size={14} /> {children ?? 'Reviews'}
    </button>
  );
}
