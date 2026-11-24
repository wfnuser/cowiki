/** `@@ -1,2 +4,3 @@ heading` → "Lines 4–6", the place a person can find. */
export function hunkLabel(header: string): string {
  const match = /^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@\s*(.*)$/.exec(header);
  if (!match) return header;
  const start = Number(match[1]);
  const count = match[2] == null ? 1 : Number(match[2]);
  const where = count <= 1 ? `Line ${start}` : `Lines ${start}–${start + count - 1}`;
  return match[3] ? `${where} · ${match[3]}` : where;
}
