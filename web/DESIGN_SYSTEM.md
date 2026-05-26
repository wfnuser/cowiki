# CoWiki Design System

Warm editorial aesthetic — aligned with cowiki.ai landing page.

## Color Tokens

### Surfaces
| Token | Value | Usage |
|-------|-------|-------|
| `--color-bg` | `#FDFCFA` | Page background |
| `--color-bg-secondary` | `#F8F6F2` | Cards, secondary surfaces |
| `--color-bg-hover` | `#F3F1EC` | Hover states |
| `--color-bg-active` | `#EDEBE5` | Active/pressed states |
| `--color-border` | `#E4E0D8` | Default borders |
| `--color-border-hover` | `#D4CFC5` | Hover borders |

### Text
| Token | Value | Usage |
|-------|-------|-------|
| `--color-text` | `#1F1B16` | Primary text (warm ink) |
| `--color-text-secondary` | `#4A433A` | Secondary text |
| `--color-text-tertiary` | `#8B8275` | Muted text, placeholders |

### Brand
| Token | Value | Usage |
|-------|-------|-------|
| `--color-accent` | `#C8442A` | Brand rust-red — links, highlights, CTA |
| `--color-accent-hover` | `#A8371F` | Accent hover state |

### Semantic
| Token | Value | Usage |
|-------|-------|-------|
| `--color-green` | `#2A6B5A` | Success, positive actions |
| `--color-red` | `#C8442A` | Destructive actions (same as accent) |
| `--color-amber` | `#B5651D` | Warnings |
| `--color-tag` | `#ECE8E1` | Tag/badge backgrounds |

## Typography

| Family | Variable | Usage |
|--------|----------|-------|
| Inter Variable | `--font-sans` | Body text, UI elements |
| Source Serif 4 Variable | `--font-serif` | Headlines, brand mark, page titles |

### Brand Mark
```html
<span style="font-family: var(--font-serif); font-weight: 700;">
  CoWiki<span style="color: var(--color-accent);">.</span>
</span>
```

## Component Conventions

- Use **shadcn/ui** components (`Button`, `Dialog`, `Input`, `DropdownMenu`, etc.)
- Border radius: `0.375rem` (slightly tighter than default shadcn)
- No heavy shadows — depth via borders and background tints
- Selection color: accent rust-red with cream text
- Inline code: `--color-accent` (#C8442A) on `--color-bg-hover`

## Relationship to Landing Page

The app uses a **lighter variant** of the landing page palette:
- Landing: `#EFEAE0` (warm beige) background
- App: `#FDFCFA` (warm white) background — more readable for long-form content

Both share:
- Accent: `#C8442A` (rust red)
- Ink: `#1F1B16` (warm black)
- Serif: used for brand mark and display headings
- Sans: Inter for body and UI
