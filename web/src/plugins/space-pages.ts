/** The only page capabilities handed to trusted built-ins: no overwrite/delete. */
export interface SpacePagePorts {
  read: (path: string) => Promise<string>;
  createOnly: (path: string, body: string) => Promise<void>;
  refresh: () => Promise<void>;
  open: (path: string) => Promise<void>;
}

export interface SpacePageAdapter {
  read: (path: string) => Promise<string | null>;
  createOnly: (path: string, body: string) => Promise<'created' | 'exists'>;
  refresh: () => Promise<void>;
  open: (path: string) => Promise<void>;
}

export class PluginUnavailableError extends Error {
  constructor() { super('This plugin action is no longer available in the current Space'); }
}

function errorCode(error: unknown): unknown {
  return error && typeof error === 'object' && 'code' in error ? error.code : null;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function missing(error: unknown): boolean {
  // The current desktop IPC returns OS error strings rather than typed errors.
  // Do NOT interpret arbitrary “not found” messages (e.g. Git errors) as absence.
  return errorCode(error) === 'ENOENT'
    || /^No such file or directory \(os error 2\)$/.test(message(error))
    || /^The system cannot find the (?:file|path) specified\. \(os error [23]\)$/.test(message(error));
}

function markdownPath(path: string): void {
  const parts = path.split('/');
  if (!/\.md$/i.test(path) || path.includes('\\') || /^[a-z]:\//i.test(path)
    || parts.some((part) => !part || part.startsWith('.')) || parts.at(-1) === 'log.md') {
    throw new Error('Plugin page path must be a visible Space-relative Markdown path');
  }
}

export function createSpacePageAdapter(ports: SpacePagePorts, isCurrent: () => boolean): SpacePageAdapter {
  const guard = () => { if (!isCurrent()) throw new PluginUnavailableError(); };
  return {
    async read(path) {
      guard();
      markdownPath(path);
      try {
        const body = await ports.read(path);
        guard();
        return body;
      } catch (error) {
        guard();
        if (missing(error)) return null;
        throw error;
      }
    },
    async createOnly(path, body) {
      guard();
      markdownPath(path);
      try {
        await ports.createOnly(path, body);
        guard();
        return 'created';
      } catch (error) {
        guard();
        if (errorCode(error) === 'EEXIST' || message(error) === 'a page with this name already exists') return 'exists';
        throw error;
      }
    },
    async refresh() { guard(); await ports.refresh(); guard(); },
    async open(path) { guard(); markdownPath(path); await ports.open(path); guard(); },
  };
}
