import { useEffect, useState } from 'react';
import { Users, UserPlus, ChevronDown } from 'lucide-react';
import { listMembers, type MemberInfo } from '../../api';

/* ── Design tokens ── */
const C = {
  bg: '#faf9f7',
  panel: '#fdfcfb',
  sidebar: '#f5f4f1',
  rail: '#efedea',
  ink: '#1d1c1a',
  ink2: '#403e3a',
  muted: '#8c897f',
  faint: '#a8a59b',
  line: '#e8e6e1',
  accent: '#e2590b',
} as const;

interface MembersViewProps {
  workspaceSlug: string;
}

export function MembersView({ workspaceSlug }: MembersViewProps) {
  const [members, setMembers] = useState<MemberInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setMembers(null);
    setError(null);
    listMembers(workspaceSlug)
      .then((m) => !cancelled && setMembers(m))
      .catch((e) => !cancelled && setError(e.message || 'Failed to load members'));
    return () => { cancelled = true; };
  }, [workspaceSlug]);

  if (error) {
    return <p style={{ color: '#cf222e', fontSize: 14, padding: '16px 0' }}>Failed to load members: {error}</p>;
  }

  if (members == null) {
    return <p style={{ color: C.muted, fontSize: 14, padding: '16px 0' }}>Loading members...</p>;
  }

  const roleTextColors: Record<string, string> = {
    owner: C.accent,
    writer: C.ink2,
    editor: C.ink2,
    reader: C.muted,
    viewer: C.muted,
  };

  return (
    <div>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 20 }}>
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 10 }}>
          <h1 className="page-title" style={{ marginBottom: 0 }}>
            Members
          </h1>
          <span style={{ fontSize: 24, color: C.faint, fontWeight: 400 }}>{members.length}</span>
        </div>
        <button
          style={{
            display: 'flex', alignItems: 'center', gap: 7,
            padding: '8px 14px', borderRadius: 8, border: 'none',
            background: C.accent, color: '#fff', fontSize: 13.5, fontWeight: 600,
            cursor: 'pointer',
          }}
        >
          <UserPlus size={15} /> Invite people
        </button>
      </div>

      {/* Members table */}
      <div style={{ border: `1px solid ${C.line}`, borderRadius: 12, overflow: 'hidden', background: C.panel }}>
        {/* Table header */}
        <div style={{
          display: 'grid', gridTemplateColumns: '1fr 150px 130px 40px',
          gap: 12, padding: '8px 16px', background: C.sidebar,
          borderBottom: `1px solid ${C.line}`, fontSize: 12, fontWeight: 600,
          color: C.faint, textTransform: 'uppercase', letterSpacing: '0.05em',
        }}>
          <span>Member</span>
          <span>Role</span>
          <span>Last active</span>
          <span></span>
        </div>

        {/* Table rows */}
        {members.length === 0 ? (
          <div style={{ padding: 24, textAlign: 'center', color: C.muted, fontSize: 13 }}>
            <Users size={20} style={{ margin: '0 auto 8px', display: 'block' }} />
            No members found.
          </div>
        ) : (
          members.map((m, i) => {
            const roleColor = roleTextColors[m.role] ?? C.muted;
            return (
              <div
                key={m.id}
                style={{
                  display: 'grid', gridTemplateColumns: '1fr 150px 130px 40px',
                  gap: 12, padding: '10px 16px', alignItems: 'center',
                  borderBottom: i < members.length - 1 ? `1px solid ${C.line}` : 'none',
                  background: C.panel,
                }}
              >
                {/* Avatar + name */}
                <div style={{ display: 'flex', alignItems: 'center', gap: 10, minWidth: 0 }}>
                  <div style={{
                    width: 30, height: 30, borderRadius: '50%', background: C.rail,
                    display: 'flex', alignItems: 'center', justifyContent: 'center',
                    fontSize: 12, fontWeight: 600, color: C.ink2, flexShrink: 0,
                  }}>
                    {m.name[0]?.toUpperCase() || '?'}
                  </div>
                  <div style={{ minWidth: 0 }}>
                    <div style={{ fontSize: 14.5, fontWeight: 550, color: C.ink, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {m.name}
                    </div>
                    <div style={{ fontSize: 12.5, color: C.muted, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {m.email || 'No email'}
                    </div>
                  </div>
                </div>

                {/* Role */}
                <span style={{
                  display: 'inline-flex', alignItems: 'center', gap: 4,
                  fontSize: 13, fontWeight: 500, color: roleColor,
                  textTransform: 'capitalize', cursor: 'default',
                }}>
                  {m.role}
                  <ChevronDown size={12} color={roleColor} />
                </span>

                {/* Last active */}
                <span style={{ fontSize: 12, color: C.faint }}>--</span>

                {/* Actions (placeholder) */}
                <button
                  disabled
                  style={{
                    background: 'none', border: 'none', fontSize: 12, color: C.faint,
                    cursor: 'not-allowed',
                  }}
                >
                  ...
                </button>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

export default MembersView;
