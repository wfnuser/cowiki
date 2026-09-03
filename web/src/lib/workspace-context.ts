import type { CloudSyncState } from '../local-api.ts';

export interface WorkspaceContextStatus {
  kind: 'local' | 'linked' | 'cloud';
  label: string;
  detail: string;
  attention: boolean;
}

export function workspaceContextStatus({
  desktop,
  state,
  hasLocalChanges = false,
  statusKnown = true,
}: {
  desktop: boolean;
  state?: CloudSyncState | 'unavailable' | null;
  hasLocalChanges?: boolean;
  statusKnown?: boolean;
}): WorkspaceContextStatus {
  if (!desktop) {
    return {
      kind: 'cloud',
      label: 'Cloud',
      detail: 'Shared Cloud Space',
      attention: false,
    };
  }
  if (!statusKnown) {
    return {
      kind: 'local',
      label: 'Local · Checking link',
      detail: 'This Space is stored on this device',
      attention: false,
    };
  }
  if (state === 'unavailable') {
    return {
      kind: 'local',
      label: 'Local · Link unavailable',
      detail: 'CoWiki could not read this Space’s Cloud link status',
      attention: true,
    };
  }
  if (!state || state === 'unlinked') {
    return {
      kind: 'local',
      label: 'Local only',
      detail: 'Changes stay on this device until you publish',
      attention: false,
    };
  }
  if (hasLocalChanges || state === 'dirty') {
    return {
      kind: 'linked',
      label: 'Local + Cloud · Not uploaded',
      detail: 'Local changes have not been submitted to Cloud',
      attention: true,
    };
  }
  if (state === 'submitted') {
    return {
      kind: 'linked',
      label: 'Local + Cloud · In review',
      detail: 'Your Cloud submission is waiting for Review',
      attention: false,
    };
  }
  if (state === 'needsSync') {
    return {
      kind: 'linked',
      label: 'Local + Cloud · Cloud update',
      detail: 'Cloud has changes that are not in this local copy',
      attention: true,
    };
  }
  if (state === 'conflicted' || state === 'leaseRejected') {
    return {
      kind: 'linked',
      label: 'Local + Cloud · Attention',
      detail: 'Cloud sync needs your attention',
      attention: true,
    };
  }
  return {
    kind: 'linked',
    label: 'Local + Cloud · Synced',
    detail: 'This local copy is linked to Cloud',
    attention: false,
  };
}
