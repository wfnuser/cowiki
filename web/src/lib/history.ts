function twoDigits(value: number): string {
  return String(value).padStart(2, '0');
}

export function defaultCheckpointName(now = new Date()): string {
  return [
    `Checkpoint ${now.getFullYear()}-${twoDigits(now.getMonth() + 1)}-${twoDigits(now.getDate())}`,
    `${twoDigits(now.getHours())}:${twoDigits(now.getMinutes())}`,
  ].join(' ');
}

export function draftChangeLabel(changedFiles: number, hasCheckpoint: boolean): string {
  if (changedFiles === 0 && hasCheckpoint) return 'No changes since the latest checkpoint';
  if (changedFiles === 0) return 'No saved changes in the current draft';
  if (hasCheckpoint) {
    if (changedFiles === 1) return '1 saved file changed since the latest checkpoint';
    return `${changedFiles} saved files changed since the latest checkpoint`;
  }
  if (changedFiles === 1) return '1 saved file in the current draft';
  return `${changedFiles} saved files in the current draft`;
}

export function canCreateCheckpoint(changedFiles: number | undefined): boolean {
  return changedFiles !== undefined && changedFiles > 0;
}
