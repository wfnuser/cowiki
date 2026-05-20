import { Link, Outlet, useLocation } from 'react-router-dom';
import {
  BookOpen,
  FolderOpen,
  GitPullRequest,
  Search,
  ChevronRight,
} from 'lucide-react';

const navItems = [
  { to: '/', label: 'Shared Wiki', icon: BookOpen },
  { to: '/personal', label: 'My Space', icon: FolderOpen },
  { to: '/reviews', label: 'Reviews', icon: GitPullRequest },
  { to: '/search', label: 'Search', icon: Search },
];

export function Layout() {
  const location = useLocation();

  return (
    <div className="flex h-screen overflow-hidden">
      {/* Sidebar */}
      <aside className="w-60 shrink-0 border-r border-[var(--color-border)] bg-[var(--color-bg-secondary)] flex flex-col">
        {/* Logo */}
        <div className="px-4 py-3 flex items-center gap-2">
          <div className="w-5 h-5 rounded bg-[var(--color-text)] flex items-center justify-center">
            <span className="text-white text-xs font-bold">c</span>
          </div>
          <span className="font-semibold text-sm text-[var(--color-text)]">cowiki</span>
        </div>

        {/* Navigation */}
        <nav className="flex-1 px-2 py-1">
          <div className="mb-3">
            <span className="px-2 text-xs font-medium text-[var(--color-text-tertiary)] uppercase tracking-wider">
              Workspace
            </span>
          </div>
          {navItems.map((item) => {
            const active = location.pathname === item.to;
            const Icon = item.icon;
            return (
              <Link
                key={item.to}
                to={item.to}
                className={`flex items-center gap-2 px-2 py-1.5 rounded-md text-sm transition-colors mb-0.5 ${
                  active
                    ? 'bg-[var(--color-bg-active)] text-[var(--color-text)] font-medium'
                    : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)]'
                }`}
              >
                <Icon size={16} strokeWidth={active ? 2 : 1.5} />
                <span>{item.label}</span>
                {active && (
                  <ChevronRight size={14} className="ml-auto text-[var(--color-text-tertiary)]" />
                )}
              </Link>
            );
          })}
        </nav>

        {/* Footer */}
        <div className="px-4 py-3 border-t border-[var(--color-border)]">
          <div className="flex items-center gap-2">
            <div className="w-6 h-6 rounded-full bg-[var(--color-bg-active)] flex items-center justify-center">
              <span className="text-xs text-[var(--color-text-secondary)]">U</span>
            </div>
            <span className="text-xs text-[var(--color-text-secondary)]">default</span>
          </div>
        </div>
      </aside>

      {/* Main Content */}
      <main className="flex-1 overflow-y-auto bg-[var(--color-bg)]">
        <div className="max-w-3xl mx-auto px-16 py-10">
          <Outlet />
        </div>
      </main>
    </div>
  );
}
