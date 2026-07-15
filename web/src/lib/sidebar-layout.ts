export const SIDEBAR_MIN_WIDTH = 220;
export const SIDEBAR_MAX_WIDTH = 480;
export const SIDEBAR_DEFAULT_WIDTH = 284;
export const SIDEBAR_LAYOUT_STORAGE_KEY = 'cowiki.sidebar.layout';

export interface SidebarLayout {
  width: number;
  collapsed: boolean;
}

export interface SidebarLayoutStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export function clampSidebarWidth(width: number): number {
  return Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, width));
}

export function loadSidebarLayout(storage: SidebarLayoutStorage): SidebarLayout {
  try {
    const value: unknown = JSON.parse(storage.getItem(SIDEBAR_LAYOUT_STORAGE_KEY) ?? 'null');
    if (
      value
      && typeof value === 'object'
      && typeof (value as Partial<SidebarLayout>).width === 'number'
      && Number.isFinite((value as Partial<SidebarLayout>).width)
      && typeof (value as Partial<SidebarLayout>).collapsed === 'boolean'
    ) {
      return {
        width: clampSidebarWidth((value as SidebarLayout).width),
        collapsed: (value as SidebarLayout).collapsed,
      };
    }
  } catch {
    // Ignore malformed settings and use the stable defaults below.
  }

  return { width: SIDEBAR_DEFAULT_WIDTH, collapsed: false };
}

export function saveSidebarLayout(storage: SidebarLayoutStorage, layout: SidebarLayout): void {
  storage.setItem(SIDEBAR_LAYOUT_STORAGE_KEY, JSON.stringify({
    width: clampSidebarWidth(layout.width),
    collapsed: layout.collapsed,
  }));
}
