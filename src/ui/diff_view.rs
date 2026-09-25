//! Diff pane: sticky file header, commit banner, and the virtualized diff body.

use super::app::{DiffState, Kerf, Loaded, Target};
use super::scrollbar::Bar;
use super::widgets::{self, age, bytes, counts, micro, seg, status_glyph, thousands};
use crate::diff::{self, Layout, Row};
use crate::git::{short_sha, ChangeStatus, DiffBody, LineKind};
use crate::highlight::Syn;
use crate::theme;
use gpui::{
    div, prelude::*, px, relative, uniform_list, AnyElement, Context, Div, FontStyle, FontWeight,
    HighlightStyle, Hsla, ListHorizontalSizingBehavior, SharedString, StyledText,
    Window,
};
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

/// Monospace advance at `TEXT_CODE` (JetBrains Mono ≈ 0.6em).
const CHAR_W: f32 = 8.4;

impl Kerf {
    pub fn render_diff_pane(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (target, loaded, loading, error) = match &self.diff {
            DiffState::Empty => (None, None, false, None),
            DiffState::Loading { target, prev, since } => {
                let show_spinner = since.elapsed() > Duration::from_millis(150);
                // Keep showing the previous diff (dimmed once slow) — never a blank flash.
                (Some(target.clone()), prev.clone().map(|p| (p, show_spinner)), show_spinner, None)
            }
            DiffState::Ready(l) => (Some(l.target.clone()), Some((l.clone(), false)), false, None),
            DiffState::Error { target, message } => (Some(target.clone()), None, false, Some(message.clone())),
        };
        let mut pane = div().flex_1().min_w_0().h_full().flex().flex_col().bg(theme::void());
        if !self.tabs.is_empty() {
            pane = pane.child(self.render_tab_bar(cx));
        }
        if let Some(sc) = self.active_scratch().filter(|s| !s.ready()) {
            return pane.child(self.render_composer(&sc, cx)).into_any_element();
        }
        let Some(target) = target else {
            return pane.child(self.render_welcome(cx)).into_any_element();
        };
        pane = pane.child(self.render_file_header(&target, loaded.as_ref().map(|l| &l.0), loading, cx));
        if let Some(c) = &target.commit {
            pane = pane.child(self.render_commit_banner(c, cx));
        }
        if let Some(msg) = error {
            return pane
                .child(notice("Could not diff this file", &msg, theme::del_fg()))
                .into_any_element();
        }
        match loaded {
            None => pane.child(div().flex_1()).into_any_element(),
            Some((l, dim)) => {
                // Stale diff for another file while loading → dim it.
                let stale = l.target.key() != target.key();
                pane.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .when(stale && dim, |d| d.opacity(0.5))
                        .child(self.render_body(&l, cx)),
                )
                .into_any_element()
            }
        }
    }

    /// Zed-style tab strip: preview tab in italics, close on hover/active, middle-click closes,
    /// double-click pins.
    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let nerd = self.nerd();
        div()
            .id("tab-bar")
            .h(theme::HEADER_H)
            .flex_none()
            .flex()
            .items_end()
            .bg(theme::abyss())
            .border_b_1()
            .border_color(theme::line())
            .overflow_x_scroll()
            .children(self.tabs.iter().enumerate().map(|(i, t)| {
                let active = self.active_tab == Some(i);
                let c = &t.target.change;
                let name = c.path.rsplit('/').next().unwrap_or(&c.path).to_string();
                let path = c.path.clone();
                let commit = t.target.commit.as_ref().map(|c| c.short());
                let tip = match &commit {
                    Some(sha) => format!("{path} @ {sha}"),
                    None => path,
                };
                div()
                    .id(("tab", i))
                    .group("tab")
                    .h_full()
                    .max_w(px(240.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .pl(px(12.))
                    .pr(px(6.))
                    .relative()
                    .border_r_1()
                    .border_color(theme::line())
                    .when(active, |d| {
                        d.bg(theme::crypt())
                            .child(div().absolute().top_0().left_0().right_0().h(px(2.)).bg(theme::frost()))
                    })
                    .when(!active, |d| d.hover(|s| s.bg(theme::ash())))
                    .cursor_pointer()
                    .tooltip(move |_, cx| cx.new(|_| TabTip(tip.clone())).into())
                    .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, _, cx| {
                        if ev.click_count() >= 2 {
                            if let Some(t) = this.tabs.get_mut(i) {
                                t.pinned = true;
                            }
                        }
                        this.activate_tab(i, cx);
                    }))
                    .on_mouse_down(
                        gpui::MouseButton::Middle,
                        cx.listener(move |this, _, _, cx| this.close_tab(i, cx)),
                    )
                    .child(status_glyph(c.status))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(theme::TEXT_LIST)
                            .text_color(if active { theme::bone() } else { theme::body() })
                            .when(!t.pinned, |d| d.italic())
                            .child(name),
                    )
                    .when_some(commit, |d, sha| {
                        d.child(
                            div()
                                .flex_none()
                                .text_size(theme::TEXT_CONTROL)
                                .text_color(theme::syn_type())
                                .child(format!("{} {sha}", widgets::ref_icon(super::app::RefLook::Commit, nerd))),
                        )
                    })
                    .child(
                        div()
                            .id(("tab-close", i))
                            .w(px(22.))
                            .h(px(22.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(theme::RADIUS)
                            .text_size(if nerd { theme::TEXT_CODE } else { theme::TEXT_LIST })
                            .text_color(theme::mute())
                            .when(!active, |d| d.invisible().group_hover("tab", |s| s.visible()))
                            .hover(|s| s.bg(theme::slate()).text_color(theme::bone()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_tab(i, cx);
                            }))
                            .child(if nerd { "\u{ea76}" } else { "✕" }),
                    )
            }))
    }

    /// Commit message panel: one-line header (collapsible), then subject + full body in a
    /// vertical-only scroll area whose max height the user drags from the bottom edge.
    fn render_commit_banner(&self, c: &crate::git::CommitInfo, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.banner_open;
        let body: String = c.message.lines().skip(1).skip_while(|l| l.trim().is_empty()).collect::<Vec<_>>().join("\n");
        let body = body.trim_end().to_string();
        let sha = c.oid.to_string();
        let header = div()
            .id("banner-header")
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(6.))
            .pr(px(12.))
            .cursor_pointer()
            .hover(|s| s.bg(theme::ash()))
            .on_click(cx.listener(|this, _, _, cx| {
                this.banner_open = !this.banner_open;
                this.persisted.banner_collapsed = !this.banner_open;
                this.persisted.save();
                cx.notify();
            }))
            .child(widgets::chevron(open, self.nerd()))
            .child(micro("Commit"))
            .child(
                div()
                    .id("banner-sha")
                    .flex_none()
                    .px(px(4.))
                    .rounded(theme::RADIUS)
                    .text_size(theme::TEXT_CONTROL)
                    .text_color(theme::frost())
                    .hover(|s| s.bg(theme::slate()))
                    .tooltip(|_, cx| cx.new(|_| widgets::Tip("Copy full SHA")).into())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(sha.clone()));
                        this.flash(format!("Copied {}", &sha[..7]), cx);
                    }))
                    .child(c.short()),
            )
            .child(div().flex_none().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(c.author.clone()))
            .child(div().flex_none().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(age(c.time)))
            .when(c.is_merge(), |d| {
                d.child(div().flex_none().text_size(theme::TEXT_MICRO).text_color(theme::frost()).child("MERGE · vs first parent"))
            })
            .when(!open, |d| {
                d.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_size(theme::TEXT_LIST)
                        .text_color(theme::bone())
                        .child(c.summary.clone()),
                )
            });

        div()
            .flex_none()
            .flex()
            .flex_col()
            .bg(theme::abyss())
            .border_b_1()
            .border_color(theme::line())
            .child(header)
            .when(open, |d| {
                d.child(
                    div()
                        .id("banner-body")
                        .max_h(px(self.banner_h))
                        .overflow_y_scroll()
                        .overflow_x_hidden()
                        .px(px(12.))
                        .pb(px(10.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(div().text_size(theme::TEXT_LIST).font_weight(FontWeight::MEDIUM).text_color(theme::bone()).child(c.summary.clone()))
                        .when(!body.is_empty(), |d| {
                            d.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .text_size(theme::TEXT_CONTROL)
                                    .text_color(theme::body())
                                    .children(body.lines().map(|l| {
                                        // Blank lines keep paragraph spacing.
                                        div().min_h(px(16.)).child(l.to_string())
                                    })),
                            )
                        }),
                )
                // Drag handle: sets the panel's max height (like the sidebar's width handle).
                .child(
                    div()
                        .id("banner-resize")
                        .h(px(5.))
                        .mt(px(-3.))
                        .cursor_row_resize()
                        .hover(|s| s.bg(theme::line_hi()))
                        .when(self.banner_drag.is_some(), |d| d.bg(theme::frost()))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|this, ev: &gpui::MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                this.banner_drag = Some((ev.position.y.into(), this.banner_h));
                                cx.notify();
                            }),
                        ),
                )
            })
    }

    fn render_welcome(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, sub) = if self.repo_path.is_none() {
            ("Open a repository", "Pick any local clone — Kerf is read-only. Or diff any two texts or files.")
        } else if self.range_data().is_some_and(|d| d.cmp.identical()) {
            ("Identical", "Base and compare point at the same commit.")
        } else if self.range_data().is_some_and(|d| d.changes.is_empty()) {
            ("No file changes", "The trees are identical in this view.")
        } else {
            ("Select a file", "Pick a file on the left — the diff shows here.")
        };
        let action = |id: &'static str, label: &'static str, sub: &'static str| {
            div()
                .id(id)
                .w(px(200.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .p(px(12.))
                .border_1()
                .border_color(theme::line_hi())
                .rounded(theme::RADIUS)
                .bg(theme::abyss())
                .cursor_pointer()
                .hover(|s| s.border_color(theme::frost()).bg(theme::crypt()))
                .child(div().text_size(theme::TEXT_LIST).font_weight(FontWeight::MEDIUM).text_color(theme::bone()).child(label))
                .child(div().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(sub))
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(16.))
            .child(div().text_size(theme::TEXT_DISPLAY).font_weight(FontWeight::BOLD).text_color(theme::bone()).child(title))
            .child(div().text_size(theme::TEXT_LIST).text_color(theme::mute()).child(sub))
            .child(
                div()
                    .mt(px(12.))
                    .flex()
                    .gap(px(12.))
                    .when(self.repo_path.is_none(), |d| {
                        d.child(
                            action("w-open", "Open Repository", "Compare branches & commits")
                                .on_click(cx.listener(|this, _, _, cx| this.prompt_open(cx))),
                        )
                    })
                    .child(action("w-new", "New Diff", "Paste two texts").on_click(cx.listener(|this, _, _, cx| this.new_scratch(cx))))
                    .child(
                        action("w-files", "Compare Files", "Pick any two files")
                            .on_click(cx.listener(|this, _, _, cx| this.prompt_compare_files(cx))),
                    ),
            )
    }

    fn render_file_header(
        &self,
        target: &Target,
        loaded: Option<&Arc<Loaded>>,
        loading: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &target.change;
        let scratch = target.scratch.clone();
        let path: SharedString = match (&scratch, &c.old_path, c.status) {
            (Some(sc), _, _) => match (&sc.left, &sc.right) {
                (Some(l), Some(r)) => format!("{}  ↔  {}", l.label, r.label).into(),
                _ => sc.title().into(),
            },
            (None, Some(old), ChangeStatus::Renamed | ChangeStatus::Copied) => format!("{old} → {}", c.path).into(),
            _ => c.path.clone().into(),
        };
        let (add, del) = match loaded.filter(|l| l.target.key() == target.key()) {
            Some(l) if matches!(l.fd.body, DiffBody::Text) => (Some(l.fd.additions() as u64), Some(l.fd.deletions() as u64)),
            _ => (c.additions.map(u64::from), c.deletions.map(u64::from)),
        };
        let split = self.layout == Layout::Split;
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
            .child(status_glyph(c.status))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme::bone())
                    .child(path),
            )
            .when_some(c.similarity, |d, s| d.child(div().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(format!("{s}%"))))
            .when(c.mode_changed(), |d| {
                d.child(
                    div()
                        .text_size(theme::TEXT_CONTROL)
                        .text_color(theme::mod_fg())
                        .child(format!("mode {:o} → {:o}", c.old_mode, c.new_mode)),
                )
            })
            .when(loaded.is_some_and(|l| l.fd.non_utf8), |d| {
                d.child(div().text_size(theme::TEXT_MICRO).text_color(theme::mod_fg()).child("NON-UTF-8"))
            })
            .child(counts(add, del))
            .when(loading, |d| d.child(widgets::spinner()))
            .child(div().w(px(8.)))
            .when(scratch.is_some(), |d| {
                d.child(seg("sc-edit", "Edit", false, "Back to the paste / open panes").on_click(cx.listener(|this, _, _, cx| this.edit_scratch(cx))))
                    .child(seg("sc-swap2", "⇄ Swap", false, "Swap left and right").on_click(cx.listener(|this, _, _, cx| this.swap_scratch(cx))))
                    .child(div().w(px(8.)))
            })
            .child(
                seg("unified", "Unified", !split, "Unified view  s")
                    .on_click(cx.listener(|this, _, _, cx| this.set_layout(Layout::Unified, cx))),
            )
            .child(
                seg("split", "Split", split, "Split view  s")
                    .on_click(cx.listener(|this, _, _, cx| this.set_layout(Layout::Split, cx))),
            )
            .child(
                seg("ws", "Ignore WS", self.ignore_ws, "Ignore whitespace  w")
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_ws(cx))),
            )
    }

    fn render_body(&self, l: &Arc<Loaded>, cx: &mut Context<Self>) -> AnyElement {
        let fd = &l.fd;
        match &fd.body {
            DiffBody::Binary => {
                return notice(
                    "Binary file",
                    &format!("{} → {}", bytes(fd.old_size), bytes(fd.new_size)),
                    theme::mute(),
                )
                .into_any_element()
            }
            DiffBody::Submodule { old, new } => {
                let s = |o: &Option<git2::Oid>| o.map(short_sha).unwrap_or_else(|| "∅".into());
                return notice("Submodule", &format!("{} → {}", s(old), s(new)), theme::frost()).into_any_element();
            }
            DiffBody::TooLarge => {
                return gate(
                    "Large diff",
                    &format!("{} → {} · above the {} safety limit", bytes(fd.old_size), bytes(fd.new_size), bytes(20 * 1024 * 1024)),
                    "Load anyway ↵",
                    cx,
                )
                .into_any_element()
            }
            DiffBody::Text => {}
        }
        if self.showing_collapsed_generated() && l.target.key() == self.current_target().map(|t| t.key()).unwrap_or_default() {
            let c = &l.target.change;
            return gate(
                "Generated file",
                &format!(
                    "+{} −{} · lockfiles and build output start collapsed",
                    thousands(fd.additions() as u64),
                    thousands(fd.deletions() as u64)
                ),
                "Show ↵",
                cx,
            )
            .when(c.is_generated(), |d| d)
            .into_any_element();
        }
        if fd.lines.is_empty() {
            let why = if l.target.change.mode_changed() {
                "Only the file mode changed."
            } else if self.ignore_ws {
                "Only whitespace changed (ignore-whitespace is on — press w)."
            } else if fd.old_size == 0 && fd.new_size == 0 {
                "Empty file."
            } else {
                "No textual changes."
            };
            return notice("No line changes", why, theme::mute()).into_any_element();
        }

        let count = l.rows.rows.len();
        let loaded = l.clone();
        let split = self.layout == Layout::Split && loaded.rows.rows.iter().any(|r| matches!(r, Row::Pair { .. }));
        let list = uniform_list(
            "diff",
            count,
            cx.processor(move |_this, range: Range<usize>, _, cx| {
                range.map(|ix| render_row(&loaded, ix, cx)).collect::<Vec<_>>()
            }),
        )
        .track_scroll(self.diff_scroll.clone())
        .size_full()
        .when(!split, |u| {
            u.with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .with_width_from_item(l.widest_row)
        });

        let mut foot: Vec<AnyElement> = Vec::new();
        if fd.old_no_newline || fd.new_no_newline {
            let which = match (fd.old_no_newline, fd.new_no_newline) {
                (true, true) => "Neither side ends with a newline",
                (true, false) => "Old side had no newline at end of file",
                _ => "New side has no newline at end of file",
            };
            foot.push(
                div()
                    .h(theme::ROW_CODE)
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(theme::line())
                    .text_size(theme::TEXT_CONTROL)
                    .text_color(theme::mute())
                    .child(format!("⏎ {which}"))
                    .into_any_element(),
            );
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(div().flex_1().min_w_0().h_full().child(list))
                    .child(self.render_minimap(l, cx))
                    .child(self.render_scrollbar(Bar::Diff, cx)),
            )
            .children(foot)
            .into_any_element()
    }

    /// Minimap column: consecutive changed rows merge into one block, coloured by content.
    fn render_minimap(&self, l: &Arc<Loaded>, cx: &mut Context<Self>) -> AnyElement {
        let blocks = change_blocks(l);
        let total = l.rows.rows.len().max(1) as f32;
        let marks = blocks
            .into_iter()
            .map(|(start, len, kind)| {
                div()
                    .absolute()
                    .left(px(4.))
                    .right(px(4.))
                    .top(relative(start as f32 / total))
                    .h(relative(len as f32 / total))
                    .min_h(px(2.))
                    .bg(if kind == LineKind::Added { theme::add_fg() } else { theme::del_fg() })
                    .into_any_element()
            })
            .collect();
        self.render_map(marks, cx)
    }
}

/// Runs of changed rows: (first row, row count, kind). Mixed pairs in split view count as Added.
fn change_blocks(l: &Loaded) -> Vec<(usize, usize, LineKind)> {
    let mut out: Vec<(usize, usize, LineKind)> = Vec::new();
    for (i, row) in l.rows.rows.iter().enumerate() {
        let kind = match row {
            Row::Line { idx, .. } => l.fd.lines[*idx].kind,
            Row::Pair { left, right } => {
                let r = right.as_ref().map(|c| l.fd.lines[c.idx].kind);
                let lk = left.as_ref().map(|c| l.fd.lines[c.idx].kind);
                match (lk, r) {
                    (_, Some(LineKind::Added)) => LineKind::Added,
                    (Some(LineKind::Removed), _) => LineKind::Removed,
                    _ => LineKind::Context,
                }
            }
            _ => LineKind::Context,
        };
        if kind == LineKind::Context {
            continue;
        }
        match out.last_mut() {
            Some((s, n, k)) if *k == kind && *s + *n == i => *n += 1,
            _ => out.push((i, 1, kind)),
        }
    }
    out
}

fn notice(title: &str, detail: &str, color: Hsla) -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .child(div().text_size(theme::TEXT_LIST).font_weight(FontWeight::BOLD).text_color(color).child(title.to_string()))
        .child(div().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(detail.to_string()))
}

fn gate(title: &str, detail: &str, action: &'static str, cx: &mut Context<Kerf>) -> Div {
    div().size_full().flex().items_center().justify_center().child(
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(16.))
            .border_1()
            .border_color(theme::line_hi())
            .rounded(theme::RADIUS)
            .bg(theme::crypt())
            .child(div().text_size(theme::TEXT_LIST).font_weight(FontWeight::BOLD).text_color(theme::mod_fg()).child(title.to_string()))
            .child(div().text_size(theme::TEXT_CONTROL).text_color(theme::body()).child(detail.to_string()))
            .child(
                div()
                    .id("gate-action")
                    .mt(px(4.))
                    .h(theme::CONTROL_H)
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .border_color(theme::frost())
                    .rounded(theme::RADIUS)
                    .text_size(theme::TEXT_CONTROL)
                    .text_color(theme::frost())
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::slate()))
                    .on_click(cx.listener(|this, _, _, cx| this.force_load(cx)))
                    .child(action),
            ),
    )
}

fn syn_style(s: Syn) -> HighlightStyle {
    let (color, weight, style) = match s {
        Syn::Keyword => (theme::syn_keyword(), Some(FontWeight::BOLD), None),
        Syn::Function => (theme::syn_function(), None, None),
        Syn::Type => (theme::syn_type(), None, None),
        Syn::String => (theme::syn_string(), None, None),
        Syn::Number => (theme::syn_number(), None, None),
        Syn::Comment => (theme::syn_comment(), None, Some(FontStyle::Italic)),
        Syn::Punct => (theme::syn_punct(), None, None),
        Syn::Attr => (theme::syn_attr(), None, None),
    };
    HighlightStyle { color: Some(color), font_weight: weight, font_style: style, ..Default::default() }
}

/// Builds the styled text for one diff line: truncation, tab expansion, syntax + word emphasis.
fn line_text(l: &Loaded, idx: usize, emph: &[Range<usize>]) -> StyledText {
    let raw = l.fd.line_text(&l.fd.lines[idx]);
    let kind = l.fd.lines[idx].kind;
    let (shown, hidden) = diff::truncate(raw);
    let limit = shown.len();
    let emph_bg = match kind {
        LineKind::Added => theme::add_emph(),
        LineKind::Removed => theme::del_emph(),
        LineKind::Context => theme::ash(),
    };
    let mut spans: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    if let Some(hl) = &l.hl {
        for (r, s) in &hl[idx] {
            if r.start < limit {
                spans.push((r.start..r.end.min(limit), syn_style(*s)));
            }
        }
    }
    let emph: Vec<(Range<usize>, HighlightStyle)> = emph
        .iter()
        .filter(|r| r.start < limit)
        .map(|r| (r.start..r.end.min(limit), HighlightStyle { background_color: Some(emph_bg), ..Default::default() }))
        .collect();
    let combined: Vec<(Range<usize>, HighlightStyle)> = gpui::combine_highlights(spans, emph).collect();
    let (mut text, map) = diff::expand_tabs(shown);
    let mut combined: Vec<(Range<usize>, HighlightStyle)> = match map {
        Some(m) => combined.into_iter().map(|(r, s)| (m[r.start]..m[r.end], s)).collect(),
        None => combined,
    };
    if hidden > 0 {
        let start = text.len();
        text.push_str(&format!("  … {} more chars", thousands(hidden as u64)));
        combined.push((start..text.len(), HighlightStyle { color: Some(theme::mute()), font_style: Some(FontStyle::Italic), ..Default::default() }));
    }
    StyledText::new(SharedString::from(text)).with_highlights(combined)
}

fn gutter(n: Option<u32>, digits: usize) -> Div {
    div()
        .w(px(digits as f32 * CHAR_W + 16.))
        .flex_none()
        .pr(px(8.))
        .flex()
        .justify_end()
        .text_color(theme::mute())
        .child(n.map(|n| n.to_string()).unwrap_or_default())
}

fn line_bg(kind: LineKind) -> Hsla {
    match kind {
        LineKind::Added => theme::add_bg(),
        LineKind::Removed => theme::del_bg(),
        LineKind::Context => theme::void(),
    }
}

fn sign(kind: LineKind) -> Div {
    let (s, c) = match kind {
        LineKind::Added => ("+", theme::add_fg()),
        LineKind::Removed => ("−", theme::del_fg()),
        LineKind::Context => (" ", theme::mute()),
    };
    div().w(px(CHAR_W * 2.)).flex_none().text_color(c).child(s)
}

fn render_row(l: &Arc<Loaded>, ix: usize, cx: &mut Context<Kerf>) -> AnyElement {
    let row_base = || {
        div()
            .id(ix)
            .h(theme::ROW_CODE)
            .min_w_full()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .text_size(theme::TEXT_CODE)
    };
    match &l.rows.rows[ix] {
        Row::Hunk(_) if l.fd.unchanged.is_some() => {
            let (text, color) = match l.fd.unchanged {
                Some(crate::git::Unchanged::WhitespaceOnly) => {
                    ("Only whitespace differs — ignored. Press w to show whitespace changes.", theme::mod_fg())
                }
                _ => ("Identical — no differences. Showing the full content.", theme::frost()),
            };
            row_base()
                .bg(theme::crypt())
                .pl(px(12.))
                .gap(px(8.))
                .text_size(theme::TEXT_CONTROL)
                .text_color(color)
                .child("●")
                .child(text)
                .into_any_element()
        }
        Row::Hunk(h) => {
            let h = &l.fd.hunks[*h];
            row_base()
                .bg(theme::crypt())
                .pl(px(12.))
                .gap(px(12.))
                .text_color(theme::mute())
                .child(format!("@@ -{},{} +{},{} @@", h.old_start, h.old_lines, h.new_start, h.new_lines))
                .child(div().text_color(theme::body()).child(h.context.clone()))
                .into_any_element()
        }
        Row::Gap { hidden } => row_base()
            .justify_center()
            .text_size(theme::TEXT_CONTROL)
            .text_color(theme::mute())
            .cursor_pointer()
            .hover(|s| s.text_color(theme::body()).bg(theme::abyss()))
            .on_click(cx.listener(|this, _, _, cx| this.expand_context(cx)))
            .child(format!("┄┄┄  ⋯ {} unchanged lines · click to expand  ┄┄┄", thousands(*hidden as u64)))
            .into_any_element(),
        Row::Line { idx, emph } => {
            let line = &l.fd.lines[*idx];
            row_base()
                .bg(line_bg(line.kind))
                .child(gutter(line.old_no, l.gutter_digits))
                .child(gutter(line.new_no, l.gutter_digits))
                .child(sign(line.kind))
                .child(div().pr(px(24.)).child(line_text(l, *idx, emph)))
                .into_any_element()
        }
        Row::Pair { left, right } => {
            let half = |cell: &Option<diff::Cell>, old: bool| {
                let base = div().flex_1().min_w_0().h_full().flex().items_center().overflow_hidden();
                match cell {
                    None => base.bg(theme::abyss()),
                    Some(c) => {
                        let line = &l.fd.lines[c.idx];
                        let kind = line.kind;
                        base.bg(line_bg(kind))
                            .child(gutter(if old { line.old_no } else { line.new_no }, l.gutter_digits))
                            .child(sign(kind))
                            .child(line_text(l, c.idx, &c.emph))
                    }
                }
            };
            row_base()
                .w_full()
                .child(half(left, true))
                .child(div().w(px(1.)).h_full().flex_none().bg(theme::line_hi()))
                .child(half(right, false))
                .into_any_element()
        }
    }
}

struct TabTip(String);

impl Render for TabTip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(4.))
            .bg(theme::crypt())
            .border_1()
            .border_color(theme::line_hi())
            .rounded(theme::RADIUS)
            .text_size(theme::TEXT_CONTROL)
            .text_color(theme::body())
            .child(self.0.clone())
    }
}
