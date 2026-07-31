import { invoke } from '@tauri-apps/api/core';

export function openExternalUrl(url: string): Promise<void> {
  return invoke('open_external_url', { url });
}
