import { useState, useRef, useEffect, useCallback } from 'react';
import { C } from '@/lib/design';
import type { AgentEvent } from '@/hooks/useCompileStream';

// ── Types ───────────────────────────────────────────────────────

export interface ChatMessage {
  id: string;
  role: 'agent' | 'user';
  text: string;
  time: string;
}

interface AgentChatViewProps {
  events: AgentEvent[];
  isCompiling: boolean;
}

// ── Helpers ──────────────────────────────────────────────────────

function formatTime(): string {
  return new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatToolName(tool: string): string {
  return tool
    .replace(/^(cowiki_|wiki_)/, '')
    .replace(/_/g, ' ');
}

let msgCounter = 0;
function nextId(): string {
  return `msg-${Date.now()}-${++msgCounter}`;
}

// ── Component ────────────────────────────────────────────────────

export function AgentChatView({ events, isCompiling }: AgentChatViewProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [inputValue, setInputValue] = useState('');
  const scrollRef = useRef<HTMLDivElement>(null);

  // Derive conversation messages from SSE events
  useEffect(() => {
    const newMessages: ChatMessage[] = [];
    for (const event of events) {
      switch (event.type) {
        case 'TaskStarted':
          newMessages.push({
            id: nextId(),
            role: 'agent',
            text: '开始编译任务…',
            time: formatTime(),
          });
          break;
        case 'ToolStart':
          // Tool starts are rendered as system cards, not chat bubbles
          break;
        case 'ToolEnd':
          if (event.summary) {
            newMessages.push({
              id: nextId(),
              role: 'agent',
              text: event.tool
                ? `${formatToolName(event.tool)}：${event.summary}`
                : event.summary,
              time: formatTime(),
            });
          }
          break;
        case 'TaskCompleted':
          newMessages.push({
            id: nextId(),
            role: 'agent',
            text: `编译完成，共 ${event.rounds ?? '?'} 轮。需要我做任何调整吗？`,
            time: formatTime(),
          });
          break;
        case 'AgentStopped':
          if (event.reason && event.reason !== 'finished') {
            newMessages.push({
              id: nextId(),
              role: 'agent',
              text: `Agent 已停止：${event.reason}`,
              time: formatTime(),
            });
          }
          break;
      }
    }
    // Only update if we have new messages
    if (newMessages.length > 0) {
      setMessages(newMessages);
    }
  }, [events]);

  // Auto-scroll
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, isCompiling]);

  const handleSend = useCallback(() => {
    const text = inputValue.trim();
    if (!text) return;

    const userMsg: ChatMessage = {
      id: nextId(),
      role: 'user',
      text,
      time: formatTime(),
    };
    setMessages((prev) => [...prev, userMsg]);
    setInputValue('');

    // TODO: send to backend stream agent endpoint
  }, [inputValue]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  // Build tool-call system cards from events
  const toolCards = events
    .filter((e) => e.type === 'ToolEnd' && e.tool && e.success !== false)
    .map((e, i) => ({
      id: `tool-${i}`,
      tool: e.tool!,
      summary: e.summary ?? '',
      success: e.success !== false,
    }));

  // Active tool (last ToolStart without ToolEnd)
  const activeTool = (() => {
    for (let i = events.length - 1; i >= 0; i--) {
      if (events[i].type === 'ToolStart') {
        const tn = events[i].tool;
        let ended = false;
        for (let j = i + 1; j < events.length; j++) {
          if (events[j].type === 'ToolEnd' && events[j].tool === tn) {
            ended = true;
            break;
          }
        }
        if (!ended) return events[i].tool;
      }
    }
    return null;
  })();

  return (
    <>
      {/* Messages */}
      <div
        ref={scrollRef}
        style={{
          flex: 1,
          overflow: 'auto',
          padding: '16px 20px',
          display: 'flex',
          flexDirection: 'column',
          gap: 10,
        }}
      >
        {/* System cards for tool calls */}
        {toolCards.map((tc) => (
          <div key={tc.id} style={systemCardStyle}>
            <span style={{ fontSize: 13, flexShrink: 0 }}>✓</span>
            <span style={{ fontFamily: 'var(--font-mono)', fontSize: 11, color: C.ink }}>
              {formatToolName(tc.tool)}
            </span>
            {tc.summary && (
              <span style={{ color: C.muted, fontSize: 11 }}>· {tc.summary}</span>
            )}
          </div>
        ))}

        {/* Active tool indicator */}
        {activeTool && isCompiling && (
          <div style={{ ...systemCardStyle, borderColor: C.accent }}>
            <span style={{ fontSize: 13, flexShrink: 0 }}>🔧</span>
            <span style={{ fontFamily: 'var(--font-mono)', fontSize: 11, color: C.accent }}>
              {formatToolName(activeTool)}
            </span>
            <span style={{ color: C.faint, fontSize: 11 }}>…</span>
          </div>
        )}

        {/* Chat bubbles */}
        {messages.map((msg) =>
          msg.role === 'agent' ? (
            <div key={msg.id} style={agentMsgGroupStyle}>
              <div style={senderStyle}>
                🤖 compiler
                <span style={timeLabelStyle}>{msg.time}</span>
              </div>
              <div style={agentBubbleStyle}>{msg.text}</div>
            </div>
          ) : (
            <div key={msg.id} style={userMsgGroupStyle}>
              <div style={{ ...senderStyle, justifyContent: 'flex-end' }}>
                <span style={timeLabelStyle}>{msg.time}</span>
                你
              </div>
              <div style={userBubbleStyle}>{msg.text}</div>
            </div>
          ),
        )}

        {/* Empty state */}
        {events.length === 0 && !isCompiling && (
          <div style={emptyChatStyle}>等待 Agent 连接…</div>
        )}

        {/* Typing indicator */}
        {isCompiling && events.length > 0 && (
          <div style={typingContainerStyle}>
            <span style={{ ...typingDotStyle, animationDelay: '0s' }} />
            <span style={{ ...typingDotStyle, animationDelay: '0.2s' }} />
            <span style={{ ...typingDotStyle, animationDelay: '0.4s' }} />
          </div>
        )}
      </div>

      {/* Input area — skeleton, backend endpoint TBD */}
      <div style={inputAreaStyle}>
        <input
          type="text"
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="输入消息…"
          style={inputStyle}
        />
        <button
          onClick={handleSend}
          style={sendBtnStyle}
          disabled={!inputValue.trim()}
          aria-label="发送"
        >
          →
        </button>
      </div>
    </>
  );
}

// ── Styles ───────────────────────────────────────────────────────

const agentMsgGroupStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  alignSelf: 'flex-start',
  maxWidth: '88%',
};

const userMsgGroupStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  alignSelf: 'flex-end',
  maxWidth: '88%',
};

const senderStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 6,
  fontSize: 12,
  color: C.muted,
  fontWeight: 500,
  marginBottom: 3,
  padding: '0 4px',
};

const timeLabelStyle: React.CSSProperties = {
  fontWeight: 400,
  color: C.faint,
  fontSize: 11,
};

const agentBubbleStyle: React.CSSProperties = {
  fontSize: 13,
  lineHeight: 1.6,
  padding: '10px 14px',
  borderRadius: 14,
  borderTopLeftRadius: 4,
  background: C.sidebar,
  border: `1px solid ${C.lineSoft}`,
  color: C.ink2,
};

const userBubbleStyle: React.CSSProperties = {
  fontSize: 13,
  lineHeight: 1.6,
  padding: '10px 14px',
  borderRadius: 14,
  borderTopRightRadius: 4,
  background: C.accent,
  color: '#fff',
};

const systemCardStyle: React.CSSProperties = {
  alignSelf: 'center',
  maxWidth: 320,
  background: C.sidebar,
  border: `1px solid ${C.line}`,
  borderRadius: 10,
  padding: '8px 14px',
  display: 'flex',
  alignItems: 'center',
  gap: 8,
  fontSize: 12,
  color: C.ink2,
};

const typingContainerStyle: React.CSSProperties = {
  alignSelf: 'flex-start',
  display: 'flex',
  alignItems: 'center',
  gap: 4,
  padding: '10px 14px',
};

const typingDotStyle: React.CSSProperties = {
  width: 6,
  height: 6,
  borderRadius: '50%',
  background: C.muted,
  animation: 'agentBounce 1.4s ease-in-out infinite',
};

const inputAreaStyle: React.CSSProperties = {
  padding: '14px 20px',
  borderTop: `1px solid ${C.line}`,
  display: 'flex',
  gap: 10,
  alignItems: 'center',
  flexShrink: 0,
};

const inputStyle: React.CSSProperties = {
  flex: 1,
  padding: '10px 14px',
  border: `1px solid ${C.line}`,
  borderRadius: 12,
  fontSize: 13,
  fontFamily: 'var(--font-sans)',
  color: C.ink,
  background: C.sidebar,
  outline: 'none',
};

const sendBtnStyle: React.CSSProperties = {
  width: 36,
  height: 36,
  borderRadius: '50%',
  border: 'none',
  background: C.accent,
  color: '#fff',
  fontSize: 16,
  cursor: 'pointer',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  flexShrink: 0,
  opacity: 1,
};

const emptyChatStyle: React.CSSProperties = {
  padding: '24px 0',
  textAlign: 'center',
  color: C.muted,
  fontSize: 13,
  fontFamily: 'var(--font-sans)',
};
