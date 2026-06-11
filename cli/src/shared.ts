import type { CowikiClient } from './client.js';
import { ConfigError } from './error.js';

// ── Global option types ────────────────────────────────

export interface GlobalOpts {
  server?: string;
  workspace?: string;
  json?: boolean;
}

// ── Branch resolution ──────────────────────────────────

let cachedBranch: string | null = null;

/** Resolve user branch: always user/<id> when authenticated, else "main". */
export async function resolveUserBranch(client: CowikiClient): Promise<string> {
  if (cachedBranch !== null) return cachedBranch;
  try {
    const me = await client.getMe();
    cachedBranch = `user/${me.id}`;
  } catch {
    cachedBranch = 'main';
  }
  return cachedBranch;
}

// ── Workspace requirement ───────────────────────────────

/** Require workspace from global opts; throws if missing. */
export function requireWorkspace(workspace?: string): string {
  if (!workspace) {
    throw new ConfigError(
      'Workspace required. Use -w <slug>.\n' +
        'Tip: run "cowiki workspaces" to list available workspaces.',
    );
  }
  return workspace;
}
