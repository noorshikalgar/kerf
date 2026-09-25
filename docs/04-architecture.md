# Kerf — Architecture

## Stack
| Concern | Choice | Why |
|---|---|---|
| UI | `gpui` (Zed's GPU UI framework, crates.io) | Native, GPU text, `uniform_list` virtualization, Zed look |
| Git | `git2` (libgit2), `default-features = false` | Merge-base, revwalk, tree-to-tree diff, rename detection; no OpenSSL (local only) |
| Syntax | `syntect` (default-fancy off, onig-free `regex-fancy`) | Mature grammars, per-line incremental state |
| Diff (intra-line) | `similar` | Word/char-level diff for changed line pairs |
| State | `serde` + `serde_json` in `~/Library/Application Support/kerf/state.json` | Recents, last range, toggles |

## Modules (deep modules, thin seams)
```
kerf/
  src/
    main.rs          – boot: fonts, theme, window, CLI arg
    theme.rs         – ALL colour/size tokens (03-style.md). No literals elsewhere.
    git/             – pure, UI-free, fully unit-tested
      mod.rs         – Repo facade: open, refs, range, changes, commit list, file diff
      model.rs       – RefInfo, RangeSpec, RangeSummary, Change, CommitInfo, FileDiff, Hunk, Line
    diff/            – render model: rows for unified/split, gaps, intra-line spans, highlight
    ui/
      app.rs         – root view: layout, key bindings, async job orchestration
      sidebar.rs     – repo section, range bar, tabs, lists
      picker.rs      – fuzzy ref picker popover
      diff_view.rs   – virtualized diff renderer
```
The `git` module is the one test seam: a temp repo is built with `git2` in tests; every scenario in PRD §6–7 that doesn't need pixels is asserted there.

## Threading
- UI thread owns state. Every git/diff call runs on `cx.background_executor()`.
- Each request carries a **generation** number; results with a stale generation are dropped (cancel-by-supersede).
- `git2::Repository` is opened per job (cheap) — no cross-thread sharing.

## Large-file strategy
1. Blob sizes checked first (`odb` header read) → over threshold → gate, no content loaded.
2. Diff produced as compact line records: `(kind, old_no, new_no, byte_range)` into one shared `Arc<str>` buffer — no per-line `String`.
3. Render with `uniform_list`: only visible rows (≈60) are shaped per frame.
4. Lines > 10k chars truncated at render time with an expand marker.
5. Syntax highlighting computed lazily in background in chunks; rows render plain until their chunk is ready. Skipped entirely > 50k lines.
6. Generated files (lockfiles, `*.min.*`, `dist/`) start collapsed.

## Read-only guarantee
Kerf never calls anything that writes refs, index or working tree.
