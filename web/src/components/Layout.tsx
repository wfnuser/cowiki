import { Link, Outlet, useLocation, useNavigate, useParams } from 'react-router-dom';
import { useState } from 'react';
import {
  BookOpen,
  GitPullRequest,
  Search,
  LogOut,
  ChevronLeft,
} from 'lucide-react';
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarInset,
} from '@/components/ui/sidebar';
import { Input } from '@/components/ui/input';
import { TooltipProvider } from '@/components/ui/tooltip';
import { getStoredAuth, clearAuth } from '../auth';

function AppSidebar() {
  const location = useLocation();
  const navigate = useNavigate();
  const { workspaceSlug } = useParams();
  const [searchQuery, setSearchQuery] = useState('');

  const base = `/w/${workspaceSlug}`;

  const navItems = [
    { to: base, label: 'Wiki', icon: BookOpen },
    { to: `${base}/reviews`, label: 'Reviews', icon: GitPullRequest },
  ];

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (searchQuery.trim()) {
      navigate(`${base}/search?q=${encodeURIComponent(searchQuery.trim())}`);
    }
  };

  return (
    <Sidebar>
      <SidebarHeader>
        <div className="flex items-center gap-2 px-2 py-1">
          <Link to="/" className="text-sidebar-foreground/40 hover:text-sidebar-foreground/70 transition-colors">
            <ChevronLeft size={16} />
          </Link>
          <div className="flex h-5 w-5 items-center justify-center rounded bg-sidebar-primary text-sidebar-primary-foreground">
            <span className="text-[10px] font-bold">c</span>
          </div>
          <span className="font-semibold text-sm truncate">{workspaceSlug}</span>
        </div>

        {/* Search — like Feishu */}
        <form onSubmit={handleSearch} className="px-2 mt-1">
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-sidebar-foreground/40" />
            <Input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search..."
              className="h-7 pl-8 text-xs bg-sidebar-accent/50 border-none focus-visible:ring-1 focus-visible:ring-sidebar-ring"
            />
          </div>
        </form>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel>Workspace</SidebarGroupLabel>
          <SidebarMenu>
            {navItems.map((item) => {
              const active = location.pathname === item.to;
              return (
                <SidebarMenuItem key={item.to}>
                  <SidebarMenuButton asChild isActive={active} tooltip={item.label}>
                    <Link to={item.to}>
                      <item.icon />
                      <span>{item.label}</span>
                    </Link>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              );
            })}
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter>
        <UserFooter />
      </SidebarFooter>
    </Sidebar>
  );
}

function UserFooter() {
  const auth = getStoredAuth();
  const navigate = useNavigate();

  const handleLogout = () => {
    clearAuth();
    navigate('/login');
  };

  return (
    <div className="flex items-center justify-between px-2 py-1">
      <div className="flex items-center gap-2">
        <div className="flex h-6 w-6 items-center justify-center rounded-full bg-sidebar-accent text-sidebar-accent-foreground text-xs font-medium">
          {auth?.name?.[0]?.toUpperCase() || 'U'}
        </div>
        <span className="text-xs text-sidebar-foreground/70">{auth?.name || 'user'}</span>
      </div>
      <button
        onClick={handleLogout}
        className="text-sidebar-foreground/40 hover:text-sidebar-foreground/70 transition-colors"
        title="Sign out"
      >
        <LogOut size={14} />
      </button>
    </div>
  );
}

export function Layout() {
  return (
    <TooltipProvider>
      <SidebarProvider>
        <AppSidebar />
        <SidebarInset>
          <div className="max-w-3xl px-16 py-10">
            <Outlet />
          </div>
        </SidebarInset>
      </SidebarProvider>
    </TooltipProvider>
  );
}
