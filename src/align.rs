//! Side-by-side alignment for the live plain-diff editors (Meld / VS Code diff editor style).
//! Both sides get the same number of view rows; where one side has extra lines the other
//! gets filler rows, so matching lines always sit level. UI-free and unit-tested.

use similar::{capture_diff_slices_deadline, Algorithm, DiffOp};
use std::ops::Range;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// Line present on both sides, unchanged.
    Same,
    /// Removed (left) / added or changed (right).
    Changed,
    /// Placeholder: this side has no line here.
    Filler,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SideRow {
    /// Buffer line shown on this row (`None` for filler).
    pub line: Option<usize>,
    pub kind: RowKind,
    /// Byte ranges within the line to emphasise (word-level changes).
    pub emph: Vec<Range<usize>>,
}

#[derive(Debug, Clone, Default)]
pub struct Aligned {
    pub left: Vec<SideRow>,
    pub right: Vec<SideRow>,
    pub additions: usize,
    pub deletions: usize,
}

impl Aligned {
    pub fn identical(&self) -> bool {
        self.additions == 0 && self.deletions == 0
    }
    pub fn rows(&self) -> usize {
        self.left.len()
    }
    /// View row of each buffer line, per side (for caret placement / scrolling).
    pub fn row_of_line(rows: &[SideRow], line_count: usize) -> Vec<usize> {
        let mut out = vec![0; line_count];
        for (i, r) in rows.iter().enumerate() {
            if let Some(l) = r.line.filter(|l| *l < line_count) {
                out[l] = i;
            }
        }
        out
    }
    /// Runs of changed view rows: (first row, count). For the minimap.
    pub fn change_blocks(&self) -> Vec<(usize, usize, bool)> {
        let mut out: Vec<(usize, usize, bool)> = Vec::new();
        for i in 0..self.rows() {
            let (l, r) = (self.left[i].kind, self.right[i].kind);
            if l == RowKind::Same && r == RowKind::Same {
                continue;
            }
            // true = has additions (green), false = removal only (red)
            let added = r == RowKind::Changed;
            match out.last_mut() {
                Some((s, n, a)) if *s + *n == i && *a == added => *n += 1,
                _ => out.push((i, 1, added)),
            }
        }
        out
    }
}

fn same(line: usize) -> SideRow {
    SideRow { line: Some(line), kind: RowKind::Same, emph: Vec::new() }
}
fn changed(line: usize, emph: Vec<Range<usize>>) -> SideRow {
    SideRow { line: Some(line), kind: RowKind::Changed, emph }
}
fn filler() -> SideRow {
    SideRow { line: None, kind: RowKind::Filler, emph: Vec::new() }
}

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Aligns two texts line by line. `ignore_ws` compares lines with whitespace collapsed.
pub fn align(old: &[String], new: &[String], ignore_ws: bool) -> Aligned {
    let (ok, nk): (Vec<String>, Vec<String>) = if ignore_ws {
        (old.iter().map(|s| normalize(s)).collect(), new.iter().map(|s| normalize(s)).collect())
    } else {
        (old.to_vec(), new.to_vec())
    };
    let deadline = Instant::now() + Duration::from_millis(500);
    let ops = capture_diff_slices_deadline(Algorithm::Myers, &ok, &nk, Some(deadline));
    let mut a = Aligned::default();
    for op in ops {
        match op {
            DiffOp::Equal { old_index, new_index, len } => {
                for k in 0..len {
                    a.left.push(same(old_index + k));
                    a.right.push(same(new_index + k));
                }
            }
            DiffOp::Delete { old_index, old_len, .. } => {
                for k in 0..old_len {
                    a.left.push(changed(old_index + k, Vec::new()));
                    a.right.push(filler());
                }
                a.deletions += old_len;
            }
            DiffOp::Insert { new_index, new_len, .. } => {
                for k in 0..new_len {
                    a.left.push(filler());
                    a.right.push(changed(new_index + k, Vec::new()));
                }
                a.additions += new_len;
            }
            DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                for k in 0..old_len.max(new_len) {
                    let (o, n) = (old_index + k, new_index + k);
                    match (k < old_len, k < new_len) {
                        (true, true) => {
                            let (el, er) = crate::diff::intraline(&old[o], &new[n]);
                            a.left.push(changed(o, el));
                            a.right.push(changed(n, er));
                        }
                        (true, false) => {
                            a.left.push(changed(o, Vec::new()));
                            a.right.push(filler());
                        }
                        (false, true) => {
                            a.left.push(filler());
                            a.right.push(changed(n, Vec::new()));
                        }
                        (false, false) => unreachable!(),
                    }
                }
                a.deletions += old_len;
                a.additions += new_len;
            }
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Vec<String> {
        s.split('\n').map(str::to_string).collect()
    }

    fn render(a: &Aligned) -> Vec<String> {
        (0..a.rows())
            .map(|i| {
                let side = |r: &SideRow| match r.kind {
                    RowKind::Same => format!("={}", r.line.unwrap()),
                    RowKind::Changed => format!("*{}", r.line.unwrap()),
                    RowKind::Filler => "_".into(),
                };
                format!("{} {}", side(&a.left[i]), side(&a.right[i]))
            })
            .collect()
    }

    #[test]
    fn identical_texts_align_one_to_one() {
        let a = align(&v("a\nb\nc"), &v("a\nb\nc"), false);
        assert!(a.identical());
        assert_eq!(render(&a), ["=0 =0", "=1 =1", "=2 =2"]);
    }

    #[test]
    fn insertion_gets_filler_on_the_left() {
        let a = align(&v("a\nc"), &v("a\nb\nc"), false);
        assert_eq!(render(&a), ["=0 =0", "_ *1", "=1 =2"]);
        assert_eq!((a.additions, a.deletions), (1, 0));
    }

    #[test]
    fn deletion_gets_filler_on_the_right() {
        let a = align(&v("a\nb\nc"), &v("a\nc"), false);
        assert_eq!(render(&a), ["=0 =0", "*1 _", "=2 =1"]);
    }

    #[test]
    fn replacement_pairs_lines_with_word_emphasis_and_pads_the_shorter_side() {
        let a = align(&v("x\nlet a = 1;\nlet b = 2;\ny"), &v("x\nlet a = 10;\ny"), false);
        assert_eq!(render(&a), ["=0 =0", "*1 *1", "*2 _", "=3 =2"]);
        assert!(!a.right[1].emph.is_empty(), "changed pair has word emphasis");
        assert_eq!((a.additions, a.deletions), (1, 2));
    }

    #[test]
    fn both_sides_always_have_equal_rows() {
        let a = align(&v("1\n2\n3\n4"), &v("0\n2\n5\n6\n7\n4"), false);
        assert_eq!(a.left.len(), a.right.len());
    }

    #[test]
    fn ignore_whitespace_treats_reindented_lines_as_same() {
        let a = align(&v("if x {\n  y\n}"), &v("if x {\n        y\n}"), true);
        assert!(a.identical());
    }

    #[test]
    fn row_of_line_and_change_blocks() {
        let a = align(&v("a\nc"), &v("a\nb\nb2\nc"), false);
        assert_eq!(Aligned::row_of_line(&a.right, 4), [0, 1, 2, 3]);
        assert_eq!(Aligned::row_of_line(&a.left, 2), [0, 3]);
        assert_eq!(a.change_blocks(), [(1, 2, true)]);
    }

    #[test]
    fn empty_side() {
        let a = align(&v(""), &v("a\nb"), false);
        assert_eq!(a.left.len(), a.right.len());
        assert!(a.additions >= 2);
    }
}
