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
                Layout::Split => Row::Pair {
                    left: Some(Cell { idx: i, emph: vec![] }),
                    right: Some(Cell { idx: i, emph: vec![] }),
                },
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
            similar::DiffOp::Delete { old_index, old_len, .. } => push_span(&mut a, &ot[old_index..old_index + old_len]),
            similar::DiffOp::Insert { new_index, new_len, .. } => push_span(&mut b, &nt[new_index..new_index + new_len]),
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

/// Expands tabs to 4 spaces, keeping emphasis ranges aligned.
pub fn expand_tabs(s: &str, emph: &[Range<usize>]) -> (String, Vec<Range<usize>>) {
    if !s.contains('\t') {
        return (s.to_string(), emph.to_vec());
    }
    let mut out = String::with_capacity(s.len() + 8);
    let mut map = Vec::with_capacity(s.len() + 1);
    for (i, ch) in s.char_indices() {
        while map.len() <= i {
            map.push(out.len());
        }
        if ch == '\t' {
            out.push_str("    ");
        } else {
            out.push(ch);
        }
    }
    while map.len() <= s.len() {
        map.push(out.len());
    }
    let emph = emph.iter().map(|r| map[r.start]..map[r.end.min(s.len())]).collect();
    (out, emph)
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
            hunks: vec![Hunk { old_start: 10, old_lines: 4, new_start: 10, new_lines: 3, context: "fn f()".into(), first_line: 0, line_count: 5 }],
            non_utf8: false,
            old_no_newline: false,
            new_no_newline: false,
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
        let (s, e) = expand_tabs("\tab", &vec![1..3]);
        assert_eq!(s, "    ab");
        assert_eq!(&s[e[0].clone()], "ab");
    }
}
