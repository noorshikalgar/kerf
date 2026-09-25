# Kerf — Product Requirements (PRD)

Status: v1 draft · Owner: Noor · Date: 2026-09-25

## 1. Problem Statement

Comparing two Git branches is a daily task — "what does `feature/x` change against `develop`?", "what landed in `release` that isn't in `main`?" — and every existing tool makes it worse than it should be:

- **CLI (`git diff a...b`, `git log a..b`)** is fast and correct but linear. No overview, no jumping between files, huge files scroll forever, no side-by-side.
- **VS Code + GitLens** can do it, but the compare flow is buried in panels-inside-panels, mixes many concepts (blame, heatmaps, graph, remotes, AI), is slow on big repos and chokes on big files.
- **Web UIs (GitHub compare)** need a push, a remote and a network, and truncate large diffs ("Load diff… this file is too large").

The user wants one thing: pick a repo, pick two branches, and read the cut between them — fast, readable, native.

## 2. Solution

Kerf is a native desktop app (Rust + GPUI, Zed's GPU UI framework) that does **branch comparison and nothing else, extremely well**.

- Left **sidebar**: repo picker → two branch pickers (base ◀ compare) → a tabbed tree of *Files* changed and *Commits* in the range.
- Right **diff pane**: GPU-rendered, virtualized diff of the selected file (or the selected commit's files), unified or split, with line numbers, hunk headers, intra-line highlights and syntax colour.
- Every diff is computed off the UI thread; files of any size scroll at 120fps because only visible rows are laid out.
- Zed-style minimal chrome, **black metal** dark theme, JetBrains Mono everywhere.

## 3. Personas

| Persona | Context | Cares about |
|---|---|---|
| **P1 Reviewer-dev (primary)** — Noor | Local clone, many feature branches, reviews own/others' work before opening a PR | Speed, density, keyboard, "what changed and why" at a glance |
| **P2 Release wrangler** | Compares `release/*` vs `main` before cutting a tag | Commit list both directions, file counts, nothing missed |
| **P3 Archaeologist** | Wants to know how two long-diverged branches differ | Merge-base clarity, big diffs that don't hang |

Expert users. Optimise for speed and density, not onboarding.

## 4. Core concepts (ubiquitous language)

| Term | Meaning |
|---|---|
| **Repo** | A local Git working copy (or bare repo) opened by path. v1: local only, no fetch/push. |
| **Base** | The branch you compare *against* (left side, "old"). Default: `main`/`master`/`develop` if present. |
| **Compare (Head)** | The branch whose changes you want to see (right side, "new"). |
| **Merge base** | Best common ancestor of Base and Compare. |
| **Range mode** | `Three-dot` (default): diff `merge-base(B,C)` → `C` — "what Compare introduced". `Two-dot`: diff `B` → `C` tip-to-tip — "how the trees differ now". |
| **Ahead / Behind** | Commits reachable from Compare not Base (ahead) and vice-versa (behind). |
| **Change** | One file entry in the diff: Added, Modified, Deleted, Renamed, Copied, Type-changed; binary flag; +/− counts. |
| **Hunk** | A contiguous block of changed lines with context. |
| **Selection** | What the diff pane shows: the whole range for one file, or one commit (all its files / one file). |

## 5. User Stories

### Repository
1. As a dev, I want to open a repo via a native folder picker, so that I don't type paths.
2. As a dev, I want to open a repo by passing a path on the command line (`kerf ~/code/app`), so that I can launch from a terminal.
3. As a dev, I want recently opened repos listed in the sidebar, so that switching is one click.
4. As a dev, I want Kerf to find the repo root if I pick a sub-folder, so that I don't need to be precise.
5. As a dev, I want a clear message when the folder is not a Git repo, so that I know what went wrong.
6. As a dev, I want bare repos and worktrees to open, so that unusual setups still work.
7. As a dev, I want a repo with zero commits to show an empty state instead of crashing.

### Branch selection
8. As a dev, I want two branch pickers labelled *Base* and *Compare*, so that direction is never ambiguous.
9. As a dev, I want the pickers pre-filled (Base = default branch, Compare = current HEAD), so that the common case needs zero clicks.
10. As a dev, I want to fuzzy-filter branches by typing, so that I can find one among hundreds.
11. As a dev, I want local and remote-tracking branches (`origin/*`) listed and grouped, so that I can compare against what I last fetched.
12. As a dev, I want tags selectable too, so that I can compare `v1.2.0` to `main`.
13. As a dev, I want a swap button (⇄) to flip Base and Compare, so that I can see the other direction instantly.
14. As a dev, I want each branch shown with its tip short-SHA and last-commit age, so that I pick the right one.
15. As a dev, I want to see the merge base SHA and ahead/behind counts once both are chosen, so that I understand the relationship.
16. As a dev, I want to toggle three-dot vs two-dot mode, so that I can answer both "what did this branch add" and "how do they differ right now".
17. As a dev, I want a clear state when Base == Compare ("identical — nothing to compare").
18. As a dev, I want a clear state when branches share no history (unrelated), falling back to two-dot automatically with a notice.
19. As a dev, I want a detached HEAD to be selectable as Compare, so that I can inspect a checkout.

### Commit-to-commit comparison (v1.1)
64. As a dev, I want Base and Compare to accept any commit (picked from Recent Commits, found by SHA prefix or message, or typed as a revision like `HEAD~3`), so that I can diff two points in history, not just two branches.
65. As a dev, I want to press `b` / `c` on a commit in the Commits tab to make it Base / Compare, so that I can narrow a review to part of a branch.
66. As a dev, I want a commit shown in a field as `short-SHA  subject`, so that I know which point in history I'm looking at.

### Plain diff, outside git (v1.1)
67. As a dev, I want to paste two snippets into Left and Right panes and see their diff, so that I can compare config, logs or API responses without saving files.
68. As a dev, I want to pick any two files (dialog or `kerf a b`) and diff them, so that I can compare files that aren't in a repo.
69. As a dev, I want plain diffs to open as tabs with the same diff view (split, syntax, minimap), so that there's one tool for every diff.

### Files view (sidebar tab 1)
20. As a dev, I want every changed file listed with its status glyph (A/M/D/R) and +/− counts, so that I can size up the change.
21. As a dev, I want files shown as a collapsible directory tree or a flat list (toggle), so that I can scan by area or by name.
22. As a dev, I want to filter files by path substring, so that I can jump to `auth/`.
23. As a dev, I want renamed files shown as `old → new` with similarity %, so that renames don't look like delete+add.
24. As a dev, I want binary files marked and not diffed as text, so that nothing garbles.
25. As a dev, I want a summary header "N files · +X −Y", so that I know the total size.
26. As a dev, I want ↑/↓ to move selection and the diff to follow instantly, so that I can read a whole change set with one hand.
27. As a dev, I want `]` / `[` to jump to next/previous file from inside the diff pane.
28. As a dev, I want files I've viewed to be marked (dimmed tick), so that I know my progress in a long review.

### Commits view (sidebar tab 2)
29. As a dev, I want commits *ahead* (in Compare, not Base) listed newest-first with subject, author, age, short-SHA.
30. As a dev, I want commits *behind* (in Base, not Compare) in a separate collapsible group, so that I see what I'm missing.
31. As a dev, I want to click a commit and see only that commit's changes, so that I can review commit-by-commit.
32. As a dev, I want a selected commit to expand into its changed files, so that I can drill into one file of one commit.
33. As a dev, I want merge commits marked and diffed against their first parent, so that they aren't noise.
34. As a dev, I want to copy a commit SHA with one keystroke (`y`), so that I can paste it elsewhere.
35. As a dev, I want the full commit message visible for the selected commit, so that I know the intent.

### Diff pane
36. As a dev, I want a unified view with old/new line-number gutters, so that it reads like `git diff` but nicer.
37. As a dev, I want a split (side-by-side) view, toggled with one key, so that I can compare structurally.
38. As a dev, I want added/removed lines tinted and intra-line (word) changes emphasised, so that small edits pop.
39. As a dev, I want syntax highlighting by file extension, so that code is readable.
40. As a dev, I want hunk headers with the enclosing function/section line, so that I know where I am.
41. As a dev, I want `n` / `p` to jump to next/previous hunk.
42. As a dev, I want collapsed unchanged regions between hunks that I can expand, so that I can get context on demand.
43. As a dev, I want a sticky file header (path, status, +/−) while scrolling, so that I never lose track.
44. As a dev, I want to toggle "ignore whitespace", so that reformatting noise disappears.
45. As a dev, I want long lines to either soft-wrap or scroll horizontally (toggle), so that minified code stays readable.
46. As a dev, I want to select and copy text from the diff, so that I can quote code. *(v1.1)*
47. As a dev, I want `/` to search inside the current diff. *(v1.1)*
48. As a dev, I want a new/deleted file shown as all-green/all-red with a single gutter.
49. As a dev, I want a mode-only change (e.g. 644→755) shown as a notice, not an empty diff.
50. As a dev, I want a scrollbar minimap marking hunk positions, so that I see the change shape of a long file.

### Large repos & large files
51. As a dev, I want a 100k-line file diff to open and scroll smoothly, so that big generated files don't freeze the app.
52. As a dev, I want a 10k-file change set to list without lag.
53. As a dev, I want diffs over a size threshold (default 20 MB or 200k lines) to show a "Large diff — load anyway?" gate, so that I opt in consciously.
54. As a dev, I want lines longer than 10k chars truncated with a marker (expand on click), so that minified bundles don't stall layout.
55. As a dev, I want lockfiles and generated files (`*.lock`, `package-lock.json`, `*.min.js`, `dist/`) collapsed by default with a one-click expand.
56. As a dev, I want every git operation cancellable by just selecting something else — stale results are dropped.
57. As a dev, I want a visible spinner in-place when a diff takes >150 ms, never a frozen window.

### App & keyboard
58. As a dev, I want every action reachable by keyboard, with shortcuts shown in tooltips.
59. As a dev, I want a command palette (⌘K / ⌘⇧P). *(v1.1)*
60. As a dev, I want the sidebar resizable and collapsible (⌘B).
61. As a dev, I want Kerf to remember last repo, branches, view mode and sidebar width across restarts.
62. As a dev, I want a refresh (⌘R) that re-reads refs, since I may have committed in a terminal.
63. As a dev, I want the window title to show `repo — base…compare`.

## 6. Scenarios (end-to-end)

**S1 — Pre-PR self review (P1, daily).** Launch `kerf .` in repo on `feature/login`. Base auto = `develop`, Compare = `feature/login`. Sidebar: "14 files · +420 −88 · 6 ahead · 2 behind". ↓ through files, `s` to split view on the gnarly one, `n` through hunks, notices a debug print, fixes it in editor, ⌘R — list updates, that file's counts change.

**S2 — Commit-by-commit review (P1).** Switch to *Commits* tab, select oldest ahead commit, read its message, ↓ to next commit. Expand commit → pick single file.

**S3 — Release audit (P2).** Base = `main`, Compare = `release/2.4`. Reads "Behind: 3" group — hotfixes on main not in release — copies SHAs with `y` for cherry-picking.

**S4 — Monster lockfile (P1).** Change set contains `pnpm-lock.yaml` (+18k lines). It's collapsed by default ("generated · +18,204 −17,990"). User expands; the virtualized view opens in <300 ms and scrolls smoothly.

**S5 — Minified bundle (P3).** `app.min.js`, one 4 MB line. Line truncated at 10k chars with "… 3,990,000 more chars" marker; UI stays responsive.

**S6 — Diverged branches (P3).** Compare two branches with 2,000 commits either side and 9,000 changed files. File list appears progressively; first file selectable before full stats are computed.

**S7 — Unrelated histories.** `gh-pages` vs `main`: no merge base → notice "No common ancestor — showing two-dot diff", mode toggle disabled.

**S8 — Not a repo / broken repo.** User picks `~/Downloads`. Inline error in sidebar: "Not a Git repository: ~/Downloads" + "Choose another folder" button. Recents entry for a deleted repo is shown struck-through with "Remove".

**S9 — Binary & special files.** PNG changed → "Binary file changed (12.4 KB → 13.1 KB)". Submodule pointer changed → shows old→new SHA. Symlink → shows target change.

**S10 — Rename with edits.** `src/auth.rs → src/auth/mod.rs (92%)` listed as rename; diff shows only the 8% that changed.

## 7. Edge cases matrix

| Case | Expected |
|---|---|
| Empty repo (no commits) | Empty state: "No commits yet" |
| Only one branch | Compare picker shows it; Base picker offers tags/commits; hint to create a branch |
| Base == Compare | "Identical — nothing to compare" |
| No merge base | Auto two-dot + notice |
| Branch deleted while open | On refresh: picker shows "(missing)", prompt to re-pick |
| Repo deleted while open | Error state, repo removed from active, kept in recents struck-through |
| File > threshold | Gate "Large diff (38 MB) — Load anyway" |
| Line > 10k chars | Truncate + expand marker |
| Binary | No text diff; show sizes |
| Non-UTF-8 text | Lossy decode, badge "non-UTF-8" |
| CRLF vs LF only | Notice "Line endings changed" with ignore-whitespace hint |
| Mode-only change | Notice "Mode 100644 → 100755" |
| Submodule | Old → new commit SHA |
| 10k+ files | Virtualized list; filter stays <16 ms per keystroke |
| 5k+ commits | Virtualized list |
| Very deep paths | Middle-ellipsis in list, full path in tooltip + header |
| Diff in flight, user selects other file | Previous job result discarded (generation counter) |

## 8. Functional requirements (v1)

- **FR1** Open repo (dialog, CLI arg, recents). Discover root.
- **FR2** List refs: local branches, remote branches, tags; HEAD indicator.
- **FR3** Base/Compare pickers with fuzzy filter, swap, defaults.
- **FR4** Compute merge base, ahead/behind lists, range mode (3-dot/2-dot).
- **FR5** Changed file list with status, rename detection, +/− stats, binary flag; tree & flat views; path filter.
- **FR6** Commit list (ahead/behind) with metadata; per-commit file list and diffs.
- **FR7** File diff: unified + split, line numbers, hunk headers, intra-line highlight, syntax highlight, ignore-whitespace, wrap toggle.
- **FR8** Large-file handling: background compute, virtualized render, size gate, long-line truncation, generated-file collapse.
- **FR9** Keyboard map for all primary actions.
- **FR10** Persist session state (last repo, branches, modes, sidebar width, recents).

## 9. Non-functional requirements

| NFR | Target |
|---|---|
| Cold start to window | < 400 ms (release build) |
| Open repo → file list (1k changed files) | < 500 ms |
| Select file → first paint of diff (≤ 5k lines) | < 100 ms |
| 100k-line diff open | < 1 s; scroll at display refresh rate |
| Memory for 100k-line diff | < 150 MB |
| UI thread | Never blocks on git or diff work |
| Platform | macOS (Apple Silicon) v1; Linux v1.x; Windows later |
| Safety | Read-only. Kerf never writes to the repo (no checkout, no index changes) |

## 10. Out of scope (v1)

Remotes (fetch/pull/push), staging/committing, working-tree/uncommitted diff (v1.1 candidate), blame, graph visualisation, merge-conflict resolution, editing files, light theme, AI summaries, PR integration, plugins.

## 11. Release plan

| Milestone | Content |
|---|---|
| **M0 Shell** | GPUI window, theme, font, sidebar + pane layout |
| **M1 Git core** | Repo open, refs, merge base, ahead/behind, changed files, file diff model — fully tested |
| **M2 Branch diff (MVP)** | Sidebar pickers, files tab, commits tab, unified diff, large-file handling |
| **M3 Rich diff** | Split view, intra-line, syntax highlighting, hunk nav, whitespace toggle |
| **M4 Polish** | Persistence, recents, palette, search, copy |

Success metric: Noor uses Kerf instead of `git diff`/GitLens for branch reviews for two weeks straight.
