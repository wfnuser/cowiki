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

## Interface principles

Calm and spare, in the spirit of Obsidian, Notion and Bramble: the content
leads and the chrome stays quiet. Check every new screen against these.

- **Few words.** No explanatory subtitle under a page or dialog title. A
  control gets at most one short line of help, and only when the choice is
  not obvious. An empty state is one sentence.
- **Hairlines, not boxes.** Separate sections with a hairline
  (`border-border-soft`); never put a bordered card inside another bordered
  card. Lists are rows, not a stack of cards.
- **One thing per row.** A glyph, a name, at most one quiet value on the
  right. Details open on hover or click.
- **Colour as marks.** The accent fills one primary action per view (Send,
  Review, Allow once) and small marks (a dot, an icon, today in a calendar).
  It never fills a card or a section, and links to pages are ink or muted,
  not orange. State colours (red, amber, blue) colour a word or a dot, not a
  block.
- **Sentence case.** No ALL CAPS headings or labels. Section headings are 13px,
  weight 600, ink.
- **Say a thing once.** If a tab already says "No date", the list under it does
  not repeat "NO DATE · 22"; if every task in a group lives on one page, the
  page name appears once as the group's heading.
- **Fold the process.** An Agent's tool steps fold into one muted summary line
  ("2 commands · 1 tool call") that opens to every step; finished steps carry
  no check mark, only failed, declined and stopped ones get a mark.
- **Name things by what a person knows.** Show "Claude Pro", "Codex 0.166.0",
  "Tomorrow", never raw identifiers such as `oauth_token` or
  `2.1.296 (Claude Code)`.

## View layout

Every tool view (Tasks, Flashcards, History, Broken links, Reviews, Members,
Notifications, Review details, Cloud pages) is built the same way, with
`components/ui/view-header`:

- **Title:** `ViewHeader` renders `.view-title`: 22px, weight 650, **sans**.
  Serif (`.page-title`) is for content people write: page, source and
  drawing titles, and the welcome screen. A view title has no icon tile next
  to it and no subtitle unless it says something the title does not.
- **Actions** sit at the right end of the title row: icon-only
  (`ViewIconButton`, e.g. Refresh) or one small button.
- **Tabs** under the title are line tabs (`ViewTabs`): the open tab is
  underlined in ink, counts are faint numbers, never pills.
- **Groups** use `ViewSectionHeading` (13px, weight 600, sentence case, a
  faint count on the right). **Lists** are plain rows (`.view-list` /
  `.view-row` in `index.css`): no frame and no rule between rows; spacing
  and a hover fill separate them, and each row reaches 8px past the column
  so its icon lines up with the view title (user decision 2026-11-24:
  "lists looked unfinished next to the rest"). A row is one line: an icon,
  the name, an optional detail after it in tertiary (a folder, an author, a
  time), then at most one or two quiet values on the right (`5 files +31
  −0`; zeros faint).
  Never show raw ids or hashes; times read the way a person says them
  ("4:05 PM", "Yesterday, 4:05 PM"). **Empty and loading** states are one
  centered sentence (`ViewEmpty`).
- **History** is a timeline: the Current Draft (an accent ring) on top, then
  checkpoints newest first on one line of dots; named checkpoints are ink,
  automatic ones muted. *Save snapshot* sits in the title row and appears
  only when the Draft has changes (recording a snapshot without moving the
  branch; committing to the branch is *Commit draft* in Reviews).
- Views are left-aligned in the content area (`36px 56px` padding) with a
  `min(860px, 100%)` column; a wide board may use `min(1040px, 100%)`.
- **Tasks** tabs (Today, This week, Upcoming, No date) each read as a list
  or a board (user decision 2026-11-25), chosen by a two-icon switch in the
  title row and remembered per tab on the device; This week starts as a
  board, the others as lists, and Done is only a list. A board is To do /
  Doing / Done; its Done is what you finished today (This week: this week).
  A list of This week is grouped the same way. Saved custom views belong in
  query blocks in a page, not in this view.
- **Flashcards** read like Readwise (user decision 2026-11-25, replacing
  the Anki-style summary band): under the title, one sentence says how many
  cards are due today (*3 cards to review today.*) followed by one accent
  word, **Start**; when nothing is due it says *All done for today.* in
  tertiary, never a disabled button. Below are the decks as plain view rows
  (a chevron for sub-decks, the name, then "n to review" or a faint *Done*);
  no column headers and no New/Due split. Clicking a row opens the deck
  (a step in the top bar's Back and Forward, shown in the breadcrumb; the
  view has no back button of its own): the same sentence, a tertiary line of
  how well its cards are known (*3 cards: 1 new, 1 learning, 1 mastered.*),
  then the cards themselves, one per row: question, answer in tertiary, then
  two fixed right-aligned columns, mastery and next review (*in 4 days*).
  Mastery uses Anki's buckets: *New* (never answered), *Learning* (next
  review under 21 days away, or just forgotten), *Mastered* (21 days or
  more), *Suspended* (faint). A row opens the card's note; a suspended card
  offers **Resume** on hover. Hovering a deck with cards due swaps its count
  for **Start** in the same spot, so no gutter is reserved for it. The
  accent appears only on these action words (**Start**, **Resume**): no blue
  for new cards, no green for due ones (user: "blue and orange together is a
  bit much"). Decks come from tags: the card's own tags besides `#card`
  (`#Japanese/N2` is a sub-deck), else the page's `tags`, else its folder,
  else *Unsorted*.
- **Studying** is a centred focus column (`min(640px, 100%)`) with no colour.
  A session is its own Back/Forward step: Back leaves it, Forward studies
  that deck again, and finishing or ending steps back to where it started.
  The top line has the deck and *New*/*Review n* in plain text on the left, "n left" and quiet
  icon tools (open note, skip, suspend, reset, end) on the right, a 2px
  neutral progress line, then one paper card (`.study-card`: 12px radius,
  `--shadow-card`). The question is content (serif 22px, centred); revealing
  adds a lower answer pane under a hairline without moving the frame
  (single-bullet answers centre cleanly without a lone dot). Below the card
  sit text buttons only: *Show answer* with its key, then on reveal three
  equal answers (label, next interval in the reader's language, key hint),
  with a hover fill and no red/amber/green.
- **Scrolling areas keep room for the scrollbar** (`.scroll-pane`), so a
  list growing past the window never narrows the column.

- **Callouts** (`> [!tip] Title`, `lib/callouts.ts`) look the same in the
  reader and the editor: lucide's icon (drawn as a CSS mask) and a bold
  title in the callout's colour, and a quote rule in that colour. No fill.
  Families: note/info (blue), tip/success (green), question/warning (amber),
  danger/failure/bug (red), quote/example (grey). In the editor `[!tip]`
  shows again while the caret is on the line.
- **Links in text** take a thin underline at 30% that firms up on hover;
  page links are accent, web links ink. **Code** is highlighted with the
  `--color-syntax-*` tokens, which have dark values.
- **Diffs** label a hunk by the lines it shows ("Lines 4–6"); the raw `@@`
  header is the tooltip. A file says *Added* or *Deleted* in green or red; an
  edited file says nothing more.

## Start page, editing and the top bar

- **No Wiki entry.** The page tree already reaches every page. Every Space
  opens on **Home**, never on its first file (user decision 2026-11-24).
  Home is small on purpose:
  - one quiet line of what needs you: tasks due or overdue, cards to
    review today, changes waiting for review (your unsent draft counts in a
    shared Space), broken links. Only non-zero counts show; with nothing to
    do the line is gone. Each count opens its view. **Broken links** (links
    to pages that do not exist yet) have no sidebar entry of their own
    (user decision 2026-11-25): in Obsidian such links are often deliberate,
    so they are a count here, not a place to visit.
  - **today's journal**, written in place (when the Journal module is on);
  - **Recently edited** pages as hairline rows (local Spaces; a Cloud Space
    in the browser cannot list edit times).
- **Journals**, the next entry when the Journal module is on, is the full
  stream, as in Logseq: scrolled with the wheel, today first, earlier days
  loading below (seven at a time; only days that have a note, plus today).
  Each day sits under its date ("Nov 24th, 2026", serif 30px) with room
  below its text (today the most) and a hairline between days; headings
  inside a day are 15px sans section labels. An empty day is a single faint
  bullet, ready for text. Opening Home or Journals writes nothing; today's
  file is created from the journal template on the first click.
  To reach a day, **Go to a day** (a calendar icon in the top bar, in the
  slot comments use on a page) opens a month with a dot under every day
  that has a note, as in Obsidian's Calendar; only those days and today can
  be picked, and the stream loads down to the day and scrolls to it. The
  `journal` folder stays in the page tree (user decision 2026-11-25).
- **Click to edit**, as in Logseq; there is no Edit button. A single click
  in a page or journal day opens the editor with the caret where you
  clicked (matched through the text, `lib/click-to-edit.ts`). Links,
  controls and comment highlights never start editing, and a drag selection
  stays a selection, so selecting text to comment still works. Esc (after
  any open completion menu and any selection) returns to reading; a journal
  day also returns when focus leaves it. While editing, the comments
  button saves, returns to reading and opens the comments.
- **Editing never moves the page.** The reader (`.prose`) and the editor
  (`components/editor/theme.ts`, `livePreview.ts`) share the `--doc-*`
  tokens in `index.css` and the same text column (`.page-column`, byline
  first), so every line stays where it was when you click to edit and
  when you return (`tests/browser/page-layout.spec.ts` checks this to
  within a pixel). In the editor a blank line is one `--doc-gap`, the
  reader's space between blocks; bullets, numbers, checkboxes and quote
  bars are drawn the same way in both and never turn back into `- ` or
  `> ` under the caret; a heading's `## ` appears in the left margin while
  the caret is on it; code fences show only while the caret is in the
  block; tables stay rendered until the caret enters them. A single newline
  is a line break in both, as in Obsidian.
- **Top bar, left:** back and forward arrows (desktop), as in Notion, then
  a name only where the view below does not show one: a page's name (its
  first heading may differ) and Journals (below are only dates). Views that
  title themselves (Tasks, Flashcards, Reviews, History…) and the Space,
  which the sidebar's top names, add nothing (user decision 2026-11-27).
  Every page, journal, tool view and Space you open is one
  step (`lib/view-history.ts`); a renamed page keeps its steps and a deleted
  one leaves them. ⌘[ and ⌘] outside text (the editor uses them to indent),
  ⌘⌥← and ⌘⌥→ everywhere (Obsidian), and the mouse's side buttons. An arrow
  with nowhere to go stays in place, faint. In the browser the browser's own
  history does this. Views add no back link of their own (a review detail
  has no "← Reviews"); only the Cloud shell, which has no top bar, keeps one.
- **Top bar, right to left:** the Agent toggle in the corner, comments, the
  Space status. The Agent sidebar opens from a Bot icon in the top bar's right
  corner and closes from the same corner of its own header (`PanelRight`), so
  the toggle never takes space next to an open sidebar. Comments show a
  count only when there are open comments. The **Space status** is one quiet
  menu: a place icon (drive for local, cloud when linked) and the version
  you are reading (Draft, main, or an Agent change). Words about sync live
  in the menu; the bar shows only a **small dot when changes are not
  uploaded or need attention**.
- Enter that confirms an input-method candidate never sends or submits
  (`lib/ime.ts`): WebKit reports it with key code 229 after the
  composition ends.

## Sign in

One centered column on the page background: the mark, the slogan in serif,
one muted line, then *Continue with GitHub* (ink) and *Continue locally*
(hairline). No card, no split panels, no caps eyebrow, no "Welcome back".

## Navigation

- Sidebar entries use `--color-text-nav`, a notch quieter than text, so
  the page leads; hover and the open entry are ink. The Space rail shows
  each Space as a muted colour tile; the open one is marked by a bar at the
  rail's edge, not a ring.
- The desktop rail is 79px wide (its 1px border included) so macOS 26's
  traffic lights (14pt, x 9–69, measured in the running app) keep 9px on both
  sides and sit centred over the logo; the browser's rail, with no traffic
  lights, is 68px.
- The open item in the left navigation is a quiet grey row
  (`bg-bg-active/70`, ink text) with only its icon in the accent, the same
  treatment as the open page in the tree. No orange fills.
- **One Settings window** (user decision 2026-11-24), as in Notion or
  Linear: a left list in two groups. **The open Space** (General: where it
  lives and its Cloud action, modules, templates; then one page per module
  with options, e.g. Journal) is stored in the Space and shared with its
  members, so its changes are saved together from a bar that appears once
  something changed. **App** (General: appearance, language, default Agent,
  history checkpoints; Account; API Keys) is only you, on this computer, and
  applies at once. The **Settings** gear at the foot of the sidebar opens
  the Space's group; *Settings* in the avatar menu opens the app's. The
  Space name at the top of the sidebar is a plain title, not a menu.
  Members stay a navigation entry of a Cloud Space, not a settings page.
- The Agent panel is organised by **session**. Its header shows the
  session's title, its status only when it needs attention, and three quiet
  icons: Session history, New session, More. History lists every session of
  the Space by day (Today, Yesterday, Previous 7 days, Older); a new session
  shows the five most recent above the composer.
- The composer footer names the Agent, its model and where it works. The
  model is a quiet menu of **only the models the Agent itself listed** in
  its handshake (Codex `model/list`, Claude Code `initialize`, ACP
  `modelState` / `session/new`); CoWiki never adds its own list. A change applies from the
  next message, and new sessions start with the Agent and model used last.

## Form controls

A control never shows the platform's look: no system-blue checkbox or focus
glow, no native select menu, no number spinner, no native date field.

| Need | Use |
|------|-----|
| Pick one of several | `components/ui/select` (Radix), never `<select>` |
| A whole number | `components/ui/number-field` (− value +) |
| A date | `components/ui/date-picker` (quick choices and a month grid) |
| Check a task | `<input type="checkbox">`: `index.css` draws it (accent fill, white check) |
| Turn a setting on or off | `<input type="checkbox" className="cw-switch">` |
| Several fixed modes | A segmented control: a sunk track with a raised chip (see the Agent panel's "Where it works") |

Focus is the accent outline from `index.css` (or the
`--shadow-focus` ring on fields), never the system ring. A bare `border`
uses `--color-border` (the base rule in `index.css`); Tailwind v4 would
otherwise draw it in the text colour.

Settings forms: `.settings-section` blocks separated by hairlines, each with a
`.settings-heading`; one setting per row, its switch at the end of the row;
fields in a two-column grid (label, input).

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
| Source Serif 4 Variable | `--font-serif` | Content titles (pages, sources, drawings), prose headings, brand mark, welcome screens. Never tool-view titles |
| System mono | `--font-mono` | Code, commit SHAs, xterm (`fonts.mono` in JS) |

Monospace is `--font-mono` (`ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace`).
Tailwind `font-mono` and `fonts.mono` in `lib/design.ts` share that stack. `fonts.mono`
keeps the concrete family list because xterm paints on a canvas and cannot resolve
`var(--font-mono)`.

### Type scale

Interface text uses five sizes, as Tailwind classes from `index.css`
(`--text-*`), plus display sizes for titles. Decided 2026-11-24 after an
audit found fifteen sizes (10, 10.5, 11, 11.5, 12, 12.5, 13, 13.5, 14, 14.5,
15, 15.5, 16, 17, 18px) drifting between screens.

| Class | Size | Use |
|-------|------|-----|
| `text-caption` | 11px | Counts in badges, keyboard hints |
| `text-meta` | 12px | Times, sizes, a row's detail, column heads |
| `text-small` | 13px | Secondary sentences: descriptions, errors, empty states, menu text, section headings (600) |
| `text-body` | 14px | Rows, inputs, buttons, the sidebar |
| `text-lead` | 16px | Dialog titles, the search field, a flashcard face |
| `text-title` | 22px | View titles (`.view-title`) |

`cn()` (`lib/utils.ts`) teaches tailwind-merge these names; without that it
reads `text-meta` as a colour and drops it beside `text-text-tertiary`.

### Radius

4 chips and keys · 6 rows, buttons, inputs · 8 menus, popovers, panels,
diff files · 10 cards · 12 dialogs and Space tiles · 999 pills. (20 only for
the welcome screen's large mark.) Inline `borderRadius` values are checked.

Larger sizes are display type (journal dates 30px, page titles 36px, the
welcome screen) and use the serif. Page and journal text uses the
`--doc-*` tokens, not this scale. Line height comes from the element
(`leading-*`); a size class does not set one. Hierarchy comes from weight
and colour (ink, secondary, tertiary, faint) before size: a section heading
is 13px semibold ink, not a bigger size.

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
`html.dark` overrides every one of them (see Dark theme).

## Dark theme

Settings → Appearance offers System, Light and Dark (stored per device,
`lib/theme.ts`). The choice puts `.dark` on `<html>`; `index.css` then
redefines **every** `--color-*` token, the shadcn tokens and the
`--shadow-*` tokens under `html.dark`. Components therefore need no `dark:`
variants: if a screen looks wrong in dark, a token is missing or a colour
was hardcoded. The dark palette keeps the warm character: charcoal paper
(`#1c1b19`), warm off-white ink, and a slightly brighter accent
(`#f07028`) so it reads on dark. Excalidraw drawings follow the app theme
the way Excalidraw's own dark mode does (an inverted, hue-rotated render).
The logo swaps to `cowiki-logo-dark.svg` (light centre dot).

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
  No dialog runs past the window: `DialogContent` caps its height and
  scrolls. A long form (Modules & settings) is centered instead, with its
  title and actions pinned and only the body scrolling.
- Menus, selects and popovers share one surface: `bg-panel`, a hairline,
  `rounded-lg` and `--shadow-popover`; items are 13px rows with a hover wash.
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

It also fails when a component uses a platform control that has a CoWiki
replacement (`<select>`, `type="number"`, `type="date"` outside
`components/ui`), when a new shadow is written as a raw `rgba()` instead of a
`--shadow-*` token, or when the base border colour rule is missing.

It fails when interface text leaves the type scale (an arbitrary
`text-[13.5px]`, an inline `fontSize: 15`, a CSS `font-size: 12.5px`), and
when a transition animates `all` instead of naming its properties.

It also fails when a colour token has no dark value, when a component sets
text in ALL CAPS (`uppercase`, `textTransform: 'uppercase'`), or when a tool
view titles itself with `.page-title` or a serif font instead of
`ViewHeader` / `.view-title`.

## Reviewing the whole app

`tests/browser/screens.spec.ts` photographs every main screen (Home,
Journals, a page read and edited, Tasks, Flashcards and a study card,
Reviews and a diff, History, Links, search, both Settings groups, the Space
menu) in light and dark with fixed demo data. It is skipped in normal runs;
capture a gallery before and after a polish pass and compare:

```sh
SCREENS=/tmp/cowiki-screens npx playwright test screens --project=chromium
```

### Sources for this system

The 2026-11-24 pass checked the app against:
[Refactoring UI](https://refactoringui.com/) (hierarchy by weight and
colour, fewer borders, labels last), Vercel's
[Web Interface Guidelines](https://github.com/vercel-labs/web-interface-guidelines)
(tabular numbers, no `transition: all`, visible focus, every state designed),
Linear's [calmer interface refresh](https://linear.app/now/behind-the-latest-design-refresh)
(navigation a notch dimmer than content, fewer and softer separators, no
coloured icon backgrounds) and [Radix Themes](https://www.radix-ui.com/themes/docs/theme/typography)
(a fixed type scale as tokens).
