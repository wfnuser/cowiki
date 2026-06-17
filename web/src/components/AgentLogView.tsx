import { useEffect, useRef, useState } from 'react';
import { C } from '@/lib/design';
import type { AgentEvent, StreamStatus } from '@/hooks/useCompileStream';

// ── Props ────────────────────────────────────────────────────────

interface AgentLogViewProps {
  events: AgentEvent[];
  isCompiling: boolean;
  status: StreamStatus;
  isReconnecting: boolean;
}

// ── Helpers ──────────────────────────────────────────────────────

function formatToolName(tool: string): string {
  return tool
    .replace(/^(cowiki_|wiki_)/, '')
    .replace(/_/g, ' ');
}

function formatTime(): string {
  return new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

// ── Component ────────────────────────────────────────────────────

export function AgentLogView({
  events,
  isCompiling,
  status,
  isReconnecting,
}: AgentLogViewProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [hoveredRow, setHoveredRow] = useState<number | null>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [events]);

  // Find the currently active tool (last ToolStart without matching ToolEnd)
  const activeIndex = ((): number => {
    for (let i = events.length - 1; i >= 0; i--) {
      if (events[i].type === 'ToolStart') {
        const toolName = events[i].tool;
        let hasEnd = false;
        for (let j = i + 1; j < events.length; j++) {
          if (events[j].type === 'ToolEnd' && events[j].tool === toolName) {
            hasEnd = true;
            break;
          }
        }
        if (!hasEnd) return i;
      }
    }
    return -1;
  })();

  const getBarColor = (index: number, eventType: string): string => {
    if (index === activeIndex) return C.accent;
    if (eventType === 'ToolEnd' || eventType === 'TaskCompleted') return C.green;
    if (eventType === 'AgentStopped') return C.red;
    if (activeIndex >= 0 && index < activeIndex) return C.green;
    return C.lineSoft;
  };

  const completedTools = events.filter((e) => e.type === 'ToolEnd').length;
  const writtenPages = events
    .filter((e) => e.type === 'ToolEnd' && e.tool?.includes('write'))
    .length;

  return (
    <div
      ref={scrollRef}
      style={{
        flex: 1,
        overflow: 'auto',
        padding: '16px 20px',
        display: 'flex',
        flexDirection: 'column',
        gap: 4,
      }}
    >
      {/* Empty: connecting */}
      {events.length === 0 && isCompiling && !isReconnecting && (
        <div style={emptyStateStyle}>
          <span style={spinnerStyle} />
          Connecting to agent…
        </div>
      )}

      {/* Reconnecting */}
      {isReconnecting && (
        <div style={{ ...emptyStateStyle, color: C.amber }}>
          ⚠ Connection lost · reconnecting…
        </div>
      )}

      {/* Error: connection lost */}
      {status === 'error' && events.length === 0 && !isReconnecting && (
        <div style={{ ...emptyStateStyle, color: C.red }}>
          ✗ Failed to connect
        </div>
      )}

      {/* Empty: idle */}
      {events.length === 0 && !isCompiling && !isReconnecting && status === 'idle' && (
        <div style={emptyStateStyle}>No events yet</div>
      )}

      {/* Event log entries */}
      {events.map((event, i) => {
        const barColor = getBarColor(i, event.type);
        return (
          <LogEntry
            key={`${event.task_id ?? 'evt'}-${i}`}
            event={event}
            barColor={barColor}
            isHovered={hoveredRow === i}
            onHover={(h) => setHoveredRow(h ? i : null)}
          />
        );
      })}

      {/* Done summary */}
      {status === 'done' && events.length > 0 && (
        <div style={doneSummaryStyle}>
          <div style={{ ...barStyle, background: C.green }} />
          <span style={{ ...statusIconStyle, color: C.green }}>✔</span>
          <span style={{ fontWeight: 600, color: C.ink }}>
            Done
            {writtenPages > 0 && (
              <span style={{ fontWeight: 400, color: C.muted }}>
                {' '}· {writtenPages} page{writtenPages !== 1 ? 's' : ''}
              </span>
            )}
            {completedTools > 0 && (
              <span style={{ fontWeight: 400, color: C.muted }}>
                {', '}{completedTools} tool call{completedTools !== 1 ? 's' : ''}
              </span>
            )}
          </span>
        </div>
      )}
    </div>
  );
}

// ── Log entry ────────────────────────────────────────────────────

function LogEntry({
  event,
  barColor,
  isHovered,
  onHover,
}: {
  event: AgentEvent;
  barColor: string;
  isHovered: boolean;
  onHover: (hovered: boolean) => void;
}) {
  const [time] = useState(() => formatTime());

  switch (event.type) {
    case 'connected':
      return null;

    case 'TaskStarted':
      return (
        <div
          style={logRowStyle}
          onMouseEnter={() => onHover(true)}
          onMouseLeave={() => onHover(false)}
        >
          <div style={{ ...barStyle, background: barColor }} />
          <span style={statusIconStyle}>⬇</span>
          <span style={logBodyStyle}>Started</span>
          <span style={hoverTimeStyle(isHovered)}>{time}</span>
        </div>
      );

    case 'ToolStart':
      return (
        <div
          style={logRowStyle}
          onMouseEnter={() => onHover(true)}
          onMouseLeave={() => onHover(false)}
        >
          <div style={{ ...barStyle, background: barColor, transition: 'background 200ms ease' }} />
          <span style={statusIconStyle}>🔧</span>
          <span style={toolNameStyle}>
            {event.tool ? formatToolName(event.tool) : 'tool'}
          </span>
          <span style={hoverTimeStyle(isHovered)}>{time}</span>
        </div>
      );

    case 'ToolEnd':
      return (
        <div
          style={logRowStyle}
          onMouseEnter={() => onHover(true)}
          onMouseLeave={() => onHover(false)}
        >
          <div style={{ ...barStyle, background: barColor, transition: 'background 200ms ease' }} />
          <span style={statusIconStyle}>✓</span>
          <span style={toolNameStyle}>
            {event.tool ? formatToolName(event.tool) : 'tool'}
          </span>
          {event.summary && <span style={summaryStyle}> · {event.summary}</span>}
          <span style={hoverTimeStyle(isHovered)}>{time}</span>
        </div>
      );

    case 'TaskCompleted':
      return null;

    case 'AgentStopped':
      return (
        <div
          style={logRowStyle}
          onMouseEnter={() => onHover(true)}
          onMouseLeave={() => onHover(false)}
        >
          <div style={{ ...barStyle, background: C.red }} />
          <span style={{ ...statusIconStyle, color: C.red }}>✗</span>
          <span style={{ ...logBodyStyle, color: C.red }}>
            Agent stopped
            {event.reason && event.reason !== 'finished' && (
              <span style={{ ...summaryStyle, color: C.red }}> · {event.reason}</span>
            )}
          </span>
          <span style={hoverTimeStyle(isHovered)}>{time}</span>
        </div>
      );

    default:
      return null;
  }
}

// ── Styles ───────────────────────────────────────────────────────

const barStyle: React.CSSProperties = {
  width: 2,
  minWidth: 2,
  alignSelf: 'stretch',
  borderRadius: 1,
  marginRight: 8,
  flexShrink: 0,
};

const logRowStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'baseline',
  fontSize: 13,
  fontFamily: 'var(--font-sans)',
  lineHeight: '20px',
  padding: '2px 0',
  gap: 4,
  cursor: 'default',
};

const statusIconStyle: React.CSSProperties = {
  fontSize: 12,
  width: 18,
  textAlign: 'center' as const,
  flexShrink: 0,
  lineHeight: '20px',
};

const logBodyStyle: React.CSSProperties = {
  color: C.ink2,
  fontWeight: 400,
};

const toolNameStyle: React.CSSProperties = {
  color: C.ink,
  fontWeight: 500,
  whiteSpace: 'nowrap' as const,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
};

const summaryStyle: React.CSSProperties = {
  color: C.muted,
  fontWeight: 400,
  whiteSpace: 'nowrap' as const,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
};

const hoverTimeStyle = (visible: boolean): React.CSSProperties => ({
  color: C.faint,
  fontSize: 11,
  fontWeight: 400,
  marginLeft: 'auto',
  flexShrink: 0,
  opacity: visible ? 1 : 0,
  transition: 'opacity 150ms ease',
});

const emptyStateStyle: React.CSSProperties = {
  padding: '24px 0',
  textAlign: 'center' as const,
  color: C.muted,
  fontSize: 13,
  fontFamily: 'var(--font-sans)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  gap: 8,
};

const spinnerStyle: React.CSSProperties = {
  width: 14,
  height: 14,
  border: `2px solid ${C.line}`,
  borderTopColor: C.accent,
  borderRadius: '50%',
  display: 'inline-block',
  animation: 'spin 0.8s linear infinite',
};

const doneSummaryStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'baseline',
  fontSize: 13,
  fontFamily: 'var(--font-sans)',
  lineHeight: '20px',
  padding: '8px 0 2px',
  gap: 4,
  borderTop: `1px solid ${C.lineSoft}`,
  marginTop: 8,
  paddingTop: 12,
};
