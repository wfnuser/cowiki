declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    /** Injected by the desktop shell: origin of the embedded local backend. */
    __COWIKI_API_ORIGIN__?: string;
  }
}

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

  // Desktop content operations use Tauri directly. HTTP is reserved for
  // explicit Cloud capabilities such as sign-in, publishing, and shared
  // Spaces, so the hosted origin is safe as the final fallback.
  if (isDesktopClient()) return 'https://api.cowiki.app';
  return '';
}

export function apiBase(): string {
  return `${apiOrigin()}/api`;
}
