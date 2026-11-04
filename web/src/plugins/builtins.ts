import { dailyNotesPlugin } from './daily-notes.ts';
import type { BuiltinPlugin } from './host.ts';

/** Only reviewed code shipped with the application. Never discover code in a Space. */
export const BUILTIN_PLUGINS: readonly BuiltinPlugin[] = [dailyNotesPlugin];
