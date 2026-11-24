import { ArrowLeft } from 'lucide-react';

/** "Back to the review list": a quiet text control above the view title. */
export function ReviewBackButton({ onClick, children }: { onClick: () => void; children?: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="-ml-1.5 inline-flex h-7 items-center gap-1.5 rounded-md px-1.5 text-small text-text-tertiary transition-colors hover:bg-bg-hover hover:text-text"
    >
      <ArrowLeft size={14} /> {children ?? 'Reviews'}
    </button>
  );
}
