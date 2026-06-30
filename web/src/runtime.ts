declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

const LOCAL_API_ORIGIN = 'http://localhost:3000';

export function isDesktopClient(): boolean {
  return typeof window !== 'undefined' && window.__TAURI_INTERNALS__ != null;
}

export function apiOrigin(): string {
  const configured = import.meta.env.VITE_API_BASE;
  if (configured) return configured.replace(/\/$/, '');

  if (typeof window !== 'undefined') {
    const saved = window.localStorage.getItem('cowiki.apiOrigin');
    if (saved) return saved.replace(/\/$/, '');
  }

  return isDesktopClient() ? LOCAL_API_ORIGIN : '';
}

export function apiBase(): string {
  return `${apiOrigin()}/api`;
}
