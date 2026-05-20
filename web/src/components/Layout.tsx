import { Link, Outlet, useLocation } from 'react-router-dom';
import {
  BookOpen,
  FolderOpen,
  GitPullRequest,
  Search,
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
import { TooltipProvider } from '@/components/ui/tooltip';

const navItems = [
  { to: '/', label: 'Shared Wiki', icon: BookOpen },
  { to: '/personal', label: 'My Space', icon: FolderOpen },
  { to: '/reviews', label: 'Reviews', icon: GitPullRequest },
  { to: '/search', label: 'Search', icon: Search },
];

function AppSidebar() {
  const location = useLocation();

  return (
    <Sidebar>
      <SidebarHeader>
        <div className="flex items-center gap-2 px-2 py-1">
          <div className="flex h-6 w-6 items-center justify-center rounded-md bg-sidebar-primary text-sidebar-primary-foreground">
            <span className="text-xs font-bold">c</span>
          </div>
          <span className="font-semibold text-sm">cowiki</span>
        </div>
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
        <div className="flex items-center gap-2 px-2 py-1">
          <div className="flex h-6 w-6 items-center justify-center rounded-full bg-sidebar-accent text-sidebar-accent-foreground text-xs">
            U
          </div>
          <span className="text-xs text-sidebar-foreground/70">default</span>
        </div>
      </SidebarFooter>
    </Sidebar>
  );
}

export function Layout() {
  return (
    <TooltipProvider>
      <SidebarProvider>
        <AppSidebar />
        <SidebarInset>
          <div className="max-w-3xl mx-auto px-12 py-10">
            <Outlet />
          </div>
        </SidebarInset>
      </SidebarProvider>
    </TooltipProvider>
  );
}
