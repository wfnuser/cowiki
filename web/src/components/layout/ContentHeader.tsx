import { APP_HEADER_HEIGHT, C } from '@/lib/design';

export function ContentHeader({ children }: { children: React.ReactNode }) {
  return (
    <div
      data-tauri-drag-region="deep"
      style={{
        position: 'sticky',
        top: 0,
        zIndex: 10,
        height: APP_HEADER_HEIGHT,
        minHeight: APP_HEADER_HEIGHT,
        padding: '0 24px',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 16,
        background: C.panel,
        borderBottom: `1px solid ${C.line}`,
      }}
    >
      {children}
    </div>
  );
}

export function ContentBreadcrumb({ children }: { children: React.ReactNode }) {
  return (
    <div
      className="app-breadcrumb"
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 6,
        minWidth: 0,
        overflow: 'hidden',
        color: C.muted,
        fontSize: 13,
      }}
    >
      {children}
    </div>
  );
}

export function ContentHeaderActions({ children }: { children: React.ReactNode }) {
  return (
    <div
      className="app-header-actions"
      style={{ display: 'flex', alignItems: 'center', gap: 4, flexShrink: 0 }}
    >
      {children}
    </div>
  );
}
