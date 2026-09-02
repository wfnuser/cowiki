# CoWiki Design System

Warm editorial aesthetic, from the team-space design handoff: paper surfaces,
near-black ink, and a direct orange accent.

The tokens below document what the code actually uses. The authoritative
definitions live in `web/src/index.css` (`@theme` block for Tailwind v4, plus
`:root` for the shadcn semantic tokens).

**Direction:** new code takes colors from Tailwind token classes
(`bg-bg`, `text-text-secondary`, `bg-accent-soft`, …) or `var(--color-*)`
when an inline style is unavoidable. `web/src/lib/design.ts` exposes `C` as a
compatibility bridge whose values are CSS-variable references, not a second
copy of the palette. It also remains the home for values CSS cannot express:
the identity palette/hash, `fonts.mono`, `radii`, `shadows`, and
`APP_HEADER_HEIGHT`.

## Color Tokens

### Surfaces
| Token | Value | Usage |
|-------|-------|-------|
| `--color-bg` | `#faf9f7` | Page background |
| `--color-panel` | `#fdfcfb` | Cards, dialogs, raised surfaces |
| `--color-bg-secondary` | `#f5f4f1` | Secondary surfaces, sidebar |
| `--color-bg-hover` | `#efedea` | Hover states |
| `--color-bg-active` | `#e8e6e1` | Active/pressed states |
| `--color-border` | `#e8e6e1` | Default borders |
| `--color-border-hover` | `#d7d3ca` | Hover borders |
| `--color-border-soft` | `#eeece8` | Hairline dividers inside panels |

### Text
| Token | Value | Usage |
|-------|-------|-------|
| `--color-text` | `#1d1c1a` | Primary text (warm ink) |
| `--color-text-secondary` | `#403e3a` | Secondary text |
| `--color-text-tertiary` | `#8c897f` | Muted text, placeholders |
| `--color-text-faint` | `#a8a59b` | Faintest labels, hints |

### Brand
| Token | Value | Usage |
|-------|-------|-------|
| `--color-accent` | `#e2590b` | Brand orange — links, highlights, CTA |
| `--color-accent-hover` | `#c94b08` | Accent hover state |
| `--color-accent-fill` | `#c94b08` | Accessible accent-filled controls |
| `--color-accent-soft` | `#fbeadd` | Accent-tinted backgrounds |
| `--color-accent-soft-hover` | `#f7dcc4` | Hovered accent-tinted backgrounds |
| `--color-accent-ink` | `#9a3a07` | Text on accent-soft surfaces |
| `--color-on-accent` | `#ffffff` | Text/icons on strong accent or identity fills |

### Semantic
| Token | Value | Soft variant | Usage |
|-------|-------|--------------|-------|
| `--color-green` | `#2f8a5b` | fill `--color-green-fill` `#226b45`; soft `--color-green-soft` `#e3f1e9`; ink `--color-green-ink` `#226b45` | Success, positive actions |
| `--color-red` | `#cf222e` | `--color-red-soft` `#fcebe8` | Destructive actions, errors |
| `--color-amber` | `#b5790f` | `--color-amber-soft` `#f6ecd6`; ink `--color-amber-ink` `#7a5108` | Warnings |
| `--color-blue` | `#2f6bb0` | `--color-blue-soft` `#e6eef7` | Informational |
| `--color-tag` | `#ece8e1` | — | Tag/badge backgrounds |

Diff backgrounds and misc colors also live in `index.css`:
`--color-green-bg` `#dafbe1`, `--color-green-bg-soft` `#e6f9ec`,
`--color-red-bg` `#ffebe9`, `--color-red-bg-soft` `#fff5f4`,
`--color-purple` `#8250df`. Code blocks use `--color-code-bg` / `--color-code-fg`.
Sheet overlays use `--color-scrim`. History/Links keep their original tints as
`--color-accent-tint-*`, `--color-accent-wash*`, `--color-green-tint-border`,
and `--color-diff-mute`.

### Avatar & space-tile colors

Space tiles, member avatars, and comment avatars historically used three
slightly different palettes and hashes. Those originals live in
`lib/design.ts` (`spaceTileColors` + `colorForSpaceId` / `colorForName`,
`avatarColors` + `colorForAvatarName`, `commentAvatarColors` +
`colorForUserId`) so call sites share one implementation without changing
which colour a given id/name already had.

User avatars render through `AvatarBadge` with a `kind` that preserves the
original letter count and palette:

- `kind="member"` — members / invite / transfer: first letter, djb2 + tile palette
- `kind="comment"` — page comments: two-character initials, user-id palette
- default — reviews / byline / Cloud members: word initials, name palette

Pass `identityKey` for comment (and any other stable user id). Do not pass a
one-off `color=` to recreate these palettes. The Cloud invitation monogram stays
`spaceMonogramColor` (fixed tile green).

`--color-accent-fill`, `--color-*-ink` exist for new AA-contrast fills and
soft-surface text. Existing screens keep `accent` / `green` / `amber` on
those surfaces so this cleanup does not restyle the product.

## Typography

| Family | Variable | Usage |
|--------|----------|-------|
| Inter Variable | `--font-sans` | Body text, UI elements |
| Source Serif 4 Variable | `--font-serif` | Headlines, brand mark, page titles |
| System mono | `--font-mono` | Code, commit SHAs, xterm (`fonts.mono` in JS) |

Monospace is `--font-mono` (`ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace`).
Tailwind `font-mono` and `fonts.mono` in `lib/design.ts` share that stack. `fonts.mono`
keeps the concrete family list because xterm paints on a canvas and cannot resolve
`var(--font-mono)`.

### Brand Mark
```html
<span style="font-family: var(--font-serif); font-weight: 700;">
  CoWiki<span style="color: var(--color-accent);">.</span>
</span>
```

## shadcn Semantic Tokens

The `:root` block in `index.css` maps the same palette onto shadcn's HSL
tokens (`--background`, `--foreground`, `--primary`, `--muted`,
`--accent`, `--destructive`, `--border`, `--ring`, …) and sidebar tokens.
A `.dark` variant currently overrides the sidebar tokens only.

## Component Conventions

- **shadcn/ui for interactive primitives that already exist:** `Dialog`,
  `Select`, `DropdownMenu`, `Tabs`, `Tooltip`, `Sheet`, and standard form
  `Button` / `Input` / `Textarea`. Do not hand-roll modals;
  `components/ui/dialog.tsx` is the single dialog implementation.
- **Native HTML is correct for layout and custom chrome.** `div` / `span` /
  headings / lists / `form` / `a` have no shadcn equivalent. Icon-only rail
  buttons, ghost comment actions, clickable tree rows, and borderless search
  fields should stay native `<button>` / `<input>` rather than wrapping
  `Button` and then fighting its padding, height, and icon defaults.
- Base border radius: `--radius: 0.5rem` (8px); `rounded-sm/md/lg/xl` derive
  from it (4/6/8/12px). Dialogs use a larger 14px radius. For inline styles
  use the `radii` scale in `lib/design.ts` instead of ad-hoc numbers.
- Depth comes from borders and background tints, not heavy shadows. Dialogs
  use `--shadow-dialog`
  (`0 24px 64px rgba(29,28,26,0.22), 0 4px 16px rgba(29,28,26,0.10)`).
  `shadows.*` in `lib/design.ts` covers the remaining pre-existing shadows;
  new chrome should prefer `raised`, `subtle`, or `focus`.
- Dialogs are top-anchored (`top: 16vh`) rather than vertically centered,
  with `--color-dialog-scrim` `rgb(29 28 26 / 32%)` + `backdrop-blur(2px)`.
- Selection color: accent-tinted `color-mix(in srgb, var(--color-accent) 18%, transparent)` inside prose.
- Inline code: `--color-accent` on `--color-bg-hover`.

## Enforcement

`tests/design-system.test.ts` (part of `npm test`) guards these rules: inline
style bridge values must reference the canonical CSS palette, identity helpers
must keep the original palettes, hashes, and initials, fill/ink tokens (when
used) must stay WCAG-AA readable on their intended surfaces, and no
`src/**/*.ts(x)` file may hardcode colors outside the exact values owned by
the identity palettes, terminal theme, CodeMirror syntax, and shadow
definitions. UI chrome in those files is not exempt and must still use
design-system tokens.
