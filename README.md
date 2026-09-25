# Kerf

> The kerf is the slit a saw blade leaves behind. Kerf shows you the cut between two branches.

A fast, native, keyboard-first **Git branch diff viewer** built in Rust on [GPUI](https://www.gpui.rs/) (Zed's GPU UI framework). Black Metal dark theme, JetBrains Mono, read-only.

## Features (v0.1)

- Open any local repo (`kerf <path>`, ⌘O, or recents) — root auto-discovered
- Pick **Base** and **Compare** from a fuzzy picker (local, remote, tags) — defaults to `main`/`develop` vs current branch
- Three-dot (what Compare introduced) or two-dot (tip-to-tip) — auto two-dot for unrelated histories
- Merge base, ahead / behind counts
- **Files** tab: tree or flat, status glyphs, +/− counts, rename similarity, binary/generated badges, filter, viewed ticks
- **Commits** tab: ahead + behind groups, expand a commit to walk its own files (merges vs first parent)
- Diff pane: unified or split, word-level emphasis, syntax colour, hunk headers with function context, collapsed gaps (click to expand), hunk minimap
- Big files: diffs computed off the UI thread, virtualized rendering, 20 MB gate, 10k-char line truncation, lockfiles/`*.min.*`/`dist/` collapsed by default
- 100k-line file diff → ~150 ms (release)

## Build & run

```bash
cargo run --release -- ~/path/to/repo
```

Requires Rust 1.85+ on macOS. GPUI is built with `runtime_shaders`, so the Xcode Metal toolchain is not needed.

## Keys

| Key | Action | Key | Action |
|---|---|---|---|
| ⌘O | open repo | ↑ ↓ / j k | move selection (diff follows) |
| ⌘R | refresh | ← → / h l | collapse / expand |
| ⌘1 / ⌘2 | pick base / compare | ] [ | next / previous file |
| ⌘⇧S | swap base ⇄ compare | n p | next / previous hunk |
| ⌘⇧M | 3-dot / 2-dot | s | split / unified |
| ⌘⇧F / ⌘⇧C | files / commits tab | w | ignore whitespace |
| ⌘B | toggle sidebar | t | tree / flat |
| / | filter files | y | copy path / SHA |
| space / ⇧space | page down / up | g / G | top / bottom |

## Docs

Planning docs live in [`docs/`](docs/): PRD, UI/UX PRD, style guide, architecture, workflow.

## Tests

```bash
cargo test                                              # unit + git fixtures
cargo test --release --test perf -- --ignored --nocapture   # 100k-line timing
```
