//! Plain diffs outside git: paste two texts or pick two files. Each is a tab ("scratch").

use super::app::{Input, Kerf, Scratch, ScratchSide, Side, Target};
use super::editor::{Editor, EditorEvent};
use super::widgets::{seg, status_glyph, thousands};
use crate::git::ChangeStatus;
use crate::theme;
use gpui::{
    div, prelude::*, px, AnyElement, App, Context, Entity, FontWeight, PathPromptOptions, SharedString, Subscription, Window,
};
use std::path::PathBuf;
use std::sync::Arc;

/// The two editors of a plain diff, plus where each side's text came from:
/// `(file label, buffer version when loaded)` — the label shows until the text is edited.
pub struct EditorPair {
    pub left: Entity<Editor>,
    pub right: Entity<Editor>,
    pub sources: [Option<(String, u64)>; 2],
    _subs: Vec<Subscription>,
}

pub fn tilde(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && path.starts_with(&home) => format!("~{}", &path[home.len()..]),
        _ => path.to_string(),
    }
}

fn read_side(path: &PathBuf) -> anyhow::Result<ScratchSide> {
    let bytes = std::fs::read(path)?;
    Ok(ScratchSide { label: tilde(&path.display().to_string()), text: Arc::from(bytes) })
}

impl Kerf {
    fn next_scratch(&mut self) -> Scratch {
        self.scratch_seq += 1;
        Scratch { id: self.scratch_seq, left: None, right: None, editing: true, focus: Side::Left }
    }

    /// ⌘N — empty two-pane diff.
    pub fn new_scratch(&mut self, cx: &mut Context<Self>) {
        let s = self.next_scratch();
        self.open_target(Target::scratch(s), true, cx);
    }

    /// Scratch diff of two files already chosen (CLI `kerf a b`).
    pub fn open_files_diff(&mut self, a: PathBuf, b: PathBuf, cx: &mut Context<Self>) {
        let mut s = self.next_scratch();
        match (read_side(&a), read_side(&b)) {
            (Ok(l), Ok(r)) => {
                s.left = Some(l);
                s.right = Some(r);
                s.editing = false;
            }
            (l, r) => {
                let e = [&l, &r].iter().find_map(|x| x.as_ref().err().map(|e| e.to_string())).unwrap_or_default();
                self.flash(format!("Could not read file: {e}"), cx);
                s.left = l.ok();
                s.right = r.ok();
            }
        }
        self.open_target(Target::scratch(s), true, cx);
    }

    /// ⌘⇧N — pick two files (or one, then the other side stays open for a second pick).
    pub fn prompt_compare_files(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Compare (select 2 files)".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            this.update(cx, |this, cx| {
                let mut it = paths.into_iter();
                match (it.next(), it.next()) {
                    (Some(a), Some(b)) => this.open_files_diff(a, b, cx),
                    (Some(a), None) => {
                        let mut s = this.next_scratch();
                        match read_side(&a) {
                            Ok(side) => {
                                s.left = Some(side);
                                s.focus = Side::Right;
                            }
                            Err(e) => this.flash(format!("Could not read file: {e}"), cx),
                        }
                        this.open_target(Target::scratch(s), true, cx);
                    }
                    _ => {}
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn active_scratch(&self) -> Option<Arc<Scratch>> {
        self.active_tab.and_then(|a| self.tabs.get(a)).and_then(|t| t.target.scratch.clone())
    }

    /// Applies `f` to the active scratch, then shows the diff (if complete) or the composer.
    pub fn update_scratch(&mut self, f: impl FnOnce(&mut Scratch), cx: &mut Context<Self>) {
        let Some(a) = self.active_tab else { return };
        let Some(sc) = self.tabs.get(a).and_then(|t| t.target.scratch.clone()) else { return };
        let mut next = (*sc).clone();
        f(&mut next);
        if next.left.is_none() || next.right.is_none() {
            next.editing = true;
        }
        let target = Target::scratch(next);
        self.tabs[a].target = target.clone();
        self.tabs[a].cached = None;
        self.tabs[a].pinned = true;
        self.load_diff_public(target, cx);
        cx.notify();
    }

    // ───────────────────────────── editors ─────────────────────────────
    // While composing, the two editors are the source of truth. Compare (⌘↵) snapshots
    // their text into the scratch sides and shows the diff; Edit brings the editors back.

    /// Creates the Left/Right editors for a scratch the first time it is composed.
    pub fn ensure_editors(&mut self, sc: &Scratch, window: &mut Window, cx: &mut Context<Self>) {
        if self.editors.contains_key(&sc.id) {
            return;
        }
        let text = |side: &Option<ScratchSide>| side.as_ref().map(|s| String::from_utf8_lossy(&s.text).into_owned()).unwrap_or_default();
        let font = self.font.clone();
        let (lt, rt) = (text(&sc.left), text(&sc.right));
        let left = cx.new(|cx| Editor::new(&lt, font.clone(), "Type, paste (⌘V) or open a file…", cx));
        let right = cx.new(|cx| Editor::new(&rt, font, "Type, paste (⌘V) or open a file…", cx));
        let source = |side: &Option<ScratchSide>, e: &Entity<Editor>, cx: &App| {
            side.as_ref().filter(|s| s.label != "Left" && s.label != "Right").map(|s| (s.label.clone(), e.read(cx).buffer.version))
        };
        let sources = [source(&sc.left, &left, cx), source(&sc.right, &right, cx)];
        let id = sc.id;
        let subs = [&left, &right]
            .into_iter()
            .map(|e| {
                cx.subscribe_in(e, window, move |this, _, ev: &EditorEvent, window, cx| match ev {
                    EditorEvent::Submit => this.compare_editors(id, cx),
                    EditorEvent::Blur => window.focus(&this.focus),
                    EditorEvent::Changed => cx.notify(),
                })
            })
            .collect();
        // Start typing straight away in the left editor of a fresh diff.
        if sc.left.is_none() && sc.right.is_none() {
            left.read(cx).focus(window);
        }
        self.editors.insert(id, EditorPair { left, right, sources, _subs: subs });
    }

    fn editor(&self, id: u64, side: Side) -> Option<Entity<Editor>> {
        let p = self.editors.get(&id)?;
        Some(match side {
            Side::Left => p.left.clone(),
            Side::Right => p.right.clone(),
        })
    }

    /// Label for a side: the file it came from while unedited, else just Left / Right.
    fn side_label(&self, id: u64, side: Side, cx: &App) -> String {
        let default = match side {
            Side::Left => "Left",
            Side::Right => "Right",
        };
        let Some(p) = self.editors.get(&id) else { return default.into() };
        let (e, src) = match side {
            Side::Left => (&p.left, &p.sources[0]),
            Side::Right => (&p.right, &p.sources[1]),
        };
        match src {
            Some((label, v)) if *v == e.read(cx).buffer.version => label.clone(),
            _ => default.into(),
        }
    }

    /// Snapshots both editors into the scratch and shows the diff.
    pub fn compare_editors(&mut self, id: u64, cx: &mut Context<Self>) {
        let (Some(l), Some(r)) = (self.editor(id, Side::Left), self.editor(id, Side::Right)) else { return };
        let side = |this: &Self, e: &Entity<Editor>, s: Side, cx: &App| ScratchSide {
            label: this.side_label(id, s, cx),
            text: Arc::from(e.read(cx).buffer.text().into_bytes()),
        };
        let (ls, rs) = (side(self, &l, Side::Left, cx), side(self, &r, Side::Right, cx));
        if ls.text.is_empty() && rs.text.is_empty() {
            self.flash("Both sides are empty", cx);
            return;
        }
        self.update_scratch(
            |s| {
                s.left = Some(ls);
                s.right = Some(rs);
                s.editing = false;
            },
            cx,
        );
    }

    /// Compare automatically once both sides have content (after Paste / Open File).
    fn auto_compare(&mut self, id: u64, cx: &mut Context<Self>) {
        let both = [Side::Left, Side::Right]
            .iter()
            .all(|s| self.editor(id, *s).is_some_and(|e| !e.read(cx).buffer.is_empty()));
        if both {
            self.compare_editors(id, cx);
        }
    }

    /// ⌘V — into the text input when one is active, else into the composer.
    /// (A focused editor handles ⌘V itself.)
    pub fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) else { return };
        match self.input {
            Input::Filter => {
                self.filter.push_str(text.lines().next().unwrap_or(""));
                self.selected = None;
                self.rebuild_rows();
                cx.notify();
                return;
            }
            Input::Picker => {
                if let Some(p) = self.picker.as_mut() {
                    p.query.push_str(text.lines().next().unwrap_or("").trim());
                    p.selected = 0;
                }
                cx.notify();
                return;
            }
            Input::None => {}
        }
        match self.active_scratch() {
            Some(sc) if !sc.ready() => {
                // First empty side, else the right one.
                let side = [Side::Left, Side::Right]
                    .into_iter()
                    .find(|s| self.editor(sc.id, *s).is_some_and(|e| e.read(cx).buffer.is_empty()))
                    .unwrap_or(Side::Right);
                self.paste_side_text(sc.id, side, text, cx);
            }
            Some(_) => {}
            None => self.new_scratch(cx),
        }
    }

    fn paste_side_text(&mut self, id: u64, side: Side, text: String, cx: &mut Context<Self>) {
        if let Some(e) = self.editor(id, side) {
            e.update(cx, |e, cx| {
                e.buffer.select_all();
                e.paste_text(&text, cx);
            });
            self.auto_compare(id, cx);
        }
    }

    fn paste_side(&mut self, id: u64, side: Side, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
            self.paste_side_text(id, side, text, cx);
        }
    }

    fn clear_side(&mut self, id: u64, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.editor(id, side) {
            e.update(cx, |e, cx| e.set_text("", cx));
            e.read(cx).focus(window);
        }
        if let Some(p) = self.editors.get_mut(&id) {
            p.sources[side as usize] = None;
        }
        cx.notify();
    }

    fn open_file_side(&mut self, id: u64, side: Side, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(match side {
                Side::Left => "Left file".into(),
                Side::Right => "Right file".into(),
            }),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            let Some(p) = paths.into_iter().next() else { return };
            let read = cx.background_executor().spawn(async move { read_side(&p) }).await;
            this.update(cx, |this, cx| match read {
                Ok(v) => {
                    let Some(e) = this.editor(id, side) else { return };
                    let text = String::from_utf8_lossy(&v.text).into_owned();
                    e.update(cx, |e, cx| e.set_text(&text, cx));
                    let version = e.read(cx).buffer.version;
                    if let Some(p) = this.editors.get_mut(&id) {
                        p.sources[side as usize] = Some((v.label, version));
                    }
                    this.auto_compare(id, cx);
                    cx.notify();
                }
                Err(e) => this.flash(format!("Could not read file: {e}"), cx),
            })
            .ok();
        })
        .detach();
    }

    pub fn swap_scratch(&mut self, cx: &mut Context<Self>) {
        let Some(sc) = self.active_scratch() else { return };
        if let Some(p) = self.editors.get_mut(&sc.id) {
            std::mem::swap(&mut p.left, &mut p.right);
            p.sources.swap(0, 1);
        }
        self.update_scratch(|s| std::mem::swap(&mut s.left, &mut s.right), cx);
    }

    pub fn edit_scratch(&mut self, cx: &mut Context<Self>) {
        self.update_scratch(|s| s.editing = true, cx);
    }

    // ───────────────────────────── composer ─────────────────────────────
    // Same visual grammar as the git diff: file header bar, split columns divided by a
    // hairline, hunk-header-style strip per side, gutter + code rows. No extra chrome.

    pub fn render_composer(&self, sc: &Scratch, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(theme::HEADER_H)
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(12.))
                    .bg(theme::crypt())
                    .border_b_1()
                    .border_color(theme::line())
                    .child(status_glyph(ChangeStatus::Modified))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme::bone())
                            .child(sc.title()),
                    )
                    .child(seg("sc-swap", "⇄ Swap", false, "Swap left and right").on_click(cx.listener(|this, _, _, cx| this.swap_scratch(cx))))
                    .child({
                        let id = sc.id;
                        seg("sc-compare", "Compare ⌘↵", true, "Show the diff of Left and Right")
                            .on_click(cx.listener(move |this, _, _, cx| this.compare_editors(id, cx)))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_pane(sc, Side::Left, cx))
                    .child(div().w(px(1.)).h_full().flex_none().bg(theme::line_hi()))
                    .child(self.render_pane(sc, Side::Right, cx)),
            )
            .into_any_element()
    }

    fn render_pane(&self, sc: &Scratch, side: Side, cx: &mut Context<Self>) -> impl IntoElement {
        let id = sc.id;
        let key = match side {
            Side::Left => "left",
            Side::Right => "right",
        };
        let editor = self.editor(id, side);
        let (lines, empty) = editor
            .as_ref()
            .map(|e| {
                let e = e.read(cx);
                (e.buffer.line_count(), e.buffer.is_empty())
            })
            .unwrap_or((0, true));
        let label = self.side_label(id, side, cx);
        let is_file = label != "Left" && label != "Right";
        let link = |id: &str, text: &'static str| {
            div()
                .id(SharedString::from(format!("{key}-{id}")))
                .px(px(6.))
                .h(px(18.))
                .flex()
                .items_center()
                .rounded(theme::RADIUS)
                .text_size(theme::TEXT_CONTROL)
                .text_color(theme::body())
                .cursor_pointer()
                .hover(|s| s.bg(theme::slate()).text_color(theme::bone()))
                .child(text)
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::void())
            // Side strip, styled like a hunk header row.
            .child(
                div()
                    .h(theme::ROW_CODE)
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(12.))
                    .pr(px(6.))
                    .bg(theme::crypt())
                    .text_size(theme::TEXT_CONTROL)
                    .child(div().flex_none().text_color(theme::body()).child(match side {
                        Side::Left => "Left",
                        Side::Right => "Right",
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(theme::mute())
                            .when(is_file, |d| d.child(label.clone()))
                            .when(!empty, |d| d.child(format!("{}{} lines", if is_file { " · " } else { "" }, thousands(lines as u64)))),
                    )
                    .child(link("paste", "Paste").on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.paste_side(id, side, cx);
                    })))
                    .child(link("open", "Open File…").on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.open_file_side(id, side, cx);
                    })))
                    .when(!empty, |d| {
                        d.child(link("clear", "Clear").on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.clear_side(id, side, window, cx);
                        })))
                    }),
            )
            .child(div().flex_1().min_h_0().when_some(editor, |d, e| d.child(e)))
    }
}

