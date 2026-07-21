import type { CloudSyncResult } from '../../local-api.ts';

export type CloudDialogKind = 'publish' | 'submit' | 'sync-submit' | 'submitted' | 'attention';

export interface CloudDialogModel {
  kind: CloudDialogKind;
  title: string;
  primaryLabel: string | null;
  commitMessageRequired: boolean;
  safeStop: boolean;
}

export function cloudDialogModel(status: CloudSyncResult, hasLocalChanges: boolean): CloudDialogModel {
  if (status.state === 'unlinked') {
    return {
      kind: 'publish',
      title: 'Publish to Cloud',
      primaryLabel: 'Publish Space',
      commitMessageRequired: hasLocalChanges,
      safeStop: false,
    };
  }
  if (status.state === 'conflicted' || status.state === 'leaseRejected') {
    return {
      kind: 'attention',
      title: 'Cloud sync needs attention',
      primaryLabel: null,
      commitMessageRequired: false,
      safeStop: true,
    };
  }
  if (status.state === 'submitted' && status.pullRequest) {
    return {
      kind: 'submitted',
      title: `Pull request #${status.pullRequest.number}`,
      primaryLabel: 'Update pull request',
      commitMessageRequired: hasLocalChanges,
      safeStop: false,
    };
  }
  if (status.state === 'needsSync') {
    return {
      kind: 'sync-submit',
      title: 'Sync and submit',
      primaryLabel: hasLocalChanges ? 'Commit, sync, and submit' : 'Sync and submit',
      commitMessageRequired: hasLocalChanges,
      safeStop: false,
    };
  }
  return {
    kind: 'submit',
    title: 'Submit to Cloud',
    primaryLabel: hasLocalChanges ? 'Commit and submit' : 'Submit current main',
    commitMessageRequired: hasLocalChanges,
    safeStop: false,
  };
}

export function cloudPullRequestUrl(status: CloudSyncResult): string | null {
  if (!status.cloudBaseUrl || !status.cloudSpaceId || !status.pullRequest) return null;
  const base = status.cloudBaseUrl.replace(/\/$/, '');
  return `${base}/cloud/spaces/${encodeURIComponent(status.cloudSpaceId)}/reviews/${encodeURIComponent(status.pullRequest.id)}`;
}

export function cloudSpaceUrl(status: CloudSyncResult): string | null {
  if (!status.cloudBaseUrl || !status.cloudSpaceId) return null;
  return `${status.cloudBaseUrl.replace(/\/$/, '')}/cloud/spaces/${encodeURIComponent(status.cloudSpaceId)}/wiki`;
}
