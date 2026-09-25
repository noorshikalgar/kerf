//! Plain-text editing model for the scratch editors: lines, a caret, an optional selection
//! anchor, and undo/redo. UI-free so every editing rule is unit-tested.
//!
//! Positions are `(row, col)` with `col` counted in chars (not bytes).

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Pos {
    pub row: usize,
    pub col: usize,
}

impl Pos {
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
    PageUp(usize),
    PageDown(usize),
}

#[derive(Clone)]
struct Snapshot {
    lines: Vec<String>,
    caret: Pos,
}

const UNDO_DEPTH: usize = 200;

pub struct Buffer {
    lines: Vec<String>,
    caret: Pos,
    /// Selection is anchor..caret when set and different from the caret.
    anchor: Option<Pos>,
    /// Remembered column for vertical moves through short lines.
    goal_col: Option<usize>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Bumped on every text change; lets views detect edits cheaply.
    pub version: u64,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new("")
    }
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn byte_at(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map(|(b, _)| b).unwrap_or(s.len())
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        let mut b = Self {
            lines: vec![String::new()],
            caret: Pos::default(),
            anchor: None,
            goal_col: None,
            undo: Vec::new(),
            redo: Vec::new(),
            version: 0,
        };
        b.lines = split_lines(text);
        b
    }

    // ── reading ──

    pub fn lines(&self) -> &[String] {
        &self.lines
    }
    pub fn line(&self, row: usize) -> &str {
        &self.lines[row]
    }
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
    pub fn caret(&self) -> Pos {
        self.caret
    }
    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
    /// Ordered selection, or `None` when nothing is selected.
    pub fn selection(&self) -> Option<(Pos, Pos)> {
        let a = self.anchor?;
        if a == self.caret {
            return None;
        }
        Some(if a < self.caret { (a, self.caret) } else { (self.caret, a) })
    }
    pub fn selected_text(&self) -> Option<String> {
        let (s, e) = self.selection()?;
        Some(self.slice(s, e))
    }
    fn slice(&self, s: Pos, e: Pos) -> String {
        if s.row == e.row {
            let l = &self.lines[s.row];
            return l[byte_at(l, s.col)..byte_at(l, e.col)].to_string();
        }
        let mut out = String::new();
        let first = &self.lines[s.row];
        out.push_str(&first[byte_at(first, s.col)..]);
        for r in s.row + 1..e.row {
            out.push('\n');
            out.push_str(&self.lines[r]);
        }
        out.push('\n');
        let last = &self.lines[e.row];
        out.push_str(&last[..byte_at(last, e.col)]);
        out
    }
    /// Longest line in chars (for horizontal sizing).
    pub fn widest_row(&self) -> usize {
        (0..self.lines.len()).max_by_key(|&r| self.lines[r].len()).unwrap_or(0)
    }

    // ── caret / selection ──

    fn clamp(&self, p: Pos) -> Pos {
        let row = p.row.min(self.lines.len() - 1);
        Pos { row, col: p.col.min(char_len(&self.lines[row])) }
    }

    /// Places the caret (e.g. from a click). `extend` keeps/starts a selection.
    pub fn set_caret(&mut self, p: Pos, extend: bool) {
        let p = self.clamp(p);
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = p;
        self.goal_col = None;
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(Pos::default());
        let last = self.lines.len() - 1;
        self.caret = Pos::new(last, char_len(&self.lines[last]));
    }

    pub fn move_caret(&mut self, m: Move, extend: bool) {
        // Collapsing a selection with ←/→ jumps to its edge, like every editor.
        if !extend {
            if let Some((s, e)) = self.selection() {
                match m {
                    Move::Left => {
                        self.set_caret(s, false);
                        return;
                    }
                    Move::Right => {
                        self.set_caret(e, false);
                        return;
                    }
                    _ => {}
                }
            }
        }
        let c = self.caret;
        let len = |r: usize| char_len(&self.lines[r]);
        let vertical = |rows: isize, this: &Self| -> Pos {
            let goal = this.goal_col.unwrap_or(c.col);
            let row = (c.row as isize + rows).clamp(0, this.lines.len() as isize - 1) as usize;
            if row == c.row {
                // Top/bottom edge: go to line start/end.
                return if rows < 0 { Pos::new(0, 0) } else { Pos::new(row, len(row)) };
            }
            Pos::new(row, goal.min(len(row)))
        };
        let next = match m {
            Move::Left if c.col > 0 => Pos::new(c.row, c.col - 1),
            Move::Left if c.row > 0 => Pos::new(c.row - 1, len(c.row - 1)),
            Move::Left => c,
            Move::Right if c.col < len(c.row) => Pos::new(c.row, c.col + 1),
            Move::Right if c.row + 1 < self.lines.len() => Pos::new(c.row + 1, 0),
            Move::Right => c,
            Move::Up => vertical(-1, self),
            Move::Down => vertical(1, self),
            Move::PageUp(n) => vertical(-(n as isize), self),
            Move::PageDown(n) => vertical(n as isize, self),
            Move::LineStart => {
                // Toggle between first non-blank and column 0.
                let indent = self.lines[c.row].chars().take_while(|c| c.is_whitespace()).count();
                Pos::new(c.row, if c.col == indent { 0 } else { indent })
            }
            Move::LineEnd => Pos::new(c.row, len(c.row)),
            Move::DocStart => Pos::new(0, 0),
            Move::DocEnd => Pos::new(self.lines.len() - 1, len(self.lines.len() - 1)),
            Move::WordLeft => self.word_left(c),
            Move::WordRight => self.word_right(c),
        };
        let keep_goal = matches!(m, Move::Up | Move::Down | Move::PageUp(_) | Move::PageDown(_));
        let goal = if keep_goal { Some(self.goal_col.unwrap_or(c.col)) } else { None };
        if extend {
            self.anchor.get_or_insert(c);
        } else {
            self.anchor = None;
        }
        self.caret = next;
        self.goal_col = goal;
    }

    fn word_left(&self, c: Pos) -> Pos {
        if c.col == 0 {
            return if c.row > 0 { Pos::new(c.row - 1, char_len(&self.lines[c.row - 1])) } else { c };
        }
        let chars: Vec<char> = self.lines[c.row].chars().collect();
        let mut i = c.col;
        while i > 0 && !is_word(chars[i - 1]) {
            i -= 1;
        }
        while i > 0 && is_word(chars[i - 1]) {
            i -= 1;
        }
        Pos::new(c.row, i)
    }

    fn word_right(&self, c: Pos) -> Pos {
        let chars: Vec<char> = self.lines[c.row].chars().collect();
        if c.col >= chars.len() {
            return if c.row + 1 < self.lines.len() { Pos::new(c.row + 1, 0) } else { c };
        }
        let mut i = c.col;
        while i < chars.len() && !is_word(chars[i]) {
            i += 1;
        }
        while i < chars.len() && is_word(chars[i]) {
            i += 1;
        }
        Pos::new(c.row, i)
    }

    // ── editing ──

    fn checkpoint(&mut self) {
        self.undo.push(Snapshot { lines: self.lines.clone(), caret: self.caret });
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn changed(&mut self) {
        self.version += 1;
        self.goal_col = None;
    }

    /// Removes the selection (no checkpoint). Returns true if something was removed.
    fn delete_selection_raw(&mut self) -> bool {
        let Some((s, e)) = self.selection() else { return false };
        let tail = {
            let last = &self.lines[e.row];
            last[byte_at(last, e.col)..].to_string()
        };
        let first = &mut self.lines[s.row];
        first.truncate(byte_at(first, s.col));
        first.push_str(&tail);
        self.lines.drain(s.row + 1..=e.row);
        self.caret = s;
        self.anchor = None;
        true
    }

    /// Types or pastes `text` at the caret, replacing any selection. Handles `\n`, `\r\n`.
    pub fn insert(&mut self, text: &str) {
        self.checkpoint();
        self.delete_selection_raw();
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let c = self.caret;
        let line = &self.lines[c.row];
        let at = byte_at(line, c.col);
        let (head, tail) = (line[..at].to_string(), line[at..].to_string());
        let mut parts = text.split('\n');
        let first = parts.next().unwrap_or("");
        let rest: Vec<&str> = parts.collect();
        if rest.is_empty() {
            self.lines[c.row] = format!("{head}{first}{tail}");
            self.caret = Pos::new(c.row, c.col + char_len(first));
        } else {
            self.lines[c.row] = format!("{head}{first}");
            let last = rest[rest.len() - 1];
            let mut new_lines: Vec<String> = rest[..rest.len() - 1].iter().map(|s| s.to_string()).collect();
            new_lines.push(format!("{last}{tail}"));
            let n = new_lines.len();
            self.lines.splice(c.row + 1..c.row + 1, new_lines);
            self.caret = Pos::new(c.row + n, char_len(last));
        }
        self.anchor = None;
        self.changed();
    }

    /// Enter: new line keeping the current line's indentation.
    pub fn newline(&mut self) {
        let indent: String = self.lines[self.caret.row].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let indent: String = indent.chars().take(self.caret.col).collect();
        self.insert(&format!("\n{indent}"));
    }

    pub fn backspace(&mut self, word: bool) {
        if self.selection().is_some() {
            self.checkpoint();
            self.delete_selection_raw();
            self.changed();
            return;
        }
        let c = self.caret;
        if c == Pos::default() {
            return;
        }
        self.anchor = Some(c);
        self.move_caret(if word { Move::WordLeft } else { Move::Left }, true);
        self.checkpoint();
        self.delete_selection_raw();
        self.changed();
    }

    pub fn delete_forward(&mut self, word: bool) {
        if self.selection().is_some() {
            self.checkpoint();
            self.delete_selection_raw();
            self.changed();
            return;
        }
        let c = self.caret;
        self.anchor = Some(c);
        self.move_caret(if word { Move::WordRight } else { Move::Right }, true);
        if self.caret == c {
            self.anchor = None;
            return;
        }
        self.checkpoint();
        self.delete_selection_raw();
        self.changed();
    }

    /// Cut: returns the removed text.
    pub fn cut(&mut self) -> Option<String> {
        let t = self.selected_text()?;
        self.checkpoint();
        self.delete_selection_raw();
        self.changed();
        Some(t)
    }

    /// Replaces everything (e.g. file loaded); undoable.
    pub fn set_text(&mut self, text: &str) {
        self.checkpoint();
        self.lines = split_lines(text);
        self.caret = Pos::default();
        self.anchor = None;
        self.changed();
    }

    // ── line operations ──

    /// Lines covered by the selection (or the caret line). A selection ending at column 0
    /// does not include that last line, like every editor.
    fn line_span(&self) -> (usize, usize) {
        match self.selection() {
            Some((s, e)) if e.col == 0 && e.row > s.row => (s.row, e.row - 1),
            Some((s, e)) => (s.row, e.row),
            None => (self.caret.row, self.caret.row),
        }
    }

    /// ⌘⌫ — delete from the caret to the start of the line (joins lines at column 0).
    pub fn delete_to_line_start(&mut self) {
        if self.selection().is_some() || self.caret.col == 0 {
            return self.backspace(false);
        }
        self.anchor = Some(Pos::new(self.caret.row, 0));
        self.checkpoint();
        self.delete_selection_raw();
        self.changed();
    }

    /// ⌘⌦ — delete from the caret to the end of the line.
    pub fn delete_to_line_end(&mut self) {
        let c = self.caret;
        let end = char_len(&self.lines[c.row]);
        if self.selection().is_some() || c.col == end {
            return self.delete_forward(false);
        }
        self.anchor = Some(Pos::new(c.row, end));
        self.checkpoint();
        self.delete_selection_raw();
        self.changed();
    }

    /// ⌘⇧⌫ — clear everything (undoable).
    pub fn clear_all(&mut self) {
        if self.is_empty() {
            return;
        }
        self.checkpoint();
        self.lines = vec![String::new()];
        self.caret = Pos::default();
        self.anchor = None;
        self.changed();
    }

    /// ⌘⇧K — delete the selected / current line(s).
    pub fn delete_lines(&mut self) {
        let (a, b) = self.line_span();
        self.checkpoint();
        if self.lines.len() == b - a + 1 {
            self.lines = vec![String::new()];
            self.caret = Pos::default();
        } else {
            self.lines.drain(a..=b);
            let row = a.min(self.lines.len() - 1);
            self.caret = Pos::new(row, self.caret.col.min(char_len(&self.lines[row])));
        }
        self.anchor = None;
        self.changed();
    }

    /// ⌘L — select the current line; again to extend down by a line.
    pub fn select_line(&mut self) {
        let (a, b) = match self.selection() {
            Some((s, e)) if s.col == 0 && e.col == 0 => (s.row, e.row),
            _ => {
                let (a, _) = self.line_span();
                (a, a)
            }
        };
        let last = self.lines.len() - 1;
        self.anchor = Some(Pos::new(a, 0));
        self.caret = if b < last { Pos::new(b + 1, 0) } else { Pos::new(last, char_len(&self.lines[last])) };
        self.goal_col = None;
    }

    /// ⌥↑ / ⌥↓ — move the selected / current line(s), keeping the selection.
    // `shift` is applied to both caret and anchor; clippy only sees the first call.
    #[allow(clippy::redundant_closure_call)]
    pub fn move_lines(&mut self, down: bool) {
        let (a, b) = self.line_span();
        if (down && b + 1 >= self.lines.len()) || (!down && a == 0) {
            return;
        }
        self.checkpoint();
        if down {
            let l = self.lines.remove(b + 1);
            self.lines.insert(a, l);
        } else {
            let l = self.lines.remove(a - 1);
            self.lines.insert(b, l);
        }
        let shift = |p: Pos| Pos::new(if down { p.row + 1 } else { p.row - 1 }, p.col);
        self.caret = shift(self.caret);
        self.anchor = self.anchor.map(shift);
        self.changed();
    }

    /// ⇧⌥↓ / ⇧⌥↑ — duplicate the selected / current line(s) below (caret follows) or above.
    // `shift` is applied to both caret and anchor; clippy only sees the first call.
    #[allow(clippy::redundant_closure_call)]
    pub fn duplicate_lines(&mut self, down: bool) {
        let (a, b) = self.line_span();
        self.checkpoint();
        let copy: Vec<String> = self.lines[a..=b].to_vec();
        let n = copy.len();
        self.lines.splice(b + 1..b + 1, copy);
        if down {
            let shift = |p: Pos| Pos::new(p.row + n, p.col);
            self.caret = shift(self.caret);
            self.anchor = self.anchor.map(shift);
        }
        self.changed();
    }

    /// ⌘] / Tab on a selection — indent the line(s) by `width` spaces.
    // `shift` is applied to both caret and anchor; clippy only sees the first call.
    #[allow(clippy::redundant_closure_call)]
    pub fn indent(&mut self, width: usize) {
        let (a, b) = self.line_span();
        self.checkpoint();
        let pad = " ".repeat(width);
        for l in &mut self.lines[a..=b] {
            l.insert_str(0, &pad);
        }
        let shift = |p: Pos| if p.row >= a && p.row <= b { Pos::new(p.row, p.col + width) } else { p };
        self.caret = shift(self.caret);
        self.anchor = self.anchor.map(shift);
        self.changed();
    }

    /// ⌘[ / ⇧Tab — outdent the line(s) by up to `width` spaces (or one tab).
    // `shift` is applied to both caret and anchor; clippy only sees the first call.
    #[allow(clippy::redundant_closure_call)]
    pub fn outdent(&mut self, width: usize) {
        let (a, b) = self.line_span();
        self.checkpoint();
        let mut removed = vec![0; b - a + 1];
        for (i, l) in self.lines[a..=b].iter_mut().enumerate() {
            let n = if l.starts_with('\t') { 1 } else { l.chars().take(width).take_while(|c| *c == ' ').count() };
            l.drain(..n);
            removed[i] = n;
        }
        let shift = |p: Pos| {
            if p.row >= a && p.row <= b {
                Pos::new(p.row, p.col.saturating_sub(removed[p.row - a]))
            } else {
                p
            }
        };
        self.caret = shift(self.caret);
        self.anchor = self.anchor.map(shift);
        self.changed();
    }

    /// True when the selection spans more than one line (Tab then indents).
    pub fn multi_line_selection(&self) -> bool {
        self.selection().is_some_and(|(s, e)| e.row > s.row)
    }

    pub fn undo(&mut self) -> bool {
        let Some(s) = self.undo.pop() else { return false };
        self.redo.push(Snapshot { lines: std::mem::replace(&mut self.lines, s.lines), caret: self.caret });
        self.caret = self.clamp(s.caret);
        self.anchor = None;
        self.changed();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(s) = self.redo.pop() else { return false };
        self.undo.push(Snapshot { lines: std::mem::replace(&mut self.lines, s.lines), caret: self.caret });
        self.caret = self.clamp(s.caret);
        self.anchor = None;
        self.changed();
        true
    }
}

fn split_lines(text: &str) -> Vec<String> {
    let text = text.replace("\r\n", "\n");
    let v: Vec<String> = text.split('\n').map(str::to_string).collect();
    if v.is_empty() { vec![String::new()] } else { v }
}

/// Display column of `col` (chars) with tabs expanded to `tab` spaces.
pub fn visual_col(line: &str, col: usize, tab: usize) -> usize {
    line.chars().take(col).map(|c| if c == '\t' { tab } else { 1 }).sum()
}

/// Inverse of `visual_col`: char column nearest to display column `vcol`.
pub fn col_from_visual(line: &str, vcol: f32, tab: usize) -> usize {
    let mut x = 0f32;
    for (i, c) in line.chars().enumerate() {
        let w = if c == '\t' { tab as f32 } else { 1. };
        if vcol < x + w / 2. {
            return i;
        }
        x += w;
    }
    char_len(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(b: &Buffer) -> (usize, usize) {
        (b.caret().row, b.caret().col)
    }

    #[test]
    fn typing_and_newlines() {
        let mut b = Buffer::new("");
        b.insert("hello");
        b.insert("\nworld");
        assert_eq!(b.text(), "hello\nworld");
        assert_eq!(at(&b), (1, 5));
    }

    #[test]
    fn insert_in_middle_and_multiline_paste() {
        let mut b = Buffer::new("abcd");
        b.set_caret(Pos::new(0, 2), false);
        b.insert("X\nY\nZ");
        assert_eq!(b.text(), "abX\nY\nZcd");
        assert_eq!(at(&b), (2, 1));
    }

    #[test]
    fn crlf_is_normalised() {
        let b = Buffer::new("a\r\nb");
        assert_eq!(b.lines(), ["a", "b"]);
    }

    #[test]
    fn backspace_joins_lines_and_deletes_words() {
        let mut b = Buffer::new("ab\ncd");
        b.set_caret(Pos::new(1, 0), false);
        b.backspace(false);
        assert_eq!(b.text(), "abcd");
        assert_eq!(at(&b), (0, 2));
        let mut w = Buffer::new("let foo_bar = 1");
        w.set_caret(Pos::new(0, 11), false);
        w.backspace(true);
        assert_eq!(w.text(), "let  = 1");
    }

    #[test]
    fn delete_forward_at_end_is_noop() {
        let mut b = Buffer::new("ab");
        b.set_caret(Pos::new(0, 2), false);
        b.delete_forward(false);
        assert_eq!(b.text(), "ab");
    }

    #[test]
    fn selection_replace_and_cut() {
        let mut b = Buffer::new("one\ntwo\nthree");
        b.set_caret(Pos::new(0, 1), false);
        b.set_caret(Pos::new(2, 2), true);
        assert_eq!(b.selected_text().unwrap(), "ne\ntwo\nth");
        b.insert("-");
        assert_eq!(b.text(), "o-ree");
        b.select_all();
        assert_eq!(b.cut().unwrap(), "o-ree");
        assert!(b.is_empty());
    }

    #[test]
    fn vertical_moves_keep_goal_column() {
        let mut b = Buffer::new("long line\nx\nanother line");
        b.set_caret(Pos::new(0, 7), false);
        b.move_caret(Move::Down, false);
        assert_eq!(at(&b), (1, 1));
        b.move_caret(Move::Down, false);
        assert_eq!(at(&b), (2, 7));
        b.move_caret(Move::Down, false); // last line → end
        assert_eq!(at(&b), (2, 12));
    }

    #[test]
    fn arrows_collapse_selection_to_edges() {
        let mut b = Buffer::new("abcdef");
        b.set_caret(Pos::new(0, 1), false);
        b.set_caret(Pos::new(0, 4), true);
        b.move_caret(Move::Left, false);
        assert_eq!(at(&b), (0, 1));
        assert!(b.selection().is_none());
    }

    #[test]
    fn word_moves() {
        let mut b = Buffer::new("foo.bar baz");
        b.move_caret(Move::WordRight, false);
        assert_eq!(at(&b), (0, 3));
        b.move_caret(Move::WordRight, false);
        assert_eq!(at(&b), (0, 7));
        b.move_caret(Move::WordLeft, false);
        assert_eq!(at(&b), (0, 4));
    }

    #[test]
    fn home_toggles_indent_and_newline_keeps_indent() {
        let mut b = Buffer::new("    code");
        b.move_caret(Move::LineEnd, false);
        b.move_caret(Move::LineStart, false);
        assert_eq!(at(&b), (0, 4));
        b.move_caret(Move::LineStart, false);
        assert_eq!(at(&b), (0, 0));
        b.move_caret(Move::LineEnd, false);
        b.newline();
        assert_eq!(b.text(), "    code\n    ");
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut b = Buffer::new("a");
        b.move_caret(Move::LineEnd, false);
        b.insert("b");
        b.insert("c");
        assert!(b.undo());
        assert_eq!(b.text(), "ab");
        assert!(b.undo());
        assert_eq!(b.text(), "a");
        assert!(b.redo());
        assert_eq!(b.text(), "ab");
        b.insert("Z"); // new edit clears redo
        assert!(!b.redo());
    }

    #[test]
    fn unicode_columns_are_chars() {
        let mut b = Buffer::new("héllo");
        b.set_caret(Pos::new(0, 2), false);
        b.insert("X");
        assert_eq!(b.text(), "héXllo");
    }

    #[test]
    fn delete_to_line_start_and_end() {
        let mut b = Buffer::new("hello world");
        b.set_caret(Pos::new(0, 6), false);
        b.delete_to_line_start();
        assert_eq!(b.text(), "world");
        b.delete_to_line_end();
        assert_eq!(b.text(), "");
        let mut j = Buffer::new("a\nb");
        j.set_caret(Pos::new(1, 0), false);
        j.delete_to_line_start(); // at column 0: joins with the line above
        assert_eq!(j.text(), "ab");
    }

    #[test]
    fn clear_all_is_undoable() {
        let mut b = Buffer::new("one\ntwo");
        b.clear_all();
        assert!(b.is_empty());
        b.undo();
        assert_eq!(b.text(), "one\ntwo");
    }

    #[test]
    fn delete_lines_covers_selection() {
        let mut b = Buffer::new("a\nb\nc\nd");
        b.set_caret(Pos::new(1, 0), false);
        b.set_caret(Pos::new(2, 1), true);
        b.delete_lines();
        assert_eq!(b.text(), "a\nd");
        let mut all = Buffer::new("only");
        all.delete_lines();
        assert!(all.is_empty());
    }

    #[test]
    fn select_line_extends() {
        let mut b = Buffer::new("a\nb\nc");
        b.select_line();
        assert_eq!(b.selected_text().unwrap(), "a\n");
        b.select_line();
        assert_eq!(b.selected_text().unwrap(), "a\nb\n");
    }

    #[test]
    fn move_and_duplicate_lines() {
        let mut b = Buffer::new("a\nb\nc");
        b.move_lines(true);
        assert_eq!(b.text(), "b\na\nc");
        assert_eq!(at(&b), (1, 0));
        b.move_lines(false);
        assert_eq!(b.text(), "a\nb\nc");
        b.move_lines(false); // already at top: no-op
        assert_eq!(b.text(), "a\nb\nc");
        b.duplicate_lines(true);
        assert_eq!(b.text(), "a\na\nb\nc");
        assert_eq!(at(&b), (1, 0));
    }

    #[test]
    fn indent_and_outdent_keep_caret_on_text() {
        let mut b = Buffer::new("x\ny");
        b.set_caret(Pos::new(0, 1), false);
        b.set_caret(Pos::new(1, 1), true);
        b.indent(4);
        assert_eq!(b.text(), "    x\n    y");
        assert_eq!(at(&b), (1, 5));
        b.outdent(4);
        assert_eq!(b.text(), "x\ny");
        assert_eq!(at(&b), (1, 1));
    }

    #[test]
    fn visual_columns_with_tabs() {
        assert_eq!(visual_col("\tab", 2, 4), 5);
        assert_eq!(col_from_visual("\tab", 5.2, 4), 2);
        assert_eq!(col_from_visual("\tab", 1.0, 4), 0);
        assert_eq!(col_from_visual("ab", 99., 4), 2);
    }
}
