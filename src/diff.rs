//! Render model: turns a `FileDiff` into rows the virtualized list paints.
//! Pure and UI-free so it can be tested and built on a background thread.

use crate::git::{FileDiff, LineKind};
use std::ops::Range;

/// Lines longer than this are truncated at render time (minified bundles).
pub const MAX_LINE_CHARS: usize = 10_000;
/// Intra-line (word) diff is skipped for lines longer than this — cost grows quadratically.
const INTRALINE_MAX: usize = 1_000;

#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    /// `@@ -a,b +c,d @@ context`, hunk index.
    Hunk(usize),
    /// Collapsed unchanged lines between hunks (or before the first / after the last).
    Gap { hidden: u32 },
    /// Unified row: index into `FileDiff::lines`.
    Line { idx: usize, emph: Vec<Range<usize>> },
    /// Split row: left (old) and right (new) cells.
    Pair { left: Option<Cell>, right: Option<Cell> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub idx: usize,
    /// Byte ranges (relative to the line text) to emphasise.
    pub emph: Vec<Range<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    #[default]
    Unified,
    Split,
}

pub struct Rows {
    pub rows: Vec<Row>,
    /// Row index of each hunk header, for `n`/`p` navigation and scrollbar markers.
    pub hunk_rows: Vec<usize>,
}

pub fn build(diff: &FileDiff, layout: Layout) -> Rows {
    let mut rows = Vec::with_capacity(diff.lines.len() + diff.hunks.len() * 2);
    let mut hunk_rows = Vec::with_capacity(diff.hunks.len());
    let mut prev_old_end: u32 = 1;
    for (h_idx, hunk) in diff.hunks.iter().enumerate() {
        let start = hunk.old_start.max(1);
        let hidden = if hunk.old_lines == 0 && hunk.old_start == 0 { 0 } else { start.saturating_sub(prev_old_end) };
        if hidden > 0 {
            rows.push(Row::Gap { hidden });
        }
        hunk_rows.push(rows.len());
        rows.push(Row::Hunk(h_idx));
        let lines = hunk.first_line..hunk.first_line + hunk.line_count;
        emit_hunk(diff, lines, layout, &mut rows);
        prev_old_end = hunk.old_start + hunk.old_lines;
    }
    Rows { rows, hunk_rows }
}

fn emit_hunk(diff: &FileDiff, lines: Range<usize>, layout: Layout, rows: &mut Vec<Row>) {
    let mut i = lines.start;
    while i < lines.end {
        let kind = diff.lines[i].kind;
        if kind == LineKind::Context {
            rows.push(match layout {
                Layout::Unified => Row::Line { idx: i, emph: vec![] },
                Layout::Split => {
                    Row::Pair { left: Some(Cell { idx: i, emph: vec![] }), right: Some(Cell { idx: i, emph: vec![] }) }
                }
            });
            i += 1;
            continue;
        }
        // A change block: run of removals followed by run of additions.
        let del_start = i;
        while i < lines.end && diff.lines[i].kind == LineKind::Removed {
            i += 1;
        }
        let add_start = i;
        while i < lines.end && diff.lines[i].kind == LineKind::Added {
            i += 1;
        }
        let dels: Vec<usize> = (del_start..add_start).collect();
        let adds: Vec<usize> = (add_start..i).collect();
        let paired = dels.len().min(adds.len());
        let mut del_emph = vec![Vec::new(); dels.len()];
        let mut add_emph = vec![Vec::new(); adds.len()];
        for k in 0..paired {
            let (a, b) = intraline(diff.line_text(&diff.lines[dels[k]]), diff.line_text(&diff.lines[adds[k]]));
            del_emph[k] = a;
            add_emph[k] = b;
        }
        match layout {
            Layout::Unified => {
                for (k, idx) in dels.iter().enumerate() {
                    rows.push(Row::Line { idx: *idx, emph: std::mem::take(&mut del_emph[k]) });
                }
                for (k, idx) in adds.iter().enumerate() {
                    rows.push(Row::Line { idx: *idx, emph: std::mem::take(&mut add_emph[k]) });
                }
            }
            Layout::Split => {
                for k in 0..dels.len().max(adds.len()) {
                    rows.push(Row::Pair {
                        left: dels.get(k).map(|&idx| Cell { idx, emph: std::mem::take(&mut del_emph[k]) }),
                        right: adds.get(k).map(|&idx| Cell { idx, emph: std::mem::take(&mut add_emph[k]) }),
                    });
                }
            }
        }
        if i == del_start {
            // Defensive: unknown kind, avoid infinite loop.
            i += 1;
        }
    }
}

/// Word-level diff of a removed/added line pair. Returns emphasised byte ranges for each side.
/// Returns empty ranges when the lines are too long or too different to be useful.
pub fn intraline(old: &str, new: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    if old.len() > INTRALINE_MAX || new.len() > INTRALINE_MAX {
        return (vec![], vec![]);
    }
    let ot = tokenize(old);
    let nt = tokenize(new);
    let ow: Vec<&str> = ot.iter().map(|r| &old[r.clone()]).collect();
    let nw: Vec<&str> = nt.iter().map(|r| &new[r.clone()]).collect();
    let diff = similar::capture_diff_slices(similar::Algorithm::Myers, &ow, &nw);
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut same = 0usize;
    for op in diff {
        match op {
            similar::DiffOp::Equal { old_index, len, .. } => {
                same += ot[old_index..old_index + len].iter().map(|r| r.len()).sum::<usize>();
            }
            similar::DiffOp::Delete { old_index, old_len, .. } => {
                push_span(&mut a, &ot[old_index..old_index + old_len])
            }
            similar::DiffOp::Insert { new_index, new_len, .. } => {
                push_span(&mut b, &nt[new_index..new_index + new_len])
            }
            similar::DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                push_span(&mut a, &ot[old_index..old_index + old_len]);
                push_span(&mut b, &nt[new_index..new_index + new_len]);
            }
        }
    }
    // If less than 40% of the text is shared the highlight is noise: whole line already tinted.
    let longest = old.len().max(new.len()).max(1);
    if same * 10 < longest * 4 {
        return (vec![], vec![]);
    }
    (a, b)
}

fn push_span(out: &mut Vec<Range<usize>>, toks: &[Range<usize>]) {
    if let (Some(first), Some(last)) = (toks.first(), toks.last()) {
        let r = first.start..last.end;
        match out.last_mut() {
            Some(prev) if prev.end == r.start => prev.end = r.end,
            _ => out.push(r),
        }
    }
}

/// Splits into word, whitespace and single-punctuation tokens (byte ranges).
fn tokenize(s: &str) -> Vec<Range<usize>> {
    #[derive(PartialEq, Clone, Copy)]
    enum C {
        Word,
        Space,
        Punct,
    }
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            C::Word
        } else if c.is_whitespace() {
            C::Space
        } else {
            C::Punct
        }
    };
    let mut out = Vec::new();
    let mut start = 0;
    let mut prev: Option<C> = None;
    for (i, ch) in s.char_indices() {
        let c = class(ch);
        if let Some(p) = prev {
            if p != c || c == C::Punct {
                out.push(start..i);
                start = i;
            }
        }
        prev = Some(c);
    }
    if start < s.len() {
        out.push(start..s.len());
    }
    out
}

/// Soft-wrap layout: logical rows split into equal-height visual rows (monospace font, so a
/// column count fully determines where lines break).
pub struct Wrapped {
    pub cols: usize,
    /// Visual row → (logical row, segment within it).
    pub map: Vec<(u32, u32)>,
    /// Logical row → first visual row; has one extra trailing entry (= total visual rows).
    pub starts: Vec<usize>,
}

impl Wrapped {
    pub fn segments(&self, logical: usize) -> usize {
        self.starts[logical + 1] - self.starts[logical]
    }
}

/// Exactly what a diff row draws for a line: truncated, tabs expanded, plus the
/// "… N more chars" suffix. Wrapping and rendering both work on this string.
pub fn display_text(s: &str) -> String {
    let (shown, hidden) = truncate(s);
    let mut out = shown.replace('\t', "    ");
    if hidden > 0 {
        out.push_str(&format!("  … {hidden} more chars"));
    }
    out
}

/// Char count on screen (see `display_text`).
pub fn display_chars(s: &str) -> usize {
    display_text(s).chars().count()
}

/// Soft-wrap break points: char index where each visual segment starts (first is 0).
/// Breaks after the last space that fits; words longer than a line are split hard.
pub fn wrap_breaks(s: &str, cols: usize) -> Vec<usize> {
    let cols = cols.max(1);
    let chars: Vec<char> = s.chars().collect();
    let mut starts = vec![0];
    let mut start = 0;
    while chars.len() - start > cols {
        let limit = start + cols;
        // Last whitespace inside the window → break right after it.
        let brk = (start + 1..=limit).rev().find(|&i| chars[i - 1].is_whitespace() && i < chars.len()).unwrap_or(limit);
        starts.push(brk);
        start = brk;
    }
    starts
}

pub fn wrap_rows(fd: &FileDiff, rows: &Rows, cols: usize) -> Wrapped {
    let mut map = Vec::with_capacity(rows.rows.len());
    let mut starts = Vec::with_capacity(rows.rows.len() + 1);
    let segs = |idx: usize| wrap_breaks(&display_text(fd.line_text(&fd.lines[idx])), cols).len();
    for (i, row) in rows.rows.iter().enumerate() {
        starts.push(map.len());
        let n = match row {
            Row::Line { idx, .. } => segs(*idx),
            Row::Pair { left, right } => {
                let l = left.as_ref().map(|c| segs(c.idx)).unwrap_or(1);
                let r = right.as_ref().map(|c| segs(c.idx)).unwrap_or(1);
                l.max(r)
            }
            Row::Hunk(_) | Row::Gap { .. } => 1,
        };
        map.extend((0..n).map(|seg| (i as u32, seg as u32)));
    }
    starts.push(map.len());
    Wrapped { cols, map, starts }
}

/// Byte range of the `seg`-th wrapped segment of `s` (empty past the end).
pub fn segment_range(s: &str, seg: usize, cols: usize) -> std::ops::Range<usize> {
    let breaks = wrap_breaks(s, cols);
    let byte_at = |ci: usize| s.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(s.len());
    match breaks.get(seg) {
        None => s.len()..s.len(),
        Some(&from) => byte_at(from)..breaks.get(seg + 1).map(|&e| byte_at(e)).unwrap_or(s.len()),
    }
}

/// Truncates display text at `MAX_LINE_CHARS` on a char boundary. Returns hidden char count.
pub fn truncate(s: &str) -> (&str, usize) {
    if s.len() <= MAX_LINE_CHARS {
        return (s, 0);
    }
    let mut end = MAX_LINE_CHARS;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (&s[..end], s[end..].chars().count())
}

/// Expands tabs to 4 spaces. When tabs were present, also returns a byte-offset map
/// (`map[old] = new`, length `s.len() + 1`) so highlight ranges can be shifted.
pub fn expand_tabs(s: &str) -> (String, Option<Vec<usize>>) {
    if !s.contains('\t') {
        return (s.to_string(), None);
    }
    let mut out = String::with_capacity(s.len() + 8);
    let mut map = vec![0; s.len() + 1];
    for (i, ch) in s.char_indices() {
        for m in map.iter_mut().skip(i).take(ch.len_utf8()) {
            *m = out.len();
        }
        if ch == '\t' {
            out.push_str("    ");
        } else {
            out.push(ch);
        }
    }
    map[s.len()] = out.len();
    (out, Some(map))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::git::{DiffBody, DiffLine, Hunk};
    use std::sync::Arc;

    /// Hunk at old line 10: ctx, -a, -b, +A, ctx
    fn sample() -> FileDiff {
        let parts = [
            (LineKind::Context, Some(10), Some(10), "keep"),
            (LineKind::Removed, Some(11), None, "let a = 1;"),
            (LineKind::Removed, Some(12), None, "let b = 2;"),
            (LineKind::Added, None, Some(11), "let a = 10;"),
            (LineKind::Context, Some(13), Some(12), "tail"),
        ];
        let mut text = String::new();
        let mut lines = Vec::new();
        for (kind, o, n, t) in parts {
            let start = text.len();
            text.push_str(t);
            lines.push(DiffLine { kind, old_no: o, new_no: n, range: start..text.len() });
        }
        FileDiff {
            path: "x.rs".into(),
            body: DiffBody::Text,
            old_size: 0,
            new_size: 0,
            total_lines: 13,
            text: Arc::from(text),
            lines,
            hunks: vec![Hunk {
                old_start: 10,
                old_lines: 4,
                new_start: 10,
                new_lines: 3,
                context: "fn f()".into(),
                first_line: 0,
                line_count: 5,
            }],
            non_utf8: false,
            old_no_newline: false,
            new_no_newline: false,
            unchanged: None,
        }
    }

    #[test]
    fn unified_rows_have_leading_gap_hunk_and_lines() {
        let r = build(&sample(), Layout::Unified);
        assert_eq!(r.rows[0], Row::Gap { hidden: 9 });
        assert_eq!(r.rows[1], Row::Hunk(0));
        assert_eq!(r.hunk_rows, [1]);
        assert_eq!(r.rows.len(), 2 + 5);
        // first removal paired with the addition gets word emphasis
        match &r.rows[3] {
            Row::Line { idx: 1, emph } => assert!(!emph.is_empty()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn split_rows_pair_removals_with_additions() {
        let r = build(&sample(), Layout::Split);
        let pairs: Vec<(Option<usize>, Option<usize>)> = r
            .rows
            .iter()
            .filter_map(|row| match row {
                Row::Pair { left, right } => Some((left.as_ref().map(|c| c.idx), right.as_ref().map(|c| c.idx))),
                _ => None,
            })
            .collect();
        assert_eq!(pairs, [(Some(0), Some(0)), (Some(1), Some(3)), (Some(2), None), (Some(4), Some(4))]);
    }

    #[test]
    fn wrap_splits_long_lines_into_equal_rows() {
        let d = sample(); // lines: "keep", "let a = 1;", "let b = 2;", "let a = 10;", "tail"
        let rows = build(&d, Layout::Unified);
        let w = wrap_rows(&d, &rows, 4);
        // Word wrap at 4 cols: "let a = 1;" → "let ", "a = ", "1;" (3); "let a = 10;" → 3
        assert_eq!(w.segments(2), 1);
        assert_eq!(w.segments(3), 3);
        assert_eq!(w.map.len(), 1 + 1 + 1 + 3 + 3 + 3 + 1);
        assert_eq!(*w.starts.last().unwrap(), w.map.len());
        assert_eq!(w.map[w.starts[3] + 2], (3, 2));
    }

    #[test]
    fn wrap_split_pairs_take_the_taller_side() {
        let d = sample();
        let rows = build(&d, Layout::Split);
        let w = wrap_rows(&d, &rows, 5);
        // pair (1: "let a = 1;" → 2 rows at 5 cols, 3: "let a = 10;" → 3) → 3 visual rows
        let pair_row =
            rows.rows.iter().position(|r| matches!(r, Row::Pair { left: Some(Cell { idx: 1, .. }), .. })).unwrap();
        assert_eq!(w.segments(pair_row), 3);
    }

    #[test]
    fn segment_ranges_are_char_safe() {
        let s = "héllöwörld";
        assert_eq!(&s[segment_range(s, 0, 4)], "héll");
        assert_eq!(&s[segment_range(s, 1, 4)], "öwör");
        assert_eq!(&s[segment_range(s, 2, 4)], "ld");
        assert_eq!(&s[segment_range(s, 3, 4)], "");
        assert_eq!(display_chars("\tab"), 6);
    }

    #[test]
    fn wrap_breaks_at_word_boundaries() {
        let s = "building a Zed/VS Code-style welcome screen";
        let b = wrap_breaks(s, 20);
        let segs: Vec<&str> = (0..b.len()).map(|i| &s[segment_range(s, i, 20)]).collect();
        assert_eq!(segs, ["building a Zed/VS ", "Code-style welcome ", "screen"]);
        assert!(segs.iter().all(|x| x.chars().count() <= 20));
        // A word longer than the line is split hard.
        assert_eq!(wrap_breaks("aaaaaaaaaa", 4), [0, 4, 8]);
        assert_eq!(wrap_breaks("short", 10), [0]);
    }

    #[test]
    fn tokenize_splits_words_space_punct() {
        let s = "let x = foo(1);";
        let toks: Vec<&str> = tokenize(s).into_iter().map(|r| &s[r]).collect();
        assert_eq!(toks, ["let", " ", "x", " ", "=", " ", "foo", "(", "1", ")", ";"]);
    }

    #[test]
    fn intraline_marks_changed_word() {
        let old = "let ok = check(user.pw);";
        let new = "let ok = verify(user.pw);";
        let (a, b) = intraline(old, new);
        assert_eq!(a.iter().map(|r| &old[r.clone()]).collect::<Vec<_>>(), ["check"]);
        assert_eq!(b.iter().map(|r| &new[r.clone()]).collect::<Vec<_>>(), ["verify"]);
    }

    #[test]
    fn intraline_skips_totally_different_lines() {
        let (a, b) = intraline("alpha beta gamma", "one two three four");
        assert!(a.is_empty() && b.is_empty());
    }

    #[test]
    fn truncate_long_line() {
        let s = "x".repeat(MAX_LINE_CHARS + 5);
        let (t, hidden) = truncate(&s);
        assert_eq!(t.len(), MAX_LINE_CHARS);
        assert_eq!(hidden, 5);
    }

    #[test]
    fn expand_tabs_shifts_ranges() {
        let (s, map) = expand_tabs("\tab");
        let map = map.unwrap();
        assert_eq!(s, "    ab");
        assert_eq!(&s[map[1]..map[3]], "ab");
        assert!(expand_tabs("none").1.is_none());
    }
}
