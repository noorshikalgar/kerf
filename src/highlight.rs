//! Syntax highlighting of diff lines with syntect's parser (no syntect themes — scopes are
//! mapped straight to Kerf's restrained palette). Old and new sides are parsed as separate
//! streams so each line gets the state of the lines that precede it on its own side.

use crate::git::{FileDiff, LineKind};
use std::ops::Range;
use std::sync::OnceLock;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

/// Files with more diff lines than this are shown without syntax colour.
pub const MAX_LINES: usize = 50_000;
/// Individual lines longer than this are left plain (and reset nothing — parsing continues).
const MAX_LINE_LEN: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syn {
    Keyword,
    Function,
    Type,
    String,
    Number,
    Comment,
    Punct,
    Attr,
}

pub type LineSpans = Vec<(Range<usize>, Syn)>;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

struct Rules(Vec<(Scope, Syn)>);

fn rules() -> &'static Rules {
    static R: OnceLock<Rules> = OnceLock::new();
    R.get_or_init(|| {
        // Most specific first; first prefix match wins for a given scope.
        let table: &[(&str, Syn)] = &[
            ("comment", Syn::Comment),
            ("string", Syn::String),
            ("constant.character.escape", Syn::String),
            ("constant.numeric", Syn::Number),
            ("constant.language", Syn::Number),
            ("constant", Syn::Number),
            ("entity.name.function", Syn::Function),
            ("support.function", Syn::Function),
            ("variable.function", Syn::Function),
            ("meta.function-call", Syn::Function),
            ("entity.name.type", Syn::Type),
            ("entity.name.class", Syn::Type),
            ("entity.name.struct", Syn::Type),
            ("entity.name.enum", Syn::Type),
            ("entity.name.trait", Syn::Type),
            ("support.type", Syn::Type),
            ("support.class", Syn::Type),
            ("storage.type.numeric", Syn::Type),
            ("entity.other.attribute-name", Syn::Attr),
            ("meta.attribute", Syn::Attr),
            ("meta.annotation", Syn::Attr),
            ("support.macro", Syn::Attr),
            ("entity.name.macro", Syn::Attr),
            ("entity.name.tag", Syn::Keyword),
            ("keyword.operator", Syn::Punct),
            ("keyword", Syn::Keyword),
            ("storage", Syn::Keyword),
            ("variable.language", Syn::Keyword),
            ("punctuation.definition.string", Syn::String),
            ("punctuation.definition.comment", Syn::Comment),
            ("punctuation", Syn::Punct),
        ];
        Rules(table.iter().filter_map(|(s, k)| Scope::new(s).ok().map(|sc| (sc, *k))).collect())
    })
}

fn classify(stack: &ScopeStack) -> Option<Syn> {
    let r = rules();
    // Innermost scope decides.
    for scope in stack.as_slice().iter().rev() {
        for (prefix, syn) in &r.0 {
            if prefix.is_prefix_of(*scope) {
                return Some(*syn);
            }
        }
    }
    None
}

pub fn syntax_for(path: &str, first_line: &str) -> Option<&'static SyntaxReference> {
    let set = syntaxes();
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or(name);
    set.find_syntax_by_extension(ext)
        .or_else(|| set.find_syntax_by_extension(name))
        .or_else(|| set.find_syntax_by_first_line(first_line))
        .filter(|s| s.name != "Plain Text")
}

/// Highlights every line of the diff. Index = `FileDiff::lines` index.
/// Returns `None` when the file is too big or the language is unknown.
pub fn highlight(diff: &FileDiff) -> Option<Vec<LineSpans>> {
    if diff.lines.len() > MAX_LINES || diff.lines.is_empty() {
        return None;
    }
    let first = diff.lines.first().map(|l| diff.line_text(l)).unwrap_or("");
    let syntax = syntax_for(&diff.path, first)?;
    let set = syntaxes();
    let mut out: Vec<LineSpans> = vec![Vec::new(); diff.lines.len()];

    for side in [LineKind::Removed, LineKind::Added] {
        // Each side is its own stream: context lines + that side's changed lines.
        let mut state = ParseState::new(syntax);
        let mut stack = ScopeStack::new();
        // Lines hidden between hunks are unknown; parser state carries over (best effort).
        #[allow(clippy::needless_range_loop)]
        for hunk in &diff.hunks {
            for idx in hunk.first_line..hunk.first_line + hunk.line_count {
                let line = &diff.lines[idx];
                if line.kind != LineKind::Context && line.kind != side {
                    continue;
                }
                let text = diff.line_text(line);
                if text.len() > MAX_LINE_LEN {
                    continue;
                }
                let mut buf = String::with_capacity(text.len() + 1);
                buf.push_str(text);
                buf.push('\n');
                let Ok(ops) = state.parse_line(&buf, set) else {
                    state = ParseState::new(syntax);
                    stack = ScopeStack::new();
                    continue;
                };
                let spans = spans_for_line(text.len(), &ops, &mut stack);
                // Context lines are parsed on both sides; keep the first result.
                if line.kind == side || out[idx].is_empty() {
                    out[idx] = spans;
                }
            }
        }
    }
    Some(out)
}

fn spans_for_line(
    len: usize,
    ops: &[(usize, syntect::parsing::ScopeStackOp)],
    stack: &mut ScopeStack,
) -> LineSpans {
    let mut spans: LineSpans = Vec::new();
    let mut pos = 0;
    let push = |from: usize, to: usize, stack: &ScopeStack, spans: &mut LineSpans| {
        let to = to.min(len);
        if from >= to {
            return;
        }
        if let Some(syn) = classify(stack) {
            match spans.last_mut() {
                Some((r, s)) if *s == syn && r.end == from => r.end = to,
                _ => spans.push((from..to, syn)),
            }
        }
    };
    for (at, op) in ops {
        push(pos, *at, stack, &mut spans);
        pos = (*at).max(pos);
        let _ = stack.apply(op);
    }
    push(pos, len, stack, &mut spans);
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{DiffBody, DiffLine, Hunk};
    use std::sync::Arc;

    fn one_hunk(path: &str, lines: &[(LineKind, &str)]) -> FileDiff {
        let mut text = String::new();
        let mut out = Vec::new();
        for (k, t) in lines {
            let s = text.len();
            text.push_str(t);
            out.push(DiffLine { kind: *k, old_no: Some(1), new_no: Some(1), range: s..text.len() });
        }
        FileDiff {
            path: path.into(),
            body: DiffBody::Text,
            old_size: 0,
            new_size: 0,
            total_lines: lines.len(),
            text: Arc::from(text),
            hunks: vec![Hunk { old_start: 1, old_lines: 1, new_start: 1, new_lines: 1, context: String::new(), first_line: 0, line_count: out.len() }],
            lines: out,
            non_utf8: false,
            old_no_newline: false,
            new_no_newline: false,
        }
    }

    #[test]
    fn rust_keywords_strings_comments() {
        let d = one_hunk(
            "src/main.rs",
            &[(LineKind::Added, "fn main() { let s = \"hi\"; } // done")],
        );
        let spans = highlight(&d).unwrap();
        let text = d.line_text(&d.lines[0]);
        let find = |syn: Syn| -> Vec<&str> {
            spans[0].iter().filter(|(_, s)| *s == syn).map(|(r, _)| &text[r.clone()]).collect()
        };
        assert!(find(Syn::Keyword).iter().any(|t| t.contains("fn")), "{:?}", spans[0]);
        assert!(find(Syn::String).iter().any(|t| t.contains("hi")), "{:?}", spans[0]);
        assert!(find(Syn::Comment).iter().any(|t| t.contains("done")), "{:?}", spans[0]);
    }

    #[test]
    fn unknown_extension_is_plain() {
        let d = one_hunk("data.zzqx", &[(LineKind::Added, "whatever")]);
        assert!(highlight(&d).is_none());
    }

    #[test]
    fn multiline_comment_state_carries_within_side() {
        let d = one_hunk(
            "a.rs",
            &[(LineKind::Added, "/* start"), (LineKind::Added, "still comment */ fn x() {}")],
        );
        let spans = highlight(&d).unwrap();
        assert_eq!(spans[1].first().map(|(_, s)| *s), Some(Syn::Comment));
    }
}
