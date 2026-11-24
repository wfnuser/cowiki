function twoDigits(value: number): string {
  return String(value).padStart(2, '0');
}

export function defaultCheckpointName(now = new Date()): string {
  return [
    `Checkpoint ${now.getFullYear()}-${twoDigits(now.getMonth() + 1)}-${twoDigits(now.getDate())}`,
    `${twoDigits(now.getHours())}:${twoDigits(now.getMinutes())}`,
  ].join(' ');
}

/** What the Draft holds, said after its name in History: "3 files changed". */
export function draftChangeLabel(changedFiles: number): string {
  if (changedFiles === 0) return 'No changes';
  return `${changedFiles} ${changedFiles === 1 ? 'file' : 'files'} changed`;
}

/** Checkpoints the app records on its own, as opposed to ones someone named. */
export const AUTO_CHECKPOINT_LABEL = 'Auto checkpoint';

/**
 * When a checkpoint was made, as a person says it: "4:05 PM" today,
 * "Yesterday, 4:05 PM", "Nov 22, 4:05 PM", and the year only when it differs.
 */
export function checkpointTime(created: Date, now = new Date(), locales?: string | string[]): string {
  const time = created.toLocaleTimeString(locales, { hour: 'numeric', minute: '2-digit' });
  const dayOf = (date: Date) => new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const days = Math.round((dayOf(now) - dayOf(created)) / 86_400_000);
  if (days === 0) return time;
  if (days === 1) return `Yesterday, ${time}`;
  const date = created.toLocaleDateString(locales, created.getFullYear() === now.getFullYear()
    ? { month: 'short', day: 'numeric' }
    : { month: 'short', day: 'numeric', year: 'numeric' });
  return `${date}, ${time}`;
}

export function canCreateCheckpoint(changedFiles: number | undefined): boolean {
  return changedFiles !== undefined && changedFiles > 0;
}
