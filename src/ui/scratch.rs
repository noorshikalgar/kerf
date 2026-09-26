//! Plain diffs outside git — a live side-by-side diff editor (Meld / VS Code style).
//!
//! Both sides are always editable. Every edit re-aligns the two texts (`crate::align`):
//! changed lines are tinted, words emphasised, and filler rows keep matching lines level.
//! Both editors share one scroll handle, so they scroll together. Nothing is written to disk.

use super::app::{Input, Kerf, Scratch, ScratchSide, Side, Target};
use super::editor::{Editor, EditorEvent};
use super::scrollbar::Bar;
use super::widgets::{seg, status_glyph, thousands};
use crate::align::{align, Aligned};
use crate::git::ChangeStatus;
use crate::theme;
use gpui::{
    div, prelude::*, px, relative, AnyElement, App, Context, Entity, FontWeight, PathPromptOptions, SharedString,
    Subscription, Task, UniformListScrollHandle, Window,
};
use std::path::PathBuf;
use std::sync::Arc;

/// Above this many total lines the diff runs on a background thread.
const SYNC_LINES: usize = 20_000;
const PLACEHOLDER: &str = "Type, paste (⌘V) or open a file…";

/// The two editors of a plain diff and their live alignment.
pub struct EditorPair {
    pub left: Entity<Editor>,
    pub right: Entity<Editor>,
    /// Shared by both editors: one scroll moves both sides.
    pub scroll: UniformListScrollHandle,
    /// Where each side's text came from: `(file label, buffer version when loaded)`.
    pub sources: [Option<(String, u64)>; 2],
    pub aligned: Option<Arc<Aligned>>,
    generation: u64,
    _task: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

pub fn tilde(path: &str) -> String {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
    if !home.is_empty() && path.starts_with(&home) {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_string()
    }
}

fn read_side(path: &PathBuf) -> anyhow::Result<ScratchSide> {
    let bytes = std::fs::read(path)?;
    Ok(ScratchSide { label: tilde(&path.display().to_string()), text: Arc::from(bytes) })
}

fn side_index(side: Side) -> usize {
    match side {
        Side::Left => 0,
        Side::Right => 1,
    }
}

impl Kerf {
    fn next_scratch(&mut self) -> Scratch {
        self.scratch_seq += 1;
        Scratch { id: self.scratch_seq, left: None, right: None }
    }

    /// ⌘N — empty live diff; start typing on the left.
    pub fn new_scratch(&mut self, cx: &mut Context<Self>) {
        let s = self.next_scratch();
        self.open_target(Target::scratch(s), true, cx);
    }

    /// Live diff of two files (Compare Files, `kerf a b`). Editable in memory only.
    pub fn open_files_diff(&mut self, a: PathBuf, b: PathBuf, cx: &mut Context<Self>) {
        let mut s = self.next_scratch();
        for (path, slot) in [(a, &mut s.left), (b, &mut s.right)] {
            match read_side(&path) {
                Ok(v) => *slot = Some(v),
                Err(e) => self.flash(format!("Could not read file: {e}"), cx),
            }
        }
        self.open_target(Target::scratch(s), true, cx);
    }

    /// ⌥⌘N — pick two files (or one; the other side stays empty to type or open into).
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
                            Ok(v) => s.left = Some(v),
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

    fn active_scratch_id(&self) -> Option<u64> {
        self.active_scratch().map(|s| s.id)
    }

    /// Scroll handle + view rows of the active live diff (for the shared scrollbar).
    pub fn live_scroll(&self) -> Option<(&UniformListScrollHandle, usize)> {
        let p = self.editors.get(&self.active_scratch_id()?)?;
        let rows = p.aligned.as_ref().map(|a| a.rows()).unwrap_or(0);
        Some((&p.scroll, rows))
    }

    // ───────────────────────────── editors ─────────────────────────────

    /// Creates the two editors for a plain diff the first time it is shown.
    pub fn ensure_editors(&mut self, sc: &Scratch, window: &mut Window, cx: &mut Context<Self>) {
        if self.editors.contains_key(&sc.id) {
            return;
        }
        let text = |side: &Option<ScratchSide>| {
            side.as_ref().map(|s| String::from_utf8_lossy(&s.text).into_owned()).unwrap_or_default()
        };
        let (lt, rt) = (text(&sc.left), text(&sc.right));
        let scroll = UniformListScrollHandle::new();
        let font = self.font.clone();
        let (f2, s2) = (font.clone(), scroll.clone());
        let left = cx.new(|cx| Editor::new(&lt, font, &super::widgets::keys(PLACEHOLDER), scroll.clone(), true, cx));
        let right = cx.new(|cx| Editor::new(&rt, f2, &super::widgets::keys(PLACEHOLDER), s2, false, cx));
        let source = |side: &Option<ScratchSide>, e: &Entity<Editor>, cx: &App| {
            side.as_ref().map(|s| (s.label.clone(), e.read(cx).buffer.version))
        };
        let sources = [source(&sc.left, &left, cx), source(&sc.right, &right, cx)];
        let id = sc.id;
        let subs = [&left, &right]
            .into_iter()
            .map(|e| {
                cx.subscribe_in(e, window, move |this, _, ev: &EditorEvent, window, cx| match ev {
                    EditorEvent::Changed => this.recompute_live(id, cx),
                    EditorEvent::Blur => window.focus(&this.focus),
                    EditorEvent::Submit => {}
                })
            })
            .collect();
        // A fresh diff starts with the caret in the left editor.
        if sc.left.is_none() {
            left.read(cx).focus(window);
        }
        self.editors.insert(
            id,
            EditorPair { left, right, scroll, sources, aligned: None, generation: 0, _task: None, _subs: subs },
        );
        self.recompute_live(id, cx);
    }

    fn editor(&self, id: u64, side: Side) -> Option<Entity<Editor>> {
        let p = self.editors.get(&id)?;
        Some(match side {
            Side::Left => p.left.clone(),
            Side::Right => p.right.clone(),
        })
    }

    /// Re-aligns both sides after an edit. Small texts: immediately; large: in background.
    pub fn recompute_live(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(p) = self.editors.get_mut(&id) else { return };
        p.generation += 1;
        let generation = p.generation;
        let (l, r) = (p.left.read(cx).buffer.lines().to_vec(), p.right.read(cx).buffer.lines().to_vec());
        let ignore_ws = self.ignore_ws;
        if l.len() + r.len() <= SYNC_LINES {
            let a = align(&l, &r, ignore_ws);
            self.apply_alignment(id, a, cx);
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            // Off the UI thread: give the diff room for an exact result.
            let a = cx
                .background_executor()
                .spawn(async move { crate::align::align_within(&l, &r, ignore_ws, std::time::Duration::from_secs(5)) })
                .await;
            this.update(cx, |this, cx| {
                if this.editors.get(&id).is_some_and(|p| p.generation == generation) {
                    this.apply_alignment(id, a, cx);
                }
            })
            .ok();
        });
        if let Some(p) = self.editors.get_mut(&id) {
            p._task = Some(task);
        }
    }

    fn apply_alignment(&mut self, id: u64, a: Aligned, cx: &mut Context<Self>) {
        let Some(p) = self.editors.get_mut(&id) else { return };
        let a = Arc::new(a);
        let (left, right) = (p.left.clone(), p.right.clone());
        p.aligned = Some(a.clone());
        // Same gutter and row width on both sides so horizontal scrolling stays in lockstep.
        let (lw, rw) = (left.read(cx).widest_chars(), right.read(cx).widest_chars());
        let digits = left.read(cx).buffer.line_count().max(right.read(cx).buffer.line_count()).to_string().len().max(3);
        let char_w = left.read(cx).char_w;
        let min_w = (digits as f32 * char_w + 20.) + 8. + lw.max(rw) as f32 * char_w + 40.;
        let (lrows, rrows) = (Arc::new(a.left.clone()), Arc::new(a.right.clone()));
        left.update(cx, |e, cx| {
            e.min_row_w = min_w;
            e.min_digits = digits;
            e.set_decor(Some(lrows), cx)
        });
        right.update(cx, |e, cx| {
            e.min_row_w = min_w;
            e.min_digits = digits;
            e.set_decor(Some(rrows), cx)
        });
        self.refresh_scratch_title(id, cx);
        cx.notify();
    }

    /// A side's file label (empty when typed/pasted) and whether it was edited since loading.
    fn side_label(&self, id: u64, side: Side, cx: &App) -> (String, bool) {
        let Some(p) = self.editors.get(&id) else { return (String::new(), false) };
        let e = match side {
            Side::Left => &p.left,
            Side::Right => &p.right,
        };
        match &p.sources[side_index(side)] {
            Some((label, v)) => (label.clone(), *v != e.read(cx).buffer.version),
            None => (String::new(), false),
        }
    }

    /// Tab title follows the files on each side.
    fn refresh_scratch_title(&mut self, id: u64, cx: &App) {
        let name = |this: &Self, side: Side| {
            let (label, _) = this.side_label(id, side, cx);
            (!label.is_empty()).then(|| label.rsplit(['/', '\\']).next().unwrap_or(&label).to_string())
        };
        let title = match (name(self, Side::Left), name(self, Side::Right)) {
            (Some(l), Some(r)) => format!("{l} ↔ {r}"),
            (Some(n), None) | (None, Some(n)) => format!("{n} ↔ …"),
            _ => "Untitled Diff".into(),
        };
        for t in &mut self.tabs {
            if t.target.scratch.as_ref().is_some_and(|s| s.id == id) {
                t.target.change.path = title.clone();
            }
        }
    }

    /// ⌘V — into the text input when one is active, else into the live diff's first empty
    /// side. (A focused editor handles ⌘V itself.)
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
            Input::RepoMenu => {
                self.repo_query.push_str(text.lines().next().unwrap_or("").trim());
                self.repo_sel = 0;
                cx.notify();
                return;
            }
            Input::None => {}
        }
        match self.active_scratch_id() {
            Some(id) => {
                let side = [Side::Left, Side::Right]
                    .into_iter()
                    .find(|s| self.editor(id, *s).is_some_and(|e| e.read(cx).buffer.is_empty()))
                    .unwrap_or(Side::Right);
                self.replace_side(id, side, &text, cx);
            }
            None => self.new_scratch(cx),
        }
    }

    fn replace_side(&mut self, id: u64, side: Side, text: &str, cx: &mut Context<Self>) {
        if let Some(e) = self.editor(id, side) {
            e.update(cx, |e, cx| {
                e.buffer.select_all();
                e.paste_text(text, cx);
            });
        }
    }

    fn paste_side(&mut self, id: u64, side: Side, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
            self.replace_side(id, side, &text, cx);
        }
    }

    fn clear_side(&mut self, id: u64, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.editors.get_mut(&id) {
            p.sources[side_index(side)] = None;
        }
        if let Some(e) = self.editor(id, side) {
            e.update(cx, |e, cx| e.set_text("", cx));
            e.read(cx).focus(window);
        }
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
                        p.sources[side_index(side)] = Some((v.label, version));
                    }
                    this.refresh_scratch_title(id, cx);
                    cx.notify();
                }
                Err(e) => this.flash(format!("Could not read file: {e}"), cx),
            })
            .ok();
        })
        .detach();
    }

    pub fn swap_scratch(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.active_scratch_id() else { return };
        let Some(p) = self.editors.get_mut(&id) else { return };
        std::mem::swap(&mut p.left, &mut p.right);
        p.sources.swap(0, 1);
        let (l, r) = (p.left.clone(), p.right.clone());
        l.update(cx, |e, _| e.set_left(true));
        r.update(cx, |e, _| e.set_left(false));
        self.recompute_live(id, cx);
    }

    /// Re-run all live diffs (e.g. ignore-whitespace toggled).
    pub fn recompute_all_live(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<u64> = self.editors.keys().copied().collect();
        for id in ids {
            self.recompute_live(id, cx);
        }
    }

    // ───────────────────────────── view ─────────────────────────────

    pub fn render_live(&self, sc: &Scratch, cx: &mut Context<Self>) -> AnyElement {
        let id = sc.id;
        let aligned = self.editors.get(&id).and_then(|p| p.aligned.clone());
        let title = self
            .tabs
            .iter()
            .find(|t| t.target.scratch.as_ref().is_some_and(|s| s.id == id))
            .map(|t| t.target.change.path.clone())
            .unwrap_or_else(|| "Untitled Diff".into());
        let both_empty =
            [Side::Left, Side::Right].iter().all(|s| self.editor(id, *s).is_none_or(|e| e.read(cx).buffer.is_empty()));
        let status: AnyElement = match &aligned {
            _ if both_empty => div()
                .text_size(theme::TEXT_CONTROL)
                .text_color(theme::mute())
                .child("Type or paste on both sides")
                .into_any_element(),
            Some(a) if a.identical() => {
                div().text_size(theme::TEXT_CONTROL).text_color(theme::frost()).child("● Identical").into_any_element()
            }
            Some(a) => div()
                .flex()
                .gap(px(6.))
                .text_size(theme::TEXT_CONTROL)
                .child(div().text_color(theme::add_fg()).child(format!("+{}", thousands(a.additions as u64))))
                .child(div().text_color(theme::del_fg()).child(format!("−{}", thousands(a.deletions as u64))))
                .into_any_element(),
            None => div().into_any_element(),
        };
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
                            .child(title),
                    )
                    .child(status)
                    .child(div().w(px(8.)))
                    .child(
                        seg("live-swap", "⇄ Swap", false, "Swap left and right")
                            .on_click(cx.listener(|this, _, _, cx| this.swap_scratch(cx))),
                    )
                    .child(
                        seg("live-ws", "Ignore WS", self.ignore_ws, "Ignore whitespace differences")
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_ws(cx))),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_side(id, Side::Left, cx))
                    .child(div().w(px(1.)).h_full().flex_none().bg(theme::line_hi()))
                    .child(self.render_side(id, Side::Right, cx))
                    .child(self.render_live_map(aligned.as_deref(), cx))
                    .child(self.render_scrollbar(Bar::Diff, cx)),
            )
            .into_any_element()
    }

    fn render_live_map(&self, aligned: Option<&Aligned>, cx: &mut Context<Self>) -> AnyElement {
        let marks = aligned
            .map(|a| {
                let total = a.rows().max(1) as f32;
                a.change_blocks()
                    .into_iter()
                    .map(|(start, len, added)| {
                        div()
                            .absolute()
                            .left(px(4.))
                            .right(px(4.))
                            .top(relative(start as f32 / total))
                            .h(relative(len as f32 / total))
                            .min_h(px(2.))
                            .bg(if added { theme::add_fg() } else { theme::del_fg() })
                            .into_any_element()
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.render_map(marks, cx)
    }

    fn render_side(&self, id: u64, side: Side, cx: &mut Context<Self>) -> impl IntoElement {
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
        let (label, edited) = self.side_label(id, side, cx);
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
                    .border_b_1()
                    .border_color(theme::line())
                    .text_size(theme::TEXT_CONTROL)
                    .child(div().flex_none().text_color(theme::body()).child(match side {
                        Side::Left => "Left",
                        Side::Right => "Right",
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .gap(px(6.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_color(theme::mute())
                            .when(!label.is_empty(), |d| {
                                d.child(div().min_w_0().overflow_hidden().text_ellipsis().child(label.clone()))
                            })
                            .when(edited, |d| d.child(div().flex_none().text_color(theme::mod_fg()).child("● edited")))
                            .when(!empty, |d| {
                                d.child(div().flex_none().child(format!("{} lines", thousands(lines as u64))))
                            }),
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
