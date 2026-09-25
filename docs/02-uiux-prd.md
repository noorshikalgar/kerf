# Kerf — UI/UX PRD

Produced with the ui-team pipeline (product-analyst → ux-architect → interaction-designer; visual system lives in [03-style.md](03-style.md)) plus Matt Pocock's `prototype/UI` rule: judge layouts *inside the real app with real data*, never in a vacuum.

Design language: **Zed-minimal × black metal**. Pure black canvas, bone-white type, hairline structure, one cold accent (frost). No gradients, no shadows except overlays, no rounded "cards". The diff is the product; chrome must disappear.

---

## 1. Product analysis

**Persona:** expert solo dev, keyboard-first, reviews branches daily. Density > explanation. (Paradox of the Active User: they won't read helper text — they act.)

### Jobs per surface (ranked)

| Surface | Primary job | Secondary jobs |
|---|---|---|
| Sidebar · Repo | Get into the right repo in 1 action | Switch among recents; see errors |
| Sidebar · Range bar | State *what vs what* unambiguously | Swap, mode toggle, ahead/behind at a glance |
| Sidebar · Files tab | Walk every changed file in order | Filter, tree/flat, size-up via +/− |
| Sidebar · Commits tab | Walk the story commit by commit | Behind list, copy SHA, read message |
| Diff pane | Read changes fast and correctly | Split/unified, hunk nav, expand context, big-file gate |

### Edge-state table (every surface ships all rows)

| State | Repo | Files | Commits | Diff |
|---|---|---|---|---|
| Empty / day 1 | "Open a repository" + button + ⌘O hint + recents | — | — | Welcome: logo mark, 3 shortcuts |
| Loading | Spinner in row | Skeleton rows (same 24px geometry) | Skeleton rows | In-pane spinner after 150 ms; old diff stays dimmed until replaced |
| Error | Inline red row: what + "Choose another" | Inline error + Retry | Inline error + Retry | Error block in pane with cause |
| None | — | "No file changes" (identical trees) | "No commits ahead" | "Select a file" |
| Huge | — | Virtualized 10k+ | Virtualized 5k+ | Gate > threshold, virtualized always |
| Longest string | Middle-ellipsis path | Middle-ellipsis, full in tooltip | Subject end-ellipsis | Horizontal scroll or wrap |
| Stale | ⌘R refresh | Refresh keeps selection if file still present | same | Re-computes |

Gaps flagged for v1.1: working-tree diff, in-diff search, text selection/copy.

---

## 2. Information architecture

One window, one screen. No routes, no modals for primary content (master-detail beats list→page→back).

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ● ● ●   kerf — skillforge — develop … feature/login                (titlebar)│
├───────────────────────────┬──────────────────────────────────────────────────┤
│ REPO                      │ src/auth/login.rs            M  +42 −8   ⫼ ≡  ⎵  │ ← sticky file header
│ ▸ skillforge          ⌘O  │──────────────────────────────────────────────────│
│───────────────────────────│ @@ -10,7 +10,9 @@ fn login(req: Request)        │ ← hunk header
│ BASE     develop       ▾  │  10  10   let user = find(req.id)?;              │
│            ⇄              │  11     − let ok = check(user.pw);               │
│ COMPARE  feature/login ▾  │      11 + let ok = verify(&user, req.pw)?;       │
│ ⋯ 3-dot · base a1b2c3d    │      12 + audit(user.id);                        │
│ ↑6 ahead  ↓2 behind       │  12  13   if ok { … }                            │
│───────────────────────────│  ┄┄┄┄┄┄┄┄  ⋯ 48 unchanged lines  ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄  │ ← expandable gap
│ [FILES 14]  COMMITS 8     │ @@ -80,4 +83,6 @@                              │
│ filter…              ⊟ ≣  │  …                                               │
│ 14 files  +420 −88        │                                                  │
│ ▾ src/                    │                                                  │
│   ▾ auth/                 │                                                  │
│     M login.rs   +42 −8 ◀ │                                                  │
│     A token.rs   +90      │                                                  │
│   D legacy.rs        −40  │                                                  │
│ R old.rs → new.rs  92%    │                                                  │
├───────────────────────────┴──────────────────────────────────────────────────┤
│ ⎇ develop…feature/login   14 files   +420 −88   unified   ws: on    100%  │ ← status bar
└──────────────────────────────────────────────────────────────────────────────┘
```

### Start page & modes (v1.1)

Like Zed / VS Code: with no repository open the window is a **start page** (no sidebar): *Start* (Open Repository…, New Diff, Compare Files…), *Learn* (Shortcuts, PR Merge vs Compare View), *Recent* (repos; missing ones struck through, removable). The **sidebar exists only in git mode** — it appears when a repository opens and disappears when it is closed (✕ on the repo block). Plain-diff tabs never need it. The last repository still reopens on launch.

### Inventory

| Region | Purpose (≤6 words) | Parent |
|---|---|---|
| Titlebar | Repo and range identity | Window |
| Sidebar › Repo section | Choose repository | Sidebar |
| Sidebar › Range bar | Define base vs compare | Sidebar |
| Sidebar › Tabs (Files / Commits) | Choose what to walk | Sidebar |
| Sidebar › List | Walk changes | Tab |
| Diff pane › File header | Current file identity & toggles | Diff pane |
| Diff pane › Body | Read the diff | Diff pane |
| Status bar | Global state readout | Window |
| Overlay › Branch picker | Fuzzy-pick a ref | Range bar |
| Overlay › Repo picker (recents) | Switch repo | Repo section |

### Nav model
- Serial position: Repo (top, rare) → Range (top, set once) → List (middle, constant use) → Diff (right, reading). Left-to-right, top-to-bottom = the mental model *where → what vs what → which → read*.
- Two tabs only (Files, Commits). No second nav level.
- Global keys reachable from anywhere (§4).

### Flow maps

| Job | Steps | Count | Verdict |
|---|---|---|---|
| First review of a branch | Launch `kerf .` → (defaults filled) → ↓ file → read | 1–2 | ok |
| Change Compare branch | ⌘2 (focus compare) → type 3 chars → ↵ | 3 | ok |
| Swap direction | ⌘⇧S or click ⇄ | 1 | ok |
| Review commit-by-commit | ⌘⇧C (Commits tab) → ↓ → ↵ expand → ↓ file | 3–4 | ok |
| Switch repo | ⌘O dialog, or click repo row → recents → ↵ | 2 | ok |
| Open a huge file | select → gate → ↵ "Load anyway" | 2 | ok (conscious opt-in) |

No dead ends: every empty state names the next action.

---

## 3. Interaction design

### Feedback contract
- **<100 ms** — row selection highlight, toggle states, hover.
- **<150 ms** — diff pane repaints for normal files (sync-feeling). The previous diff stays on screen (dimmed to 60% after 150 ms) until the new one lands — no blank flash.
- **>150 ms** — spinner in the file header slot. Selecting something else cancels (generation counter; stale results dropped).
- **>1 s** — progress text: "Diffing 9,214 files…" in list header.
- **Errors** — inline, where the eye is (row or pane), with what + next action. Never modal, never toast-only.

### Component states (every interactive element)
default · hover (`bg.ash`) · active/pressed · focus-visible (1px frost ring) · selected (`bg.slate` + 2px frost left bar) · disabled (faint ink + tooltip with reason) · busy (inline spinner, label stays).

### Patterns

| Situation | Pattern |
|---|---|
| Choose among many refs | Popover picker anchored to field; fuzzy filter; groups Local Branches / Remote Branches / Tags / Recent Commits; SHA-prefix and commit-message search; free revision (`HEAD~3`) accepted; ↑↓↵ Esc |
| Walk a list | Roving selection; ↑↓ / j k moves *and* loads diff (no extra ↵) |
| Tree vs flat | Segmented 2-icon toggle in list header; persists |
| Unified vs split | Segmented toggle in file header (`≡` / `⫼`), key `s` |
| Gap between hunks | Dashed hairline row "⋯ 48 unchanged lines" — click / ↵ expands all, ⌥-click expands 20 |
| Huge file | In-pane gate card: size, line count, "Load anyway ↵" |
| Generated file | Collapsed by default: "Generated file · +18,204 −17,990 · Show ↵" |
| Binary | In-pane notice with old → new size |
| Viewed tracking | Row gets a faint ✓ after being shown >1 s; counter "6/14 viewed" |
| Resizable sidebar | 4px drag handle, min 220, max 50% width; ⌘B collapses |

### Keyboard map (v1)

Shortcuts are **not** shown inline (no key hints on fields/rows). The full map lives in one popup opened from **Shortcuts** in the status bar; tooltips may still mention a key.


| Key | Action |
|---|---|
| ⌘O | Open repository |
| ⌘R | Refresh refs & diff |
| ⌘1 / ⌘2 | Open Base / Compare picker |
| ⌘⇧S | Swap Base ⇄ Compare |
| ⌘⇧M | Toggle PR Merge View / Compare View |
| F1 / ? | Explain the two views (popup) |
| ⌘W | Close tab |
| ⌘⇧[ / ⌘⇧] | Previous / next tab |
| ↵ / double-click | Keep preview tab open (pin) |
| ⌘⇧F / ⌘⇧C | Files tab / Commits tab |
| ⌘B | Toggle sidebar |
| ↑ ↓ / k j | Move list selection |
| ← → | Collapse / expand tree node or commit |
| ] / [ | Next / previous file (from anywhere) |
| n / p | Next / previous hunk |
| s | Toggle split / unified |
| w | Toggle ignore-whitespace |
| z | Toggle soft wrap |
| y | Copy SHA (commit) or path (file) |
| b / c | Commits tab: set selected commit as Base / Compare |
| / | Focus list filter |
| Esc | Close popover / clear filter |

Shortcuts appear in tooltips (`Split view  s`).

### Laws applied
- **Fitts** — toggles live in the file header, right where the eye already is; list rows full-width click targets, 24 px tall.
- **Hick** — two tabs, two pickers, three toggles. Nothing else competes.
- **Doherty** — keep perceived response < 400 ms by showing the old diff while computing and virtualizing everything.
- **Jakob** — `git diff` conventions: red/green, `@@` headers, `+/-` gutters, A/M/D/R letters.
- **Peak-end** — polish the huge-file gate and the "identical branches" / "no common ancestor" states; those are the moments users remember.

### Anti-patterns (blocked)
Modal dialogs for primary content · spinners that blank the pane · wrapping button labels · colour-only status (always glyph + colour) · hover-only affordances · two animations at once · any light surface.

---

## 4. Prototype plan (Matt Pocock `prototype/UI`)

Variants live *inside the real window with a real repo loaded*, switched with a debug-only floating bar (`⌥←/⌥→`), cfg-gated to debug builds:

- **A — Sidebar tree + single diff (chosen for v1 baseline):** as wireframed above.
- **B — Stacked review:** sidebar lists files; pane shows *all* files' diffs stacked (GitHub-style) with sticky headers; selection scrolls.
- **C — Three columns:** commits column | files column | diff.

Decision rule: pick after using each on a real 50-file branch. Losing variants go to a `proto/*` branch, not `develop`. v1 ships A; B's stacked mode is a strong v1.1 candidate as a pane toggle.

---

## 5. Acceptance (design-critic + qa-reviewer gates, adapted to desktop)

- Screenshots at 1280×800, 1728×1117 and min window 900×600: no clipped controls, no wrapped labels.
- Squint test: three levels — file path/diff content > list rows > metadata.
- Grayscale test: status readable via glyphs (A/M/D/R, +/−) without colour.
- Contrast measured: body ink ≥ 4.5:1 on its surface, add/del foregrounds ≥ 4.5:1 on their tints.
- Keyboard-only pass: every action in §3 map works; focus always visible.
- 100k-line file: open < 1 s, scroll without dropped frames (manual check + frame timing log).
