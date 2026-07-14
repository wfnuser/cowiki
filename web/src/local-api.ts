import { invoke } from '@tauri-apps/api/core';
import type {
  PageFull,
  PageMeta,
  SearchResponse,
  SourceContent,
  SourceItem,
  Workspace,
} from './api';

export function listSpaces(): Promise<Workspace[]> {
  return invoke('local_list_spaces');
}

export function addSpace(name: string, slug: string, localPath: string): Promise<Workspace> {
  return invoke('local_add_space', { name, slug, localPath });
}

export function listPages(spaceSlug: string, dir = 'all'): Promise<PageMeta[]> {
  return invoke('local_list_pages', { spaceSlug, dir });
}

export function getPage(spaceSlug: string, dir: string, pageSlug: string): Promise<PageFull> {
  return invoke('local_get_page', { spaceSlug, dir, pageSlug });
}

export function writePage(spaceSlug: string, dir: string, pageSlug: string, content: string): Promise<void> {
  return invoke('local_write_page', { spaceSlug, dir, pageSlug, content });
}

export function createFolder(spaceSlug: string, name: string, parent?: string): Promise<unknown> {
  return invoke('local_create_folder', { spaceSlug, name, parent });
}

export function listSources(spaceSlug: string): Promise<SourceItem[]> {
  return invoke('local_list_sources', { spaceSlug });
}

export function getSource(spaceSlug: string, filename: string): Promise<SourceContent> {
  return invoke('local_get_source', { spaceSlug, filename });
}

export function ingest(
  spaceSlug: string,
  sourceType: string,
  content: string,
  filename?: string,
): Promise<unknown> {
  return invoke('local_ingest', { spaceSlug, sourceType, content, filename });
}

export function submit(spaceSlug: string, paths: string[]): Promise<unknown> {
  return invoke('local_submit', { spaceSlug, paths });
}

export function search(spaceSlug: string, query: string, limit: number): Promise<SearchResponse> {
  return invoke('local_search', { spaceSlug, query, limit });
}

export function renamePath(spaceSlug: string, from: string, to: string): Promise<void> {
  return invoke('local_rename_path', { spaceSlug, from, to });
}

export function deletePath(spaceSlug: string, path: string): Promise<void> {
  return invoke('local_delete_path', { spaceSlug, path });
}
