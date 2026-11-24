import { type ClassValue, clsx } from "clsx"
import { extendTailwindMerge } from "tailwind-merge"

/** The type scale from index.css (`--text-*`); see DESIGN_SYSTEM.md, "Type scale". */
export const TYPE_SCALE = ["caption", "meta", "small", "body", "lead", "title"] as const

// tailwind-merge must know these are font sizes: otherwise it reads
// `text-meta` as a colour and drops it next to `text-text-tertiary`.
const twMerge = extendTailwindMerge({
  extend: { theme: { text: [...TYPE_SCALE] } },
})

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}
