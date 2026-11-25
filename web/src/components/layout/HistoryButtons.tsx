import { ArrowLeft, ArrowRight } from 'lucide-react';
import { useI18n } from '@/lib/I18nContext';

interface HistoryButtonsProps {
  canGoBack: boolean;
  canGoForward: boolean;
  onBack: () => void;
  onForward: () => void;
}

/** Back and forward at the top bar's left, as in Notion; faint while there is nowhere to go. */
export function HistoryButtons({ canGoBack, canGoForward, onBack, onForward }: HistoryButtonsProps) {
  const { t } = useI18n();
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 2, marginRight: 6, flexShrink: 0 }}>
      <HistoryButton label={t('Back')} shortcut="⌘[" disabled={!canGoBack} onClick={onBack}>
        <ArrowLeft size={16} />
      </HistoryButton>
      <HistoryButton label={t('Forward')} shortcut="⌘]" disabled={!canGoForward} onClick={onForward}>
        <ArrowRight size={16} />
      </HistoryButton>
    </span>
  );
}

function HistoryButton({ label, shortcut, disabled, onClick, children }: {
  label: string;
  shortcut: string;
  disabled: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={`${label} (${shortcut})`}
      disabled={disabled}
      onClick={onClick}
      // A class, not mouse handlers: a button that turns disabled under the
      // pointer gets no mouseleave and would keep its hover.
      className="inline-flex size-7 shrink-0 items-center justify-center rounded-md border-0 bg-transparent p-0 text-text-faint enabled:cursor-pointer enabled:hover:bg-bg-hover enabled:hover:text-text-secondary disabled:opacity-45"
    >
      {children}
    </button>
  );
}
