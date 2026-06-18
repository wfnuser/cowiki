import { useState, useEffect } from 'react';
import { C } from '@/lib/design';
import type { AgentEvent, StreamStatus } from '@/hooks/useCompileStream';
import { AgentLogView } from '@/components/AgentLogView';
import { AgentChatView } from '@/components/AgentChatView';

// ── Props ────────────────────────────────────────────────────────

interface AgentDrawerProps {
  open: boolean;
  onClose: () => void;
  events: AgentEvent[];
  isCompiling: boolean;
  status: StreamStatus;
  isPinned: boolean;
  onTogglePin: () => void;
  countdown: number;
  isReconnecting: boolean;
}

// ── Constants ────────────────────────────────────────────────────

const DRAWER_WIDTH = 400;

// ── Helpers ──────────────────────────────────────────────────────

function getStatusText(status: StreamStatus): string {
  switch (status) {
    case 'streaming': return 'COMPILING…';
    case 'done': return 'DONE';
    case 'error': return 'ERROR';
    default: return '';
  }
}

function getStatusColor(status: StreamStatus): string {
  switch (status) {
    case 'streaming': return C.accent;
    case 'done': return C.green;
    case 'error': return C.red;
    default: return C.muted;
  }
}

// ── Component ────────────────────────────────────────────────────

export function AgentDrawer({
  open,
  onClose,
  events,
  isCompiling,
  status,
  isPinned,
  onTogglePin,
  countdown,
  isReconnecting,
}: AgentDrawerProps) {
  const [mode, setMode] = useState<'stream' | 'log'>('log');

  useEffect(() => {
    // Auto-switch to stream (chat) view when compilation starts
    if (isCompiling) {
      setMode('stream');
    }
  }, [isCompiling]);

  return (
    <div
      style={{
        width: open ? DRAWER_WIDTH : 0,
        minWidth: open ? DRAWER_WIDTH : 0,
        maxWidth: DRAWER_WIDTH,
        height: '100%',
        background: C.panel,
        borderLeft: open ? `1px solid ${C.line}` : 'none',
        boxShadow: open ? '-8px 0 30px rgba(0,0,0,0.08)' : 'none',
        display: 'flex',
        flexDirection: 'column',
        overflow: 'hidden',
        transition: 'width 300ms ease, min-width 300ms ease, border 300ms ease, box-shadow 300ms ease',
        flexShrink: 0,
      }}
    >
      {/* ── Header ── */}
      <div style={headerStyle}>
        <span
          style={{
            fontFamily: 'var(--font-sans)',
            fontSize: 12,
            fontWeight: 500,
            color: getStatusColor(status),
            letterSpacing: '0.04em',
            textTransform: ('uppercase' as const),
            whiteSpace: 'nowrap',
          }}
        >
          {getStatusText(status)}
        </span>

        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          {/* Pin button */}
          {(isCompiling || countdown > 0 || isPinned) && (
            <button
              onClick={onTogglePin}
              style={{
                ...pinBtnStyle,
                color: isPinned ? C.green : C.accent,
              }}
              title={isPinned ? 'Unpin drawer' : 'Pin drawer open'}
            >
              {isPinned
                ? '📌 Pinned'
                : countdown > 0
                  ? `📌 ${countdown}s`
                  : '📌 Pin'}
            </button>
          )}

          {/* Close button */}
          <button
            onClick={onClose}
            style={closeBtnStyle}
            title="Close drawer"
            aria-label="Close agent drawer"
          >
            ✕
          </button>
        </div>
      </div>

      {/* ── Body — mode switching ── */}
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden', minWidth: DRAWER_WIDTH }}>
        {mode === 'stream' ? (
          <AgentChatView events={events} isCompiling={isCompiling} />
        ) : (
          <AgentLogView
            events={events}
            isCompiling={isCompiling}
            status={status}
            isReconnecting={isReconnecting}
          />
        )}
      </div>
    </div>
  );
}

// ── Styles ───────────────────────────────────────────────────────

const headerStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  padding: '16px 20px',
  borderBottom: `1px solid ${C.lineSoft}`,
  flexShrink: 0,
  minWidth: DRAWER_WIDTH,
};

const pinBtnStyle: React.CSSProperties = {
  background: 'none',
  border: 'none',
  cursor: 'pointer',
  fontSize: 12,
  fontWeight: 500,
  fontFamily: 'var(--font-sans)',
  padding: '2px 6px',
  borderRadius: 4,
  transition: 'color 150ms ease',
  whiteSpace: 'nowrap',
};

const closeBtnStyle: React.CSSProperties = {
  background: 'none',
  border: 'none',
  cursor: 'pointer',
  fontSize: 14,
  color: C.muted,
  padding: '2px 4px',
  borderRadius: 4,
  lineHeight: 1,
  transition: 'color 150ms ease',
};
