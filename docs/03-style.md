# Kerf — Style Guide: *Black Metal*

Visual-designer rules (Refactoring UI school): hierarchy through systems, not taste. Constrained scales. Dark only — there is no light theme and never will be.

Mood: corpse-paint monochrome. Pure black void, bone-white type, ash greys, **one** cold accent (frost). Colour is reserved for meaning: moss = added, blood = removed, brass = modified/warning. Everything else is grey.

## 1. Typography

- **One face: JetBrains Mono** (UI *and* code). Resolved as `JetBrains Mono` → `JetBrainsMono Nerd Font Mono` → `JetBrainsMono Nerd Font` → system mono. Ligatures off in the diff (they lie about characters).
- Tabular figures always (monospace gives this for free).
- Weights: 400 regular, 500 medium (labels), 700 bold (file path in header, keywords). Never 300/800.

| Token | px | Line-height | Use |
|---|---|---|---|
| `text.micro` | 11 | 16 | Title Case section labels (`Base`, `Files`), status bar |
| `text.control` | 12 | 18 | Buttons, toggles, badges, counts, shortcut hints |
| `text.list` | 13 | 26 (row) | Sidebar rows |
| `text.code` | 14 | 22 | Diff lines, gutters |
| `text.title` | 14 bold | 22 | File header path |
| `text.display` | 22 | 30 | Empty-state headline only |

No other sizes. Section labels and tab titles are **Title Case** (`Repository`, `Base`, `Compare`, `Files`, `Commits`). Uppercase is reserved for tiny badges only (`BIN`, `GEN`, `MERGE`).

## 2. Colour tokens

All values measured for WCAG contrast on their real surface (see §6).

### Surfaces (darkest → lightest)
| Token | Hex | Job |
|---|---|---|
| `bg.void` | `#000000` | Diff pane, editor canvas |
| `bg.abyss` | `#0a0a0a` | Sidebar, status bar, titlebar |
| `bg.crypt` | `#111111` | Sticky file header, hunk header rows, popovers |
| `bg.ash` | `#1a1a1a` | Hover |
| `bg.slate` | `#222222` | Selected row, active segment |
| `line` | `#1c1c1c` | Hairlines (1px) between regions |
| `line.hi` | `#2e2e2e` | Focus/hover hairlines, input borders |

### Ink (text)
| Token | Hex | On `void` | Job |
|---|---|---|---|
| `ink.bone` | `#e8e4dc` | 16.6:1 | Primary: code, file names |
| `ink.body` | `#b3aea5` | 9.5:1 | Secondary: list rows, commit subjects |
| `ink.mute` | `#7d7870` | 4.8:1 | Metadata: SHAs, ages, line numbers, micro-labels |
| `ink.faint` | `#504c47` | 2.5:1 | Decoration only: disabled, gap dashes. Never for info. |

Demote before you promote: make metadata quieter rather than making titles louder.

### Meaning
| Token | Hex | Job |
|---|---|---|
| `accent.frost` | `#a9c4d9` | Focus ring, selected-row bar, active tab underline, primary action. **Max one filled-frost element per region.** |
| `add.fg` | `#8fc49a` | `+` glyph, `A` status, added counts |
| `add.bg` | `#0b1a0f` | Added line tint |
| `add.emph` | `#163d20` | Intra-line added word |
| `del.fg` | `#e0706c` | `−` glyph, `D` status, removed counts |
| `del.bg` | `#1f0a0a` | Removed line tint |
| `del.emph` | `#44161a` | Intra-line removed word |
| `mod.fg` | `#d4a95e` | `M` status, warnings (large-file gate) |
| `ren.fg` | `#a9c4d9` | `R` status (shares frost) |

Rule: status colour never appears without a glyph (A/M/D/R, +/−). Grayscale test must pass.

### Syntax (restrained — code stays mostly bone)
| Scope | Hex | Style |
|---|---|---|
| keyword / storage | `#c8c2b8` | bold |
| function | `#e8e4dc` | regular |
| type / class | `#b8c6d1` | regular |
| string | `#a7b89a` | regular |
| number / constant | `#d4a95e` | regular |
| comment | `#6b6760` | italic |
| punctuation / operator | `#8f8a82` | regular |
| attribute / macro | `#c9b5a0` | regular |

Syntax colours sit *on top of* add/del tints; both are tuned so contrast holds (lowest: comment on `del.bg` ≥ 3:1 — acceptable because comments are decorative-secondary).

## 3. Spacing & geometry

- **Scale (px):** 2 · 4 · 6 · 8 · 12 · 16 · 24 · 32. Nothing else.
- Related items < 8 apart; groups ≥ 16 apart.
- **Row heights:** list row 26 · diff line 22 · header 36 · titlebar 34 · status bar 24 · control 24.
- **Sidebar:** default 300, min 220, max 50% window.
- **Corners:** 0 on structure; 3px on controls/popovers/badges only.
- **Hairlines:** 1px `line`. No borders heavier than 1px except the 2px selection bar.
- **Shadows:** popovers only — `0 8px 24px rgba(0,0,0,0.8)` + 1px `line.hi` border.
- **Gutters:** diff line-number columns 6ch each (grows with digit count), 1ch sign column, 8px padding.

## 4. Iconography

Toggles use **words**, not glyphs (`Unified`, `Split`, `Ignore WS`, `Tree`, `List`, `PR Merge`, `Compare`) — symbols like `⫼ ⎵ …` render too small in JetBrains Mono. Glyphs only where universally clear and legible: `▾ ▸ ⇄ ↑ ↓ ✓`. Shortcut hints use `ink.mute` at `text.control`, never `ink.faint`. Nerd Font glyphs allowed when the Nerd Font variant is present (git branch ``, commit ``), with ASCII fallback. No icon without a tooltip.

## 4b. App icon — "Hairline kerf"

| Element | Colour | Meaning |
|---|---|---|
| Tile | `bg.void` `#000000`, 28/120 corner radius, `line.hi` edge | The canvas |
| Left bar | `del.fg` `#e0706c` | Base / old / removed |
| Right bar, dropped 10/120 | `add.fg` `#8fc49a` | Compare / new / added — the offset *is* the difference |
| Hairline between them | `accent.frost` `#a9c4d9` | The kerf — the cut |

- Master: `assets/icon/kerf.svg` (macOS grid: 824-pt artwork on a 1024 canvas). Source of truth: `examples/icon.rs`.
- Small sizes are drawn, not scaled: hairline 1.5 → 2.4 → 4 → 7 design units at ≥128 / 64 / 32 / 16 px, bars widen slightly.
- In-app: `widgets::app_icon(size)` draws the same mark with shapes (titlebar 16 px, start page 56 px).
- Never recolour, add a gradient, or put text on it.

## 5. Components

| Component | Spec |
|---|---|
| **Section label** | `text.micro`, `ink.mute`, Title Case, 8px top / 4px bottom |
| **List row** | 24h, 8px x-padding, `text.list` `ink.body`; hover `bg.ash`; selected `bg.slate` + 2px frost left bar + `ink.bone` |
| **Status glyph** | 1ch wide, coloured letter A/M/D/R/C/T, `text.control` bold |
| **Count badge** | `+42` add.fg, `−8` del.fg, `text.control`, right-aligned, tabular |
| **Picker field** | 22h, `bg.crypt`, 1px `line.hi`, 3px radius, value `ink.bone`, chevron `ink.mute`; focus → frost border |
| **Segmented toggle** | 22h, icon cells 24w, inactive `ink.mute`, active `bg.slate` + `ink.bone` |
| **Tab** | `text.control` Title Case, inactive `ink.mute`, active `ink.bone` + 1px frost underline, count in `ink.mute` |
| **Hunk header** | 20h, `bg.crypt`, `@@ … @@` `ink.mute`, section context `ink.body` |
| **Gap row** | 20h, `bg.void`, centred `⋯ N unchanged lines` `ink.faint`→hover `ink.mute` |
| **Gate card** | Centered in pane, 1px `line.hi`, 16px padding, title `mod.fg`, button frost outline |
| **Tab** (diff) | 36h, max 240w, `bg.abyss`; active `bg.crypt` + 2px frost top bar; preview tab name *italic*; close × visible on hover/active; hairline separators |
| **Chevron** | Nerd codicon `` / `` at `text.list` in `ink.mute`, 16px column; fallback `▶ ▼` |
| **Commit panel** | Collapsible header (chevron · `Commit` · SHA (click = copy) · author · age); body = subject + full message in a vertical-only scroll area, text wraps. Max height set by a 5px bottom drag handle (default 140, min 40, max 60% window), persisted. Short messages stay compact. |
| **Spinner** | 3-dot pulse in `ink.mute`, 12px |
| **Minimap** | Dedicated 18px column left of the scrollbar, `bg.void`: one 2px tick per changed row (add/del fg, sampled to ~400), frame (`ink.mute` hairlines, 6% white fill; frost while dragging) shows the visible region. Click/drag centres the view. |
| **Scrollbar** | Dedicated 12px column at the far right, `bg.abyss`, 1px `line` left edge. Opaque thumb `#666360` (3.3:1 on abyss), hover `#8f8a82`, frost while dragging, min 24px. Grab to drag or click track to jump; wheel works too. Never overlays content. |

## 6. Measured contrast (WCAG 2.2)

| Pair | Ratio | Req | ✓ |
|---|---|---|---|
| bone on void | 16.56 | 4.5 | ✓ |
| body on abyss | 8.97 | 4.5 | ✓ |
| mute on void | 4.79 | 4.5 | ✓ |
| mute on abyss | 4.52 | 4.5 | ✓ |
| mute on ash (hover) | 3.97 | 3.0 (UI) | ✓ metadata only |
| add.fg on add.bg | 9.01 | 4.5 | ✓ |
| add.fg on add.emph | 6.11 | 4.5 | ✓ |
| del.fg on del.bg | 6.06 | 4.5 | ✓ |
| del.fg on del.emph | 4.87 | 4.5 | ✓ |
| bone on add.emph | 9.62 | 4.5 | ✓ |
| bone on del.emph | 12.04 | 4.5 | ✓ |
| frost on void | 11.58 | 3.0 | ✓ |
| brass on void | 9.64 | 4.5 | ✓ |
| comment on void | 3.73 | — | decorative |

## 7. Motion

Almost none. Hover/selection: instant. Popover: 80 ms fade. Spinner only after 150 ms. Never two things animating at once.

## 8. Ship checklist (per screen)

- [ ] Squint: diff content > list > metadata, primary action obvious
- [ ] Grayscale: glyphs carry status
- [ ] All sizes/spaces/colours from tokens (no literals outside `theme.rs`)
- [ ] One frost-filled element per region max
- [ ] No light surface anywhere
- [ ] Longest-path test: middle-ellipsis + tooltip
