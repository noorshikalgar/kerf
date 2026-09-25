# Kerf — Testing

All tests are headless; none need a display or control of the machine.

| Layer | Where | What |
|---|---|---|
| Git core | `src/git/tests.rs` | Real temp repos built with the git CLI: ranges (PR Merge / Compare), unrelated histories, renames, binary, large-file gate, whitespace, no-newline, commit ranges, buffer diffs, identical content |
| Diff model | `src/diff.rs` | Unified / split rows, gaps, word emphasis, wrap layout, word-boundary breaks, UTF-8-safe segments |
| Alignment | `src/align.rs` | Fillers, replace pairing, empty sides, ignore-ws — plus a **property test** over 800 random text pairs (equal rows, every line once and in order, counts, identical) |
| Editor buffer | `src/buffer.rs` | Editing rules, selection, undo/redo, line operations — plus a **property test**: 400 × 60 random edits checked against a plain string model, undo restores the start |
| Highlighting | `src/highlight.rs` | Scope mapping, multi-line state |
| Keymap | `src/ui/mod.rs` | gpui's real `Keymap` resolution: editor keys beat app keys; single-letter app keys never fire while an editor is focused |
| **UI integration** | `src/ui/tests_ui.rs` | Real Kerf windows on gpui's test platform, driven by **real keystrokes through the app keymap** against real repos: default range, errors, views, swap, refresh, navigation, split / whitespace, tabs (preview / pin / cycle / close), filter, commits tab, commit-as-base, picker, large-file gate, live diff editor (typing, Enter, undo, empty side, identical, files untouched), close repo, start page |
| Performance | `tests/perf.rs` (`--ignored`, release) | 100k-line file diff, rows, wrap of 100k rows, **exact** 50k-line live alignment |

Unit tests never read or write the user's saved state (`Persisted` is a no-op under `cfg(test)`; `KERF_STATE_DIR` redirects it otherwise).

```bash
cargo test
```

```bash
cargo test --release --test perf -- --ignored --nocapture
```

Gates (also in CI): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.

## Bugs found by this suite
- First letter typed into a new editor could trigger an app shortcut (`h` = collapse) → single-letter bindings now use `Kerf && !KerfEditor`.
- Pasting an empty clipboard created an empty undo step.
- Live alignment of large texts hit its time budget and fell back to a coarse diff (+31k/−31k instead of +7k/−7k) → lines interned to integers, background path gets a 5 s budget.
