import { ChevronRight } from 'lucide-react';

import { InlineFeedback } from '@/components/ui/inline-feedback';
import { C } from '@/lib/design';

export function ReviewInbox({
  action,
  children,
  description,
  empty,
  error,
  loading,
}: {
  action?: React.ReactNode;
  children?: React.ReactNode;
  description: string;
  empty?: string;
  error?: string | null;
  loading?: boolean;
}) {
  return (
    <div>
      <div style={{
        display: 'flex',
        alignItems: 'flex-start',
        justifyContent: 'space-between',
        gap: 16,
        marginBottom: 20,
      }}>
        <div>
          <h1 className="page-title" style={{ marginBottom: 5 }}>Reviews</h1>
          <p style={{ color: C.muted, fontSize: 13, margin: 0 }}>{description}</p>
        </div>
        {action}
      </div>

      {error && <InlineFeedback className="mb-4" title="Could not load Reviews" description={error} />}
      {loading ? (
        <p style={{ color: C.muted, fontSize: 14 }}>Loading Reviews…</p>
      ) : children ? (
        <div style={{
          border: `1px solid ${C.line}`,
          borderRadius: 12,
          overflow: 'hidden',
          background: C.panel,
        }}>
          {children}
        </div>
      ) : (
        <div style={{
          padding: 32,
          textAlign: 'center',
          color: C.muted,
          fontSize: 14,
          border: `1px solid ${C.line}`,
          borderRadius: 10,
          background: C.panel,
        }}>
          {empty ?? 'No reviews yet.'}
        </div>
      )}
    </div>
  );
}

export function ReviewInboxRow({
  additions,
  deletions,
  files,
  first,
  icon,
  onOpen,
  status,
  statusTone = 'muted',
  subtitle,
  title,
  trailing,
}: {
  additions?: number;
  deletions?: number;
  files?: number;
  first: boolean;
  icon: React.ReactNode;
  onOpen: () => void;
  status: string;
  statusTone?: 'muted' | 'danger';
  subtitle: string;
  title: string;
  trailing?: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onOpen}
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        width: '100%',
        minWidth: 0,
        padding: '15px 17px',
        border: 0,
        borderTop: first ? 'none' : `1px solid ${C.line}`,
        background: C.panel,
        textAlign: 'left',
        cursor: 'pointer',
      }}
      onMouseEnter={(event) => { event.currentTarget.style.background = C.sidebar; }}
      onMouseLeave={(event) => { event.currentTarget.style.background = C.panel; }}
    >
      {icon}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{
          fontSize: 14.5,
          fontWeight: 650,
          color: C.ink,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
        }}>
          {title}
        </div>
        <div style={{
          marginTop: 3,
          fontSize: 12.5,
          color: C.muted,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
        }}>
          {subtitle}
        </div>
      </div>
      {files != null && additions != null && deletions != null && (
        <span style={{ color: C.muted, fontSize: 12, whiteSpace: 'nowrap' }}>
          {files} file{files === 1 ? '' : 's'} ·{' '}
          <span style={{ color: C.green }}>+{additions}</span> ·{' '}
          <span style={{ color: C.red }}>−{deletions}</span>
        </span>
      )}
      <span style={{
        fontSize: 11.5,
        fontWeight: 650,
        color: statusTone === 'danger' ? C.red : C.muted,
        whiteSpace: 'nowrap',
      }}>
        {status}
      </span>
      {trailing}
      <ChevronRight size={15} color={C.faint} />
    </button>
  );
}
