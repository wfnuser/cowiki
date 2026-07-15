function twoDigits(value: number): string {
  return String(value).padStart(2, '0');
}

export function defaultCheckpointName(now = new Date()): string {
  return [
    `Checkpoint ${now.getFullYear()}-${twoDigits(now.getMonth() + 1)}-${twoDigits(now.getDate())}`,
    `${twoDigits(now.getHours())}:${twoDigits(now.getMinutes())}`,
  ].join(' ');
}

export function draftChangeLabel(changedFiles: number): string {
  if (changedFiles === 0) return 'No saved changes in the current draft';
  if (changedFiles === 1) return '1 saved file in the current draft';
  return `${changedFiles} saved files in the current draft`;
}
