//! Sidebar: repo, range bar, tabs and the virtualized file / commit list. Also titlebar + status bar.

use super::app::{Group, Input, Kerf, ListRow, RangeState, Tab, Which};
use super::widgets::{self, age, counts, micro, seg, status_glyph};
use crate::git::{short_sha, RangeMode};
use crate::theme;
use gpui::{
    div, prelude::*, px, uniform_list, AnyElement, Context, MouseButton, SharedString, Window,
};

impl Kerf {
    pub fn render_titlebar(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let range = match (&self.base, &self.compare) {
            (Some(b), Some(c)) => format!("{b} … {c}"),
            _ => String::new(),
        };
        div()
            .id("titlebar")
            .h(theme::TITLEBAR_H)
            .flex_none()
            .flex()
            .items_center()
            .pl(px(80.))
            .pr(px(12.))
            .gap(px(8.))
            .bg(theme::abyss())
            .border_b_1()
            .border_color(theme::line())
            .on_mouse_down(MouseButton::Left, |ev, window, _| {
                if ev.click_count == 2 {
                    window.titlebar_double_click();
                } else {
                    window.start_window_move();
                }
            })
            .child(div().text_size(theme::TEXT_LIST).font_weight(gpui::FontWeight::BOLD).text_color(theme::bone()).child("kerf"))
            .when(!self.repo_name.is_empty(), |d| {
                d.child(div().text_size(theme::TEXT_LIST).text_color(theme::faint()).child("/"))
                    .child(div().text_size(theme::TEXT_LIST).text_color(theme::body()).child(self.repo_name.clone()))
            })
            .when(!range.is_empty(), |d| {
                d.child(div().text_size(theme::TEXT_LIST).text_color(theme::mute()).child(range))
            })
            .child(div().flex_1())
            .when_some(self.flash.as_ref().map(|f| f.0.clone()), |d, msg| {
                d.child(div().text_size(theme::TEXT_CONTROL).text_color(theme::frost()).child(msg))
            })
    }

    pub fn render_status(&mut self, _: &mut Context<Self>) -> impl IntoElement {
        let mut left: Vec<AnyElement> = Vec::new();
        if let (Some(b), Some(c)) = (&self.base, &self.compare) {
            left.push(div().child(format!("⎇ {b}…{c}")).into_any_element());
        }
        if let Some(d) = self.range_data() {
            left.push(div().child(format!("{} files", widgets::thousands(d.changes.len() as u64))).into_any_element());
            left.push(counts(Some(d.additions), Some(d.deletions)).into_any_element());
            if d.cmp.unrelated() {
                left.push(div().text_color(theme::mod_fg()).child("no common ancestor · two-dot").into_any_element());
            }
        }
        let viewed = self.range_data().map(|d| (self.viewed.len(), d.changes.len()));
        div()
            .h(theme::STATUS_H)
            .flex_none()
            .flex()
            .items_center()
            .gap(px(16.))
            .px(px(12.))
            .bg(theme::abyss())
            .border_t_1()
            .border_color(theme::line())
            .text_size(theme::TEXT_MICRO)
            .text_color(theme::mute())
            .children(left)
            .child(div().flex_1())
            .when_some(viewed.filter(|(_, t)| *t > 0), |d, (v, t)| d.child(format!("{v}/{t} viewed")))
            .child(if self.layout == crate::diff::Layout::Split { "split" } else { "unified" })
            .child(if self.ignore_ws { "ws: ignored" } else { "ws: shown" })
    }

    pub fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(self.sidebar_w))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::abyss())
            .border_r_1()
            .border_color(theme::line())
            .child(self.render_repo_section(cx))
            .when(self.repo_path.is_some(), |d| {
                d.child(self.render_range_bar(cx))
                    .child(self.render_tabs(cx))
                    .child(self.render_list(window, cx))
            })
    }

    fn render_repo_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let name = if self.repo_path.is_some() { self.repo_name.clone() } else { "No repository".into() };
        let path = self.repo_path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        div()
            .flex_none()
            .px(px(12.))
            .pt(px(8.))
            .pb(px(8.))
            .border_b_1()
            .border_color(theme::line())
            .child(micro("Repository").mb(px(4.)))
            .child(
                div()
                    .id("repo")
                    .h(theme::ROW_LIST)
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(8.))
                    .mx(px(-8.))
                    .rounded(theme::RADIUS)
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::ash()))
                    .on_click(cx.listener(|this, _, _, cx| this.prompt_open(cx)))
                    .tooltip(|_, cx| cx.new(|_| widgets::Tip("Open repository  ⌘O")).into())
                    .child(div().text_color(theme::mute()).text_size(theme::TEXT_LIST).child("▸"))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(theme::TEXT_LIST)
                            .text_color(if self.repo_path.is_some() { theme::bone() } else { theme::body() })
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(name),
                    )
                    .when(self.repo_loading, |d| d.child(widgets::spinner()))
                    .child(div().text_size(theme::TEXT_MICRO).text_color(theme::mute()).child("⌘O")),
            )
            .when(!path.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(theme::TEXT_MICRO)
                        .text_color(theme::faint())
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(path),
                )
            })
            .when_some(self.repo_error.clone(), |d, e| {
                d.child(div().mt(px(6.)).text_size(theme::TEXT_CONTROL).text_color(theme::del_fg()).child(e))
            })
            .when(self.repo_path.is_none(), |d| d.child(self.render_recents(cx)))
    }

    fn render_recents(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let recents = self.persisted.recents.clone();
        div()
            .mt(px(12.))
            .when(!recents.is_empty(), |d| d.child(micro("Recent").mb(px(4.))))
            .children(recents.into_iter().enumerate().map(|(i, p)| {
                let exists = p.exists();
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let p2 = p.clone();
                div()
                    .id(("recent", i))
                    .h(theme::ROW_LIST)
                    .flex()
                    .items_center()
                    .px(px(8.))
                    .mx(px(-8.))
                    .rounded(theme::RADIUS)
                    .text_size(theme::TEXT_LIST)
                    .text_color(if exists { theme::body() } else { theme::faint() })
                    .when(!exists, |d| d.line_through())
                    .when(exists, |d| {
                        d.cursor_pointer()
                            .hover(|s| s.bg(theme::ash()).text_color(theme::bone()))
                            .on_click(cx.listener(move |this, _, _, cx| this.open_repo(p2.clone(), cx)))
                    })
                    .child(name)
            }))
    }

    fn render_range_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.range_data().cloned();
        let unrelated = data.as_ref().is_some_and(|d| d.cmp.unrelated());
        let field = |this: &Kerf, which: Which, cx: &mut Context<Kerf>| {
            let (label, value, key) = match which {
                Which::Base => ("Base", this.base.clone(), "⌘1"),
                Which::Compare => ("Compare", this.compare.clone(), "⌘2"),
            };
            let open = this.picker.as_ref().is_some_and(|p| p.which == which);
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(micro(label).w(px(52.)).flex_none())
                .child(
                    div()
                        .id(label)
                        .flex_1()
                        .min_w_0()
                        .h(theme::CONTROL_H)
                        .flex()
                        .items_center()
                        .px(px(8.))
                        .gap(px(6.))
                        .bg(theme::crypt())
                        .border_1()
                        .border_color(if open { theme::frost() } else { theme::line_hi() })
                        .rounded(theme::RADIUS)
                        .cursor_pointer()
                        .hover(|s| s.border_color(theme::mute()))
                        .on_click(cx.listener(move |this, _, _, cx| this.open_picker(which, cx)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(theme::TEXT_LIST)
                                .text_color(if value.is_some() { theme::bone() } else { theme::mute() })
                                .child(value.unwrap_or_else(|| "pick a branch…".into())),
                        )
                        .child(div().text_size(theme::TEXT_MICRO).text_color(theme::faint()).child(key))
                        .child(div().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child("▾")),
                )
        };
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(4.))
            .px(px(12.))
            .py(px(8.))
            .border_b_1()
            .border_color(theme::line())
            .child(field(self, Which::Base, cx))
            .child(
                div().flex().pl(px(60.)).child(
                    seg("swap", "⇄ swap", false, "Swap base and compare  ⌘⇧S")
                        .on_click(cx.listener(|this, _, _, cx| this.swap(cx))),
                ),
            )
            .child(field(self, Which::Compare, cx))
            .child(
                div()
                    .mt(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        seg("3dot", "…", self.mode == RangeMode::ThreeDot && !unrelated, "Three-dot: changes introduced by compare  ⌘⇧M")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.mode != RangeMode::ThreeDot {
                                    this.toggle_mode(cx)
                                }
                            })),
                    )
                    .child(
                        seg("2dot", "..", self.mode == RangeMode::TwoDot || unrelated, "Two-dot: tip-to-tip tree difference  ⌘⇧M")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.mode != RangeMode::TwoDot {
                                    this.toggle_mode(cx)
                                }
                            })),
                    )
                    .child(div().flex_1())
                    .map(|d| match (&self.range, &data) {
                        (RangeState::Loading, _) => d.child(widgets::spinner()),
                        (_, Some(data)) => d
                            .child(
                                div()
                                    .text_size(theme::TEXT_CONTROL)
                                    .text_color(theme::add_fg())
                                    .child(format!("↑{}", data.cmp.ahead.len())),
                            )
                            .child(
                                div()
                                    .text_size(theme::TEXT_CONTROL)
                                    .text_color(theme::mod_fg())
                                    .child(format!("↓{}", data.cmp.behind.len())),
                            ),
                        _ => d,
                    }),
            )
            .when_some(data.as_ref(), |d, data| {
                let mb = match data.cmp.merge_base {
                    Some(o) => format!("merge base {}", short_sha(o)),
                    None => "no common ancestor — two-dot diff".into(),
                };
                d.child(
                    div()
                        .text_size(theme::TEXT_MICRO)
                        .text_color(if unrelated { theme::mod_fg() } else { theme::mute() })
                        .child(mb),
                )
            })
            .when_some(
                match &self.range {
                    RangeState::Error(e) => Some(e.clone()),
                    _ => None,
                },
                |d, e| d.child(div().text_size(theme::TEXT_CONTROL).text_color(theme::del_fg()).child(e)),
            )
    }

    fn render_tabs(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (files, commits) = self
            .range_data()
            .map(|d| (d.changes.len(), d.cmp.ahead.len() + d.cmp.behind.len()))
            .unwrap_or((0, 0));
        let tab = |this: &Kerf, t: Tab, label: &'static str, n: usize, cx: &mut Context<Kerf>| {
            let active = this.tab == t;
            div()
                .id(label)
                .h(px(28.))
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(4.))
                .cursor_pointer()
                .border_b_1()
                .border_color(if active { theme::frost() } else { gpui::transparent_black() })
                .on_click(cx.listener(move |this, _, _, cx| this.set_tab(t, cx)))
                .child(
                    div()
                        .text_size(theme::TEXT_MICRO)
                        .text_color(if active { theme::bone() } else { theme::mute() })
                        .child(label),
                )
                .child(div().text_size(theme::TEXT_MICRO).text_color(theme::faint()).child(widgets::thousands(n as u64)))
        };
        let filtering = self.input == Input::Filter;
        div()
            .flex_none()
            .border_b_1()
            .border_color(theme::line())
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(12.))
                    .px(px(12.))
                    .child(tab(self, Tab::Files, "FILES", files, cx))
                    .child(tab(self, Tab::Commits, "COMMITS", commits, cx))
                    .child(div().flex_1())
                    .when(self.tab == Tab::Files, |d| {
                        d.child(
                            div()
                                .flex()
                                .pb(px(3.))
                                .child(
                                    seg("tree", "⊟", self.tree, "Tree view  t")
                                        .on_click(cx.listener(|this, _, w, cx| {
                                            if !this.tree {
                                                w.dispatch_action(Box::new(super::ToggleTree), cx);
                                            }
                                        })),
                                )
                                .child(
                                    seg("flat", "≣", !self.tree, "Flat list  t")
                                        .on_click(cx.listener(|this, _, w, cx| {
                                            if this.tree {
                                                w.dispatch_action(Box::new(super::ToggleTree), cx);
                                            }
                                        })),
                                ),
                        )
                    }),
            )
            .when(self.tab == Tab::Files, |d| {
                d.child(
                    div()
                        .id("filter")
                        .mx(px(12.))
                        .my(px(6.))
                        .h(theme::CONTROL_H)
                        .flex()
                        .items_center()
                        .px(px(8.))
                        .bg(theme::crypt())
                        .border_1()
                        .border_color(if filtering { theme::frost() } else { theme::line_hi() })
                        .rounded(theme::RADIUS)
                        .cursor_text()
                        .text_size(theme::TEXT_LIST)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.input = Input::Filter;
                            cx.notify();
                        }))
                        .map(|d| {
                            if self.filter.is_empty() && !filtering {
                                d.text_color(theme::mute()).child("filter files…").child(div().flex_1()).child(
                                    div().text_size(theme::TEXT_MICRO).text_color(theme::faint()).child("/"),
                                )
                            } else {
                                d.text_color(theme::bone())
                                    .child(self.filter.clone())
                                    .when(filtering, |d| d.child(div().w(px(1.)).h(px(14.)).bg(theme::frost())))
                            }
                        }),
                )
            })
    }

    fn render_list(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let loading = matches!(self.range, RangeState::Loading) && self.rows.is_empty();
        if loading {
            return div()
                .flex_1()
                .flex()
                .flex_col()
                .children((0..8).map(|i| {
                    div()
                        .h(theme::ROW_LIST)
                        .mx(px(12.))
                        .flex()
                        .items_center()
                        .child(div().h(px(8.)).w(px(80. + (i * 37 % 120) as f32)).bg(theme::ash()).rounded(theme::RADIUS))
                }))
                .into_any_element();
        }
        if matches!(self.range, RangeState::Idle) {
            return div()
                .flex_1()
                .p(px(12.))
                .text_size(theme::TEXT_LIST)
                .text_color(theme::mute())
                .child("Pick a base and compare branch.")
                .into_any_element();
        }
        let count = self.rows.len();
        uniform_list(
            "list",
            count,
            cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                range.map(|ix| this.render_row(ix, cx)).collect::<Vec<_>>()
            }),
        )
        .track_scroll(self.list_scroll.clone())
        .flex_1()
        .into_any_element()
    }

    fn render_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let row = &self.rows[ix];
        let selected = self.selected == Some(ix);
        let base = div()
            .id(("row", ix))
            .h(theme::ROW_LIST)
            .w_full()
            .flex()
            .items_center()
            .gap(px(6.))
            .pr(px(12.))
            .text_size(theme::TEXT_LIST)
            .text_color(if selected { theme::bone() } else { theme::body() })
            .border_l_2()
            .border_color(if selected { theme::frost() } else { gpui::transparent_black() })
            .when(selected, |d| d.bg(theme::slate()))
            .when(row.selectable(), |d| {
                d.cursor_pointer()
                    .when(!selected, |d| d.hover(|s| s.bg(theme::ash())))
                    .on_click(cx.listener(move |this, _, _, cx| this.click_row(ix, cx)))
            });
        let indent = |depth: usize| px(10. + depth as f32 * 12.);
        match row {
            ListRow::Summary => {
                let d = self.range_data();
                base.pl(px(12.))
                    .text_size(theme::TEXT_CONTROL)
                    .text_color(theme::mute())
                    .child(format!(
                        "{} files",
                        d.map(|d| widgets::thousands(d.changes.len() as u64)).unwrap_or_default()
                    ))
                    .child(counts(d.map(|d| d.additions), d.map(|d| d.deletions)))
                    .into_any_element()
            }
            ListRow::Notice(n) => base
                .pl(px(14.))
                .text_size(theme::TEXT_CONTROL)
                .text_color(theme::mute())
                .child(n.clone())
                .into_any_element(),
            ListRow::Dir { name, depth, collapsed, .. } => base
                .pl(indent(*depth))
                .text_color(if selected { theme::bone() } else { theme::mute() })
                .child(div().w(px(10.)).flex_none().child(if *collapsed { "▸" } else { "▾" }))
                .child(div().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().child(format!("{name}/")))
                .into_any_element(),
            ListRow::File { change, depth } => {
                let Some(data) = self.range_data() else { return base.into_any_element() };
                let c = &data.changes[*change];
                let name: SharedString = if self.tree && self.filter.is_empty() {
                    c.path.rsplit('/').next().unwrap_or(&c.path).to_string().into()
                } else {
                    c.path.clone().into()
                };
                let viewed = self.viewed.contains(&c.path);
                let label = match (&c.old_path, c.similarity) {
                    (Some(old), Some(sim)) if !self.tree || !self.filter.is_empty() => {
                        format!("{old} → {name}  {sim}%")
                    }
                    (Some(_), Some(sim)) => format!("{name}  {sim}%"),
                    _ => name.to_string(),
                };
                let path = c.path.clone();
                base.pl(indent(*depth))
                    .tooltip(move |_, cx| cx.new(|_| PathTip(path.clone())).into())
                    .child(status_glyph(c.status))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .when(viewed && !selected, |d| d.text_color(theme::mute()))
                            .child(label),
                    )
                    .when(c.binary, |d| d.child(div().text_size(theme::TEXT_MICRO).text_color(theme::mute()).child("BIN")))
                    .when(c.is_generated(), |d| d.child(div().text_size(theme::TEXT_MICRO).text_color(theme::faint()).child("GEN")))
                    .child(counts(c.additions.map(u64::from), c.deletions.map(u64::from)))
                    .when(viewed, |d| d.child(div().text_size(theme::TEXT_CONTROL).text_color(theme::faint()).child("✓")))
                    .into_any_element()
            }
            ListRow::Group { group, count, open } => base
                .pl(px(10.))
                .text_size(theme::TEXT_MICRO)
                .text_color(theme::mute())
                .child(div().w(px(10.)).child(if *open { "▾" } else { "▸" }))
                .child(match group {
                    Group::Ahead => "AHEAD · IN COMPARE, NOT BASE",
                    Group::Behind => "BEHIND · IN BASE, NOT COMPARE",
                })
                .child(div().flex_1())
                .child(div().text_color(match group {
                    Group::Ahead => theme::add_fg(),
                    Group::Behind => theme::mod_fg(),
                }).child(count.to_string()))
                .into_any_element(),
            ListRow::Commit { group, idx } => {
                let Some(c) = self.commit(*group, *idx) else { return base.into_any_element() };
                let open = self.expanded.as_ref().is_some_and(|e| e.oid == c.oid);
                base.pl(px(10.))
                    .child(div().w(px(10.)).flex_none().text_color(theme::mute()).child(if open { "▾" } else { "▸" }))
                    .child(div().flex_none().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(c.short()))
                    .when(c.is_merge(), |d| d.child(div().flex_none().text_size(theme::TEXT_MICRO).text_color(theme::frost()).child("MERGE")))
                    .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().child(c.summary.clone()))
                    .child(div().flex_none().text_size(theme::TEXT_CONTROL).text_color(theme::faint()).child(age(c.time)))
                    .into_any_element()
            }
            ListRow::CommitFile { change } => {
                let Some(c) = self.expanded.as_ref().and_then(|e| e.changes.as_ref()).and_then(|ch| ch.get(*change)) else {
                    return base.into_any_element();
                };
                base.pl(px(34.))
                    .child(status_glyph(c.status))
                    .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().child(c.path.clone()))
                    .child(counts(c.additions.map(u64::from), c.deletions.map(u64::from)))
                    .into_any_element()
            }
        }
    }
}

struct PathTip(String);

impl Render for PathTip {
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
