import { invoke } from '@tauri-apps/api/core';
import type {
  AgentChange,
  BrokenLink,
  PageFull,
  PageMeta,
  FileDiff,
  IngestFileOutcome,
  Checkpoint,
  SearchResponse,
  SourceContent,
  SourceItem,
  SpaceHistory,
  Workspace,
} from './api';

export function listSpaces(): Promise<Workspace[]> {
  return invoke('local_list_spaces');
}

export function addSpace(name: string, slug: string, localPath: string, createDirectory = false): Promise<Workspace> {
  return invoke('local_add_space', { name, slug, localPath, createDirectory });
}

export function listPages(spaceSlug: string): Promise<PageMeta[]> {
  return invoke('local_list_pages', { spaceSlug });
}

export function getPage(spaceSlug: string, conceptId: string): Promise<PageFull> {
  return invoke('local_get_page', { spaceSlug, pageSlug: conceptId });
}

export function writePage(
  spaceSlug: string,
  conceptId: string,
  content: string,
  expectedContent?: string,
  createOnly = false,
): Promise<void> {
  return invoke('local_write_page', { spaceSlug, pageSlug: conceptId, content, expectedContent, createOnly });
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
): Promise<SourceItem> {
  return invoke('local_ingest', { spaceSlug, sourceType, content, filename });
}

/** Opens a native multi-file picker filtered to formats CoWiki can extract. */
export function chooseSourceFiles(): Promise<string[]> {
  return invoke('choose_source_files');
}

export function ingestFiles(spaceSlug: string, sourcePaths: string[]): Promise<IngestFileOutcome[]> {
  return invoke('local_ingest_files', { spaceSlug, sourcePaths });
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

export function history(spaceSlug: string): Promise<SpaceHistory> {
  return invoke('local_history', { spaceSlug });
}

export function createCheckpoint(spaceSlug: string, name?: string): Promise<Checkpoint> {
  return invoke('local_create_checkpoint', { spaceSlug, name });
}

export function createAgentChange(spaceSlug: string, agentName: string): Promise<AgentChange> {
  return invoke('local_create_agent_change', { spaceSlug, agentName });
}

export function listAgentChanges(spaceSlug: string): Promise<AgentChange[]> {
  return invoke('local_list_agent_changes', { spaceSlug });
}

export function mergeAgentChange(spaceSlug: string, changeId: string): Promise<AgentChange> {
  return invoke('local_merge_agent_change', { spaceSlug, changeId });
}

export function discardAgentChange(spaceSlug: string, changeId: string): Promise<AgentChange> {
  return invoke('local_discard_agent_change', { spaceSlug, changeId });
}

export function search(spaceSlug: string, query: string, limit: number): Promise<SearchResponse> {
  return invoke('local_search', { spaceSlug, query, limit });
}

export function listBrokenLinks(spaceSlug: string): Promise<BrokenLink[]> {
  return invoke('local_list_broken_links', { spaceSlug });
}

export function renamePath(spaceSlug: string, from: string, to: string): Promise<void> {
  return invoke('local_rename_path', { spaceSlug, from, to });
}

export function deletePath(spaceSlug: string, path: string): Promise<void> {
  return invoke('local_delete_path', { spaceSlug, path });
}

export type CloudSyncState =
  | 'unlinked'
  | 'dirty'
  | 'upToDate'
  | 'needsSync'
  | 'synced'
  | 'conflicted'
  | 'submitted'
  | 'leaseRejected';

export interface CloudPullRequest {
  id: string;
  number: number;
  title: string;
  headRef: string;
  headOid: string;
  status: string;
}

export interface CloudSyncResult {
  state: CloudSyncState;
  conflicts: string[];
  committed: boolean;
  message: string;
  pullRequest: CloudPullRequest | null;
}

export interface CloudLinkOptions {
  spaceSlug: string;
  cloudBaseUrl: string;
  apiKey: string;
  cloudName: string;
  cloudSlug: string;
  userName: string;
  userId: string;
  cloudSpaceId?: string;
  gitUrl?: string;
  commitMessage?: string;
}

export function linkCloudSpace(options: CloudLinkOptions): Promise<CloudSyncResult> {
  return invoke<CloudSyncResult>('cloud_link_space', { ...options });
}

export function getCloudStatus(spaceSlug: string): Promise<CloudSyncResult> {
  return invoke<CloudSyncResult>('cloud_get_status', { spaceSlug });
}

export function syncCloudIfClean(spaceSlug: string, apiKey: string): Promise<CloudSyncResult> {
  return invoke<CloudSyncResult>('cloud_sync_if_clean', { spaceSlug, apiKey });
}

export function submitCloud(options: {
  spaceSlug: string;
  apiKey: string;
  userName: string;
  commitMessage?: string;
  pullRequestTitle?: string;
  pullRequestBody?: string;
}): Promise<CloudSyncResult> {
  return invoke<CloudSyncResult>('cloud_submit', { ...options });
}

export function continueCloudRebase(spaceSlug: string): Promise<CloudSyncResult> {
  return invoke<CloudSyncResult>('cloud_rebase_continue', { spaceSlug });
}

export function abortCloudRebase(spaceSlug: string): Promise<CloudSyncResult> {
  return invoke<CloudSyncResult>('cloud_rebase_abort', { spaceSlug });
}
