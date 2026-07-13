declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    /** Injected by the desktop shell: origin of the embedded local backend. */
    __COWIKI_API_ORIGIN__?: string;
  }
}

const LOCAL_API_ORIGIN = 'http://localhost:3000';

export function isDesktopClient(): boolean {
  return typeof window !== 'undefined' && window.__TAURI_INTERNALS__ != null;
}

export function apiOrigin(): string {
  // Desktop shell injects the embedded backend's origin before page load —
  // it must win so an OS-assigned port still works.
  if (typeof window !== 'undefined' && window.__COWIKI_API_ORIGIN__) {
    return window.__COWIKI_API_ORIGIN__.replace(/\/$/, '');
  }

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
