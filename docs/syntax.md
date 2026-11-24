# CoWiki Markdown syntax

Status: describes the `dev` branch on 2026-11-24. Decision (user, 2026-11-24):
**Obsidian's syntax is the base, Logseq's is read for compatibility, and
queries will follow Dataview.** This page is the reference for what CoWiki
reads and writes; where it differs from Obsidian or Logseq, it says so.

Legend: ✅ supported · 👁 shown, not specially handled · ⏳ planned · ✖ not planned

## Principles

- **Files stay plain Markdown.** A Space is a folder of `.md` files (an
  [OKF v0.1](okf-v0.1.md) bundle). CoWiki never rewrites a file to convert
  syntax; compatibility happens when a page is read or rendered.
- **One dialect.** CommonMark + GitHub Flavored Markdown, extended with
  Obsidian's syntax. Logseq-only syntax is understood where a feature needs
  it (tasks, flashcards, properties), not written by CoWiki.
- **Reference implementations**, so behaviour can be checked against
  something standard:
  - CommonMark and GFM: [micromark / remark](https://github.com/micromark/micromark)
    (MIT), which the reader already uses through `react-markdown` and
    `remark-gfm`.
  - Obsidian syntax: [Obsidian Flavored Markdown](https://help.obsidian.md/obsidian-flavored-markdown)
    (documentation; the app is closed source) and the OFM transformer in
    [Quartz](https://github.com/jackyzha0/quartz) (MIT; code may be reused
    with attribution).
  - Logseq syntax: [Logseq](https://github.com/logseq/logseq) and its
    `mldoc` parser are AGPL-3.0. CoWiki is Apache-2.0, so we follow their
    documented **behaviour only** and copy no code.
  - Queries: [Dataview](https://github.com/blacksmithgu/obsidian-dataview)
    (MIT) is the model for the query language.

## Pages and properties

| Syntax | Status | Notes |
| --- | --- | --- |
| YAML frontmatter (`---` … `---`) | ✅ | Required by OKF: a non-empty `type`. Hidden in the reader and the editor; CoWiki keeps it byte for byte when saving. |
| `tags:` in frontmatter (list, `[a, b]`, or `a, b`) | ✅ | Used to sort flashcards into decks. |
| Page title | ✅ | New pages get both `title:` and a first `# Heading`. |
| Logseq page properties (`tags:: a, [[b]]` before the first bullet) | ✅ | Read for flashcard decks. Other `key:: value` lines show as text. |

## Blocks

| Syntax | Status | Notes |
| --- | --- | --- |
| Headings, paragraphs, lists, block quotes, rules | ✅ | Reader and editor lay them out identically (`web/DESIGN_SYSTEM.md`, "Editing"). |
| Single newline inside a paragraph | ✅ | A line break, as in Obsidian (its "strict line breaks" is off by default). Plain CommonMark would join the lines. |
| Fenced code | ✅ | Highlighted in the editor; fences show only while the caret is in the block. |
| GFM tables | ✅ | Rendered in the reader; edited as source (monospace). |
| GFM task lists `- [ ]` / `- [x]` | ✅ | See [Tasks](#tasks). |
| GFM footnotes, strikethrough, autolinks | ✅ | |
| Raw HTML inside Markdown | 👁 | Not rendered (shown as text). A ` ```html ` code block shows a sandboxed preview, and a whole `.html` file opens in a sandboxed view. |
| Math (`$…$`, `$$…$$`) | ⏳ | Shown as text today. |
| Mermaid diagrams | ⏳ | Shown as code today. |

## Obsidian syntax

| Syntax | Status | Notes |
| --- | --- | --- |
| `[[Page]]`, `[[folder/Page]]` | ✅ | Resolved like Obsidian: by file name anywhere; a file next to the linking page wins, then the shortest path. |
| `[[Page\|alias]]` | ✅ | Shows the alias. |
| `[[Page#Heading]]`, `[[Page#^block]]` | ✅ | Opens the page; jumping to the heading or block is ⏳. |
| `![[image.png]]`, `![[image.png\|300]]` | ✅ | Images from the Space's attachments. |
| `![[Note]]` (note embed) | 👁 | Shown as a link (`↪ Note`); transclusion is ⏳. |
| `^block-id` at the end of a line | ✅ | Hidden in the reader. |
| `==highlight==` | 👁 | Rendered bold today; a highlight mark is ⏳. |
| `%%comment%%`, multi-line `%%` | ✅ | Hidden in the reader. |
| Callouts `> [!note] Title` | ✅ | Icon and title, then the quote. Folding (`+`/`-`) is ignored. |
| `#tag`, `#nested/tag` | 👁 | Shown as text; used by Tasks and Flashcards. A tag index is ⏳. |
| Excalidraw drawings (`.excalidraw.md`) | ✅ | Read-only view. |
| Canvas (`.canvas`) | ✖ | |

## Logseq syntax (read for compatibility)

| Syntax | Status | Notes |
| --- | --- | --- |
| `TODO` / `DOING` / `NOW` / `LATER` / `DONE` / `CANCELED` | ✅ | Tasks; shown as checkboxes in the reader. |
| `[#A]` `[#B]` `[#C]` priorities | ✅ | High, medium, low. |
| `SCHEDULED: <2026-11-24 Tue>`, `DEADLINE: <…>` on the next line | ✅ | Task dates. |
| `#card`, `[[card]]`, `{{cloze …}}` | ✅ | See [Flashcards](#flashcards). |
| `card-*::`, `id::`, `collapsed::` block properties | ✅ | Hidden; `card-*::` seed a card's schedule on first review. |
| Other `key:: value` block properties | 👁 | Shown as text. |
| Block references `((uuid))`, `{{embed …}}` | ⏳ | Shown as text. |
| `{{query …}}`, advanced (Datalog) queries | ✖ | Use [queries](#queries) instead. |
| Outliner semantics (every line is a block) | ✖ | CoWiki pages are documents; bullets are ordinary lists. |

## Tasks

A task is a list item in any page; the Tasks view and Home read it and only
ever rewrite that one line.

| Syntax | Meaning |
| --- | --- |
| `- [ ]` · `- [x]` · `- [/]` · `- [-]` | Open, done, in progress, cancelled (Obsidian Tasks statuses). |
| `📅 2026-11-24` · `⏳` · `🛫` · `✅` | Due, scheduled, start, done date (Obsidian Tasks). |
| `🔺 ⏫ 🔼 🔽 ⏬` | Priority, highest to lowest. |
| `[due:: 2026-11-24]`, `(scheduled:: …)` | Dataview inline fields. |
| Logseq markers | See above. |

A task in a journal page without dates belongs to that day, so an unfinished
item from yesterday's plan is overdue today.

## Flashcards

```markdown
- What does the borrow checker enforce? #card #Rust/ownership
  - Ownership and borrowing rules
- The {{cloze mitochondria}} makes energy for the cell #card #Biology
```

- **A card** is a bullet tagged `#card` (or `[[card]]`); its indented
  children are the answer. `{{cloze text}}` hides `text` on the question.
- **Decks** (as in Anki): the card's own tags besides `#card`
  (`#Rust/ownership` is a sub-deck of `Rust`), else the page's tags, else the
  page's folder, else *Unsorted*. A card with two tags is in both decks.
  Tags are not part of the question, so tagging a card keeps its history.
- **Scheduling** follows Logseq's SM-5. Review history is personal: it is
  kept on each device, outside the Space, so reviewing never edits a note.

Differs from Logseq: decks (Logseq has none; it filters by tag). Differs from
Obsidian's Spaced Repetition plugin: no `Question::Answer` or `?` separator
syntax yet (⏳).

## Journals

- One file per day in the journal folder (default `journal/`), named with a
  [moment.js](https://momentjs.com/docs/#/displaying/format/) format
  (default `YYYY-MM-DD`), like Obsidian's Daily notes.
- A new day starts from the template page set in Settings, if any.
  Templates support `{{date}}`, `{{title}}` and `{{time}}` (Obsidian's
  core Templates tokens); `{{date:FORMAT}}` is ⏳.

## Queries

⏳ A subset of Dataview's query language in ` ```dataview ` blocks, starting
with `LIST` and `TABLE … FROM #tag / "folder" WHERE … SORT …`. Until then,
` ```dataview ` and ` ```tasks ` blocks are shown as code; the Tasks view
and Flashcards cover the common cases. Logseq queries are not planned.

## Where CoWiki differs

| | Obsidian | Logseq | CoWiki |
| --- | --- | --- | --- |
| Unit of a page | Document | Outline of blocks | Document |
| Line breaks | Single newline breaks (default) | Each line is a block | Single newline breaks |
| Page metadata | YAML properties | `key:: value` | YAML (required `type`, OKF); Logseq page properties read |
| Tasks | Tasks plugin emoji | `TODO`/`DONE` markers | Both, plus Dataview fields |
| Flashcards | Plugin: `::`, `?`, `#flashcards` | `#card`, `{{cloze}}` | Logseq's syntax, Anki-style decks from tags |
| Queries | Dataview plugin, Bases | Simple and Datalog queries | Dataview (planned) |
| Review history | In the note (plugin) | In the note (`card-*::`) | Per device, outside the Space |
