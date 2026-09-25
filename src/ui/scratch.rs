//! Plain diffs outside git: paste two texts or pick two files. Each is a tab ("scratch").

use super::app::{Input, Kerf, Scratch, ScratchSide, Side, Target};
use super::widgets::{micro, seg, thousands};
use crate::theme;
use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, PathPromptOptions, SharedString};
use std::path::PathBuf;
use std::sync::Arc;

/// Lines shown in a composer pane preview.
const PREVIEW_LINES: usize = 400;

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

    fn set_side(&mut self, side: Side, value: ScratchSide, cx: &mut Context<Self>) {
        self.update_scratch(
            |s| {
                match side {
                    Side::Left => s.left = Some(value),
                    Side::Right => s.right = Some(value),
                }
                // Next paste goes to the other side while it is empty; show diff once both exist.
                s.focus = match side {
                    Side::Left if s.right.is_none() => Side::Right,
                    Side::Right if s.left.is_none() => Side::Left,
                    other => other,
                };
                if s.left.is_some() && s.right.is_some() {
                    s.editing = false;
                }
            },
            cx,
        );
    }

    /// ⌘V — into the text input when one is active, else into the focused scratch side.
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
        let Some(sc) = self.active_scratch() else {
            // Pasting with no scratch open starts one.
            self.new_scratch(cx);
            self.paste_into(Side::Left, text, cx);
            return;
        };
        let side = if sc.editing || !sc.ready() { sc.focus } else { Side::Right };
        if sc.ready() {
            return; // viewing a finished diff: ignore stray pastes
        }
        self.paste_into(side, text, cx);
    }

    fn paste_into(&mut self, side: Side, text: String, cx: &mut Context<Self>) {
        self.set_side(side, ScratchSide { label: "Pasted text".into(), text: Arc::from(text.into_bytes()) }, cx);
    }

    fn paste_side(&mut self, side: Side, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
            self.paste_into(side, text, cx);
        }
    }

    fn open_file_side(&mut self, side: Side, cx: &mut Context<Self>) {
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
                Ok(v) => this.set_side(side, v, cx),
                Err(e) => this.flash(format!("Could not read file: {e}"), cx),
            })
            .ok();
        })
        .detach();
    }

    pub fn swap_scratch(&mut self, cx: &mut Context<Self>) {
        self.update_scratch(|s| std::mem::swap(&mut s.left, &mut s.right), cx);
    }

    pub fn edit_scratch(&mut self, cx: &mut Context<Self>) {
        self.update_scratch(|s| s.editing = true, cx);
    }

    fn show_scratch_diff(&mut self, cx: &mut Context<Self>) {
        self.update_scratch(|s| s.editing = false, cx);
    }

    // ───────────────────────────── composer ─────────────────────────────

    pub fn render_composer(&self, sc: &Scratch, cx: &mut Context<Self>) -> AnyElement {
        let both = sc.left.is_some() && sc.right.is_some();
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
                    .child(div().font_weight(FontWeight::BOLD).text_color(theme::bone()).child(sc.title()))
                    .child(div().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child("Plain diff — not tied to git"))
                    .child(div().flex_1())
                    .child(seg("sc-swap", "⇄ Swap", false, "Swap left and right").on_click(cx.listener(|this, _, _, cx| this.swap_scratch(cx))))
                    .when(both, |d| {
                        d.child(
                            seg("sc-show", "Show Diff", true, "Show the diff")
                                .on_click(cx.listener(|this, _, _, cx| this.show_scratch_diff(cx))),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(1.))
                    .bg(theme::line())
                    .child(self.render_pane(sc, Side::Left, cx))
                    .child(self.render_pane(sc, Side::Right, cx)),
            )
            .into_any_element()
    }

    fn render_pane(&self, sc: &Scratch, side: Side, cx: &mut Context<Self>) -> impl IntoElement {
        let (label, content, key) = match side {
            Side::Left => ("Left · Original", &sc.left, "left"),
            Side::Right => ("Right · Changed", &sc.right, "right"),
        };
        let focused = sc.focus == side;
        let btn = |id: &str, text: &'static str, tip: &'static str| seg(SharedString::from(format!("{key}-{id}")), text, false, tip);
        div()
            .id(SharedString::from(format!("pane-{key}")))
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::void())
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.active_scratch().is_some_and(|s| s.focus != side) {
                    this.update_scratch(|s| s.focus = side, cx);
                }
            }))
            .child(
                div()
                    .h(px(40.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(12.))
                    .bg(theme::abyss())
                    .border_b_1()
                    .border_color(if focused { theme::frost() } else { theme::line() })
                    .child(micro(label).text_color(if focused { theme::frost() } else { theme::mute() }))
                    .when_some(content.as_ref(), |d, c| {
                        d.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(theme::TEXT_CONTROL)
                                .text_color(theme::body())
                                .child(format!("{} · {} lines", c.label, thousands(c.lines() as u64))),
                        )
                    })
                    .when(content.is_none(), |d| d.child(div().flex_1()))
                    .child(btn("paste", "Paste", "Paste clipboard into this side").on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.paste_side(side, cx);
                    })))
                    .child(btn("open", "Open File…", "Load a file into this side").on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.open_file_side(side, cx);
                    })))
                    .when(content.is_some(), |d| {
                        d.child(btn("clear", "Clear", "Empty this side").on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.update_scratch(
                                |s| {
                                    match side {
                                        Side::Left => s.left = None,
                                        Side::Right => s.right = None,
                                    }
                                    s.focus = side;
                                },
                                cx,
                            );
                        })))
                    }),
            )
            .child(match content {
                Some(c) => preview(c).into_any_element(),
                None => div()
                    .flex_1()
                    .m(px(16.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .border_1()
                    .border_dashed()
                    .border_color(if focused { theme::frost() } else { theme::line_hi() })
                    .rounded(theme::RADIUS)
                    .child(div().text_size(theme::TEXT_LIST).text_color(theme::body()).child("Paste text or open a file"))
                    .child(
                        div()
                            .text_size(theme::TEXT_CONTROL)
                            .text_color(theme::mute())
                            .child(if focused { "Next paste lands here" } else { "Click to paste here next" }),
                    )
                    .into_any_element(),
            })
    }
}

fn preview(c: &ScratchSide) -> impl IntoElement {
    let text = String::from_utf8_lossy(&c.text);
    let total = c.lines();
    let lines: Vec<String> = text.lines().take(PREVIEW_LINES).map(|l| l.replace('\t', "    ")).collect();
    let digits = total.max(1).to_string().len().max(3);
    div()
        .id(SharedString::from(format!("preview-{}", c.label)))
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .py(px(4.))
        .text_size(theme::TEXT_CODE)
        .children(lines.into_iter().enumerate().map(move |(i, l)| {
            div()
                .h(theme::ROW_CODE)
                .flex()
                .items_center()
                .whitespace_nowrap()
                .overflow_hidden()
                .child(div().w(px(digits as f32 * 8.4 + 16.)).flex_none().pr(px(8.)).flex().justify_end().text_color(theme::mute()).child((i + 1).to_string()))
                .child(div().text_color(theme::body()).child(l))
        }))
        .when(total > PREVIEW_LINES, |d| {
            d.child(
                div()
                    .px(px(12.))
                    .py(px(6.))
                    .text_size(theme::TEXT_CONTROL)
                    .text_color(theme::mute())
                    .child(format!("… {} more lines (preview only — the diff uses everything)", thousands((total - PREVIEW_LINES) as u64))),
            )
        })
}
