import { invoke } from '@tauri-apps/api/core';
import type {
  PageFull,
  PageMeta,
  FileDiff,
  SearchResponse,
  SourceContent,
  SourceItem,
  Workspace,
} from './api';

export function listSpaces(): Promise<Workspace[]> {
  return invoke('local_list_spaces');
}

export function addSpace(name: string, slug: string, localPath: string, createDirectory = false): Promise<Workspace> {
  return invoke('local_add_space', { name, slug, localPath, createDirectory });
}

export function listPages(spaceSlug: string, dir = 'all'): Promise<PageMeta[]> {
  return invoke('local_list_pages', { spaceSlug, dir });
}

export function getPage(spaceSlug: string, dir: string, pageSlug: string): Promise<PageFull> {
  return invoke('local_get_page', { spaceSlug, dir, pageSlug });
}

export function writePage(
  spaceSlug: string,
  dir: string,
  pageSlug: string,
  content: string,
  expectedContent?: string,
  createOnly = false,
): Promise<void> {
  return invoke('local_write_page', { spaceSlug, dir, pageSlug, content, expectedContent, createOnly });
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

export function workingDiff(spaceSlug: string): Promise<FileDiff[]> {
  return invoke('local_working_diff', { spaceSlug });
}

export function keepWorkingDiff(spaceSlug: string, expected: FileDiff[]): Promise<unknown> {
  return invoke('local_keep_working_diff', { spaceSlug, expected });
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
