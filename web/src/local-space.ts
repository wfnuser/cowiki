import { invoke } from '@tauri-apps/api/core';

export interface LocalSpaceIdentity {
  name: string;
  slug: string;
}

/** Ask the desktop shell for a folder. Browser builds deliberately return null. */
export async function chooseLocalSpaceDirectory(): Promise<string | null> {
  if (typeof window === 'undefined' || window.__TAURI_INTERNALS__ == null) return null;
  return invoke<string | null>('choose_local_space_directory');
}

/** Derive the user-facing identity without leaking the full local path into the UI. */
export function localSpaceIdentityFromPath(localPath: string): LocalSpaceIdentity {
  const withoutTrailingSeparators = localPath.replace(/[\\/]+$/, '');
  const name = withoutTrailingSeparators.split(/[\\/]/).at(-1)?.trim() || 'Local Space';
  const slug = name
    .normalize('NFKD')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '') || 'local-space';
  return { name, slug };
}
