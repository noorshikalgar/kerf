//! Sidebar: repo, range bar, tabs and the virtualized file / commit list. Also titlebar + status bar.

use super::app::{Group, Input, Kerf, ListRow, RangeState, RefLook, Tab, Which};
use super::widgets::{self, age, counts, micro, seg, status_glyph};
use crate::git::{short_sha, RangeMode};
use crate::theme;
use gpui::{
    div, prelude::*, px, uniform_list, AnyElement, Context, HighlightStyle, MouseButton, SharedString,
    StyledText, Window,
};

impl Kerf {
    /// Narrow sidebar: drop secondary details (SHAs, ages, badges, ratio bar, long labels).
    fn compact(&self) -> bool {
        self.sidebar_w < theme::SIDEBAR_COMPACT
    }

    pub fn render_titlebar(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let range = match (&self.base, &self.compare) {
            (Some(b), Some(c)) => format!("{} … {}", self.display_ref(b), self.display_ref(c)),
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
            .child(div().text_size(theme::TEXT_LIST).text_color(theme::faint()).child("/"))
            // Repository switcher: always reachable, from git or plain-diff mode.
            .child({
                let has_repo = self.repo_path.is_some();
                let nerd = self.nerd();
                div()
                    .id("repo-switcher")
                    .h(px(24.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(theme::RADIUS)
                    .text_size(theme::TEXT_LIST)
                    .text_color(if has_repo { theme::body() } else { theme::mute() })
                    .when(self.repo_menu_open, |d| d.bg(theme::slate()).text_color(theme::bone()))
                    .hover(|s| s.bg(theme::ash()).text_color(theme::bone()))
                    .cursor_pointer()
                    .tooltip(|_, cx| cx.new(|_| widgets::Tip("Switch or open a repository  ⌘O")).into())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.repo_menu_open = !this.repo_menu_open;
                        cx.notify();
                    }))
                    .child(if has_repo { self.repo_name.clone() } else { "Open Repository".into() })
                    .child(widgets::chevron(true, nerd))
            })
            .when(!range.is_empty(), |d| {
                d.child(div().text_size(theme::TEXT_LIST).text_color(theme::mute()).child(range))
            })
            .child(div().flex_1())
            .when_some(self.flash.as_ref().map(|f| f.0.clone()), |d, msg| {
                d.child(div().text_size(theme::TEXT_CONTROL).text_color(theme::frost()).child(msg))
            })
            // Buttons swallow mouse-down so the titlebar doesn't start a window drag.
            .child(
                seg("tb-new", "New Diff", false, "Paste two texts and diff them  ⌘N")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| this.new_scratch(cx))),
            )
            .child(
                seg("tb-files", "Compare Files", false, "Pick two files and diff them  ⌥⌘N")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| this.prompt_compare_files(cx))),
            )
    }

    pub fn render_status(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut left: Vec<AnyElement> = Vec::new();
        if let (Some(b), Some(c)) = (&self.base, &self.compare) {
            left.push(div().child(format!("{} … {}", self.display_ref(b), self.display_ref(c))).into_any_element());
            let unrelated = self.range_data().is_some_and(|d| d.cmp.unrelated());
            let mode = if unrelated { RangeMode::Compare } else { self.mode };
            left.push(div().text_color(theme::body()).child(mode.label()).into_any_element());
        }
        if let Some(d) = self.range_data() {
            left.push(div().child(format!("{} files", widgets::thousands(d.changes.len() as u64))).into_any_element());
            left.push(counts(Some(d.additions), Some(d.deletions)).into_any_element());
            if d.cmp.unrelated() {
                left.push(div().text_color(theme::mod_fg()).child("no common ancestor").into_any_element());
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
            .when(self.wrap, |d| d.child("wrap"))
            .child(
                div()
                    .id("shortcuts-link")
                    .px(px(6.))
                    .rounded(theme::RADIUS)
                    .text_color(if self.shortcuts_open { theme::frost() } else { theme::body() })
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::ash()).text_color(theme::bone()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.shortcuts_open = !this.shortcuts_open;
                        cx.notify();
                    }))
                    .child("Shortcuts"),
            )
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
                    .when(self.tab == Tab::Files && self.range_data().is_some(), |d| d.child(self.render_list_toolbar(cx)))
                    .child(self.render_list(window, cx))
            })
    }

    fn render_repo_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_repo = self.repo_path.is_some();
        let name = if has_repo { self.repo_name.clone() } else { "No Repository".into() };
        let path = self.repo_path.as_ref().map(|p| super::scratch::tilde(&p.display().to_string())).unwrap_or_default();
        let head = self.refs.iter().find(|r| r.is_head).map(|r| r.name.clone());
        let nerd = self.nerd();
        div()
            .flex_none()
            .px(px(12.))
            .pt(px(10.))
            .pb(px(10.))
            .border_b_1()
            .border_color(theme::line())
            .child(
                div()
                    .id("repo")
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .px(px(8.))
                    .py(px(6.))
                    .mx(px(-8.))
                    .rounded(theme::RADIUS)
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::ash()))
                    .on_click(cx.listener(|this, _, _, cx| this.prompt_open(cx)))
                    .tooltip(|_, cx| cx.new(|_| widgets::Tip("Open another repository  ⌘O")).into())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_size(theme::TEXT_CODE)
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(if has_repo { theme::bone() } else { theme::body() })
                                    .child(name),
                            )
                            .when(self.repo_loading, |d| d.child(widgets::spinner()))
                            .child(
                                div()
                                    .id("close-repo")
                                    .flex_none()
                                    .w(px(22.))
                                    .h(px(22.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(theme::RADIUS)
                                    .text_color(theme::mute())
                                    .hover(|s| s.bg(theme::slate()).text_color(theme::bone()))
                                    .tooltip(|_, cx| cx.new(|_| widgets::Tip("Close repository")).into())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.close_repo(cx);
                                    }))
                                    .child(if nerd { "\u{ea76}" } else { "✕" }),
                            ),
                    )
                    .when(has_repo, |d| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .text_size(theme::TEXT_CONTROL)
                                .text_color(theme::mute())
                                .child(div().flex_shrink().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().child(path))
                                .when_some(head, |d, h| {
                                    d.child(div().flex_none().text_color(theme::faint()).child("·")).child(
                                        div()
                                            .flex()
                                            .flex_none()
                                            .gap(px(4.))
                                            .text_color(theme::body())
                                            .child(div().text_color(theme::add_fg()).child(widgets::ref_icon(RefLook::Branch, nerd)))
                                            .child(h),
                                    )
                                }),
                        )
                    }),
            )
            .when_some(self.repo_error.clone(), |d, e| {
                d.child(div().mt(px(6.)).text_size(theme::TEXT_CONTROL).text_color(theme::del_fg()).child(e))
            })
    }

    /// Two-line picker field: label + shortcut on top, icon + ref (+ subject for commits) + SHA below.
    fn range_field(&self, which: Which, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = self.compact();
        let (label, value, tip) = match which {
            Which::Base => ("Base", self.base.clone(), "Pick base — branch, tag or commit  ⌘1"),
            Which::Compare => ("Compare", self.compare.clone(), "Pick compare — branch, tag or commit  ⌘2"),
        };
        let open = self.picker.as_ref().is_some_and(|p| p.which == which);
        let d = value.as_deref().map(|v| self.describe(v));
        let nerd = self.nerd();
        div()
            .id(SharedString::from(format!("field-{label}")))
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(10.))
            .py(px(6.))
            .bg(theme::crypt())
            .border_1()
            .border_color(if open { theme::frost() } else { theme::line_hi() })
            .rounded(theme::RADIUS)
            .cursor_pointer()
            .hover(|s| s.border_color(theme::mute()))
            .on_click(cx.listener(move |this, _, _, cx| this.open_picker(which, cx)))
            .tooltip(move |_, cx| cx.new(|_| widgets::Tip(tip)).into())
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(micro(label))
                    .child(div().flex_1())
                    .child(widgets::chevron(true, nerd).ml(px(4.))),
            )
            .child(match d {
                None => div().text_size(theme::TEXT_LIST).text_color(theme::mute()).child("Pick a branch or commit…"),
                Some(d) => div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(theme::TEXT_LIST)
                    .child(div().flex_none().text_color(widgets::ref_icon_color(d.look)).child(widgets::ref_icon(d.look, nerd)))
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(if d.subject.is_some() { 80. } else { 400. }))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme::bone())
                            .child(d.name.clone()),
                    )
                    .when_some(d.subject.clone(), |x, subj| {
                        x.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_color(theme::body())
                                .child(subj),
                        )
                    })
                    .when(d.subject.is_none(), |x| x.child(div().flex_1()))
                    .when_some(d.sha.filter(|_| d.look != RefLook::Commit && !compact), |x, sha| {
                        x.child(div().flex_none().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(sha))
                    }),
            })
    }

    /// Collapsible "Comparison" section: what vs what, which view, and the range facts.
    /// Collapsed it keeps a one-line summary so context is never lost.
    fn render_range_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = self.compact();
        let data = self.range_data().cloned();
        let unrelated = data.as_ref().is_some_and(|d| d.cmp.unrelated());
        let open = self.range_open;
        let mode = if unrelated { RangeMode::Compare } else { self.mode };
        let summary = match (&self.base, &self.compare) {
            (Some(b), Some(c)) => format!("{} … {} · {}", self.display_ref(b), self.display_ref(c), mode.short()),
            _ => "Pick base and compare".into(),
        };
        let pills = || -> Vec<AnyElement> {
            match (&self.range, &data) {
                (RangeState::Loading, _) => vec![widgets::spinner().into_any_element()],
                (_, Some(data)) => vec![
                    stat_pill(format!("↑ {}", data.cmp.ahead.len()), theme::add_fg(), "Commits in compare, not in base").into_any_element(),
                    stat_pill(format!("↓ {}", data.cmp.behind.len()), theme::mod_fg(), "Commits in base, not in compare").into_any_element(),
                ],
                _ => Vec::new(),
            }
        };
        let nerd = self.nerd();

        let header = div()
            .id("comparison-header")
            .h(px(34.))
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(6.))
            .pr(px(12.))
            .cursor_pointer()
            .hover(|s| s.bg(theme::ash()))
            .on_click(cx.listener(|this, _, _, cx| {
                this.range_open = !this.range_open;
                this.persisted.range_collapsed = !this.range_open;
                this.persisted.save();
                cx.notify();
            }))
            .child(widgets::chevron(open, nerd))
            .child(div().flex_none().text_size(theme::TEXT_CONTROL).font_weight(gpui::FontWeight::MEDIUM).text_color(theme::body()).child("Comparison"))
            .when(!open, |d| {
                d.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_size(theme::TEXT_CONTROL)
                        .text_color(theme::mute())
                        .child(summary),
                )
                .when(!compact, |d| d.children(pills()))
            })
            .when(open, |d| d.child(div().flex_1()));

        let body = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .px(px(12.))
            .pb(px(12.))
            // 1. What vs what
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(self.range_field(Which::Base, cx))
                            .child(self.range_field(Which::Compare, cx)),
                    )
                    .child(
                        div()
                            .id("swap")
                            .w(px(28.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(theme::RADIUS)
                            .border_1()
                            .border_color(theme::line_hi())
                            .text_size(theme::TEXT_CODE)
                            .text_color(theme::body())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::ash()).text_color(theme::bone()).border_color(theme::mute()))
                            .tooltip(|_, cx| cx.new(|_| widgets::Tip("Swap base and compare  ⌘⇧S")).into())
                            .on_click(cx.listener(|this, _, _, cx| this.swap(cx)))
                            .child("⇅"),
                    ),
            )
            .child(div().h(px(1.)).bg(theme::line()))
            // 2. Which view
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(micro("View"))
                            .child(div().flex_1())
                            .child(
                                div()
                                    .id("mode-help")
                                    .w(px(22.))
                                    .h(px(22.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(theme::RADIUS)
                                    .text_size(theme::TEXT_LIST)
                                    .text_color(if self.info_open { theme::frost() } else { theme::mute() })
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::ash()).text_color(theme::bone()))
                                    .tooltip(|_, cx| cx.new(|_| widgets::Tip("How do the two views differ?")).into())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.info_open = !this.info_open;
                                        cx.notify();
                                    }))
                                    .child(if nerd { "\u{ea74}" } else { "?" }),
                            ),
                    )
                    .child(self.view_switch(unrelated, cx))
                    .when_some(self.mode_sentence(), |d, (text, warn)| {
                        d.child(div().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(text)).when_some(warn, |d, w| {
                            d.child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .px(px(8.))
                                    .py(px(6.))
                                    .rounded(theme::RADIUS)
                                    .bg(theme::mod_fg().opacity(0.08))
                                    .border_l_2()
                                    .border_color(theme::mod_fg())
                                    .text_size(theme::TEXT_CONTROL)
                                    .text_color(theme::mod_fg())
                                    .child(div().flex_none().child("⚠"))
                                    .child(div().flex_1().min_w_0().child(w)),
                            )
                        })
                    }),
            )
            // 3. Range facts
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .when_some(data.as_ref(), |d, data| {
                        let mb = match data.cmp.merge_base {
                            Some(o) => format!("Merge base {}", short_sha(o)),
                            None => "No common ancestor".into(),
                        };
                        d.child(
                            div()
                                .text_size(theme::TEXT_CONTROL)
                                .text_color(if unrelated { theme::mod_fg() } else { theme::mute() })
                                .child(mb),
                        )
                    })
                    .child(div().flex_1())
                    .children(pills()),
            )
            .when_some(
                match &self.range {
                    RangeState::Error(e) => Some(e.clone()),
                    _ => None,
                },
                |d, e| d.child(div().text_size(theme::TEXT_CONTROL).text_color(theme::del_fg()).child(e)),
            );

        div()
            .flex_none()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(theme::line())
            .child(header)
            .when(open, |d| d.child(body))
    }

    /// Full-width segmented control: PR Merge | Compare.
    fn view_switch(&self, unrelated: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = self.compact();
        let item = |mode: RangeMode, tip: &'static str, cx: &mut Context<Kerf>| {
            let active = if unrelated { mode == RangeMode::Compare } else { self.mode == mode };
            let disabled = unrelated && mode == RangeMode::PrMerge;
            div()
                // Unique id: "Compare" alone would collide with the Compare branch field.
                .id(match mode {
                    RangeMode::PrMerge => "view-pr-merge",
                    RangeMode::Compare => "view-compare",
                })
                .flex_1()
                .h(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme::RADIUS)
                .text_size(theme::TEXT_CONTROL)
                .when(active, |d| d.bg(theme::slate()).text_color(theme::bone()).font_weight(gpui::FontWeight::MEDIUM))
                .when(!active && !disabled, |d| {
                    d.text_color(theme::mute()).cursor_pointer().hover(|s| s.text_color(theme::bone()))
                })
                .when(disabled, |d| d.text_color(theme::faint()))
                .tooltip(move |_, cx| cx.new(|_| widgets::Tip(tip)).into())
                .when(!disabled, |d| {
                    d.on_click(cx.listener(move |this, _, _, cx| {
                        if this.mode != mode {
                            this.toggle_mode(cx)
                        }
                    }))
                })
                .child(if compact { mode.short() } else { mode.label() })
        };
        div()
            .flex()
            .gap(px(2.))
            .p(px(2.))
            .rounded(theme::RADIUS)
            .bg(theme::crypt())
            .border_1()
            .border_color(theme::line_hi())
            .child(item(RangeMode::PrMerge, "Only what compare adds — like a GitHub PR  ⌘⇧M", cx))
            .child(item(RangeMode::Compare, "Full difference between the two tips  ⌘⇧M", cx))
    }

    /// Plain-language description of the current mode, plus a warning when it can mislead.
    fn mode_sentence(&self) -> Option<(String, Option<String>)> {
        let (b, c) = (self.base.as_deref()?, self.compare.as_deref()?);
        let (b, c) = (self.display_ref(b), self.display_ref(c));
        let data = self.range_data();
        if data.is_some_and(|d| d.cmp.unrelated()) {
            return Some((
                format!("{b} and {c} share no history — showing the full difference."),
                None,
            ));
        }
        Some(match self.mode {
            RangeMode::PrMerge => (format!("Changes on {c} since it branched from {b}."), None),
            RangeMode::Compare => {
                let behind = data.map(|d| d.cmp.behind.len()).unwrap_or(0);
                (
                    format!("Everything that differs between {b} and {c} right now."),
                    (behind > 0).then(|| {
                        format!("{behind} commit{} only on {b} will show as removals.", if behind == 1 { "" } else { "s" })
                    }),
                )
            }
        })
    }

    /// Tab strip — tabs only. Tools for the active tab live in `render_list_toolbar`.
    fn render_tabs(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (files, commits) = self
            .range_data()
            .map(|d| (d.changes.len(), d.cmp.ahead.len() + d.cmp.behind.len()))
            .unwrap_or((0, 0));
        let tab = |this: &Kerf, t: Tab, label: &'static str, n: usize, cx: &mut Context<Kerf>| {
            let active = this.tab == t;
            div()
                .id(SharedString::from(format!("tab-{label}")))
                .flex_1()
                .h_full()
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .cursor_pointer()
                .when(!active, |d| d.hover(|s| s.bg(theme::ash())))
                .on_click(cx.listener(move |this, _, _, cx| this.set_tab(t, cx)))
                .child(
                    div()
                        .text_size(theme::TEXT_LIST)
                        .font_weight(if active { gpui::FontWeight::MEDIUM } else { gpui::FontWeight::NORMAL })
                        .text_color(if active { theme::bone() } else { theme::mute() })
                        .child(label),
                )
                .child(
                    div()
                        .px(px(6.))
                        .rounded(px(8.))
                        .bg(if active { theme::slate() } else { theme::crypt() })
                        .text_size(theme::TEXT_CONTROL)
                        .text_color(if active { theme::body() } else { theme::mute() })
                        .child(widgets::thousands(n as u64)),
                )
                .when(active, |d| d.child(div().absolute().bottom_0().left(px(12.)).right(px(12.)).h(px(2.)).bg(theme::frost())))
        };
        div()
            .flex_none()
            .h(px(36.))
            .flex()
            .border_b_1()
            .border_color(theme::line())
            .child(tab(self, Tab::Files, "Files", files, cx))
            .child(tab(self, Tab::Commits, "Commits", commits, cx))
    }

    /// Files tab toolbar: search field + tree/list switch.
    fn render_list_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let filtering = self.input == Input::Filter;
        let nerd = self.nerd();
        let has_text = !self.filter.is_empty();
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(12.))
            .py(px(8.))
            .child(
                div()
                    .id("filter")
                    .flex_1()
                    .min_w_0()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(8.))
                    .bg(theme::crypt())
                    .border_1()
                    .border_color(if filtering { theme::frost() } else { theme::line_hi() })
                    .rounded(theme::RADIUS)
                    .cursor_text()
                    .text_size(theme::TEXT_LIST)
                    .hover(|s| s.border_color(theme::mute()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.input = Input::Filter;
                        cx.notify();
                    }))
                    .child(div().flex_none().text_color(theme::mute()).child(if nerd { "\u{ea6d}" } else { "⌕" }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .map(|d| {
                                if !has_text && !filtering {
                                    d.text_color(theme::mute()).child("Search files")
                                } else {
                                    d.text_color(theme::bone())
                                        .child(self.filter.clone())
                                        .when(filtering, |d| d.child(div().w(px(1.)).h(px(16.)).bg(theme::frost())))
                                }
                            }),
                    )
                    .when(has_text, |d| {
                        d.child(
                            div()
                                .id("filter-clear")
                                .flex_none()
                                .w(px(18.))
                                .h(px(18.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(theme::RADIUS)
                                .text_color(theme::mute())
                                .hover(|s| s.bg(theme::slate()).text_color(theme::bone()))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.filter.clear();
                                    this.input = Input::None;
                                    this.selected = None;
                                    this.rebuild_rows();
                                    cx.notify();
                                }))
                                .child(if nerd { "\u{ea76}" } else { "✕" }),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .p(px(2.))
                    .gap(px(2.))
                    .rounded(theme::RADIUS)
                    .bg(theme::crypt())
                    .border_1()
                    .border_color(theme::line_hi())
                    .child(
                        seg("tree", "Tree", self.tree, "Folders  t").on_click(cx.listener(|this, _, w, cx| {
                            if !this.tree {
                                w.dispatch_action(Box::new(super::ToggleTree), cx);
                            }
                        })),
                    )
                    .child(
                        seg("flat", "List", !self.tree, "Flat list of paths  t").on_click(cx.listener(|this, _, w, cx| {
                            if this.tree {
                                w.dispatch_action(Box::new(super::ToggleTree), cx);
                            }
                        })),
                    ),
            )
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
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_row()
            .child(
                uniform_list(
                    "list",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range.map(|ix| this.render_row(ix, cx)).collect::<Vec<_>>()
                    }),
                )
                .track_scroll(self.list_scroll.clone())
                .flex_1()
                .min_w_0()
                .h_full(),
            )
            .child(self.render_scrollbar(super::scrollbar::Bar::List, cx))
            .into_any_element()
    }

    fn render_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let compact = self.compact();
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
                    .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, _, cx| {
                        // Double-click keeps the file in its own (pinned) tab.
                        if ev.click_count() >= 2 {
                            this.pin_next = true;
                            if this.selected == Some(ix) {
                                this.pin_active(cx);
                                return;
                            }
                        }
                        this.click_row(ix, cx);
                        this.pin_next = false;
                    }))
            });
        let indent = |depth: usize| px(6. + depth as f32 * 14.);
        match row {
            ListRow::Summary => {
                let d = self.range_data();
                let (a, x) = d.map(|d| (d.additions, d.deletions)).unwrap_or((0, 0));
                base.pl(px(12.))
                    .text_size(theme::TEXT_CONTROL)
                    .text_color(theme::mute())
                    .child(format!(
                        "{} files changed",
                        d.map(|d| widgets::thousands(d.changes.len() as u64)).unwrap_or_default()
                    ))
                    .child(div().flex_1())
                    .child(counts(Some(a), Some(x)))
                    .when(!compact, |d| d.child(ratio_bar(a, x)))
                    .into_any_element()
            }
            ListRow::Notice(n) => base
                .pl(px(14.))
                .text_size(theme::TEXT_CONTROL)
                .text_color(theme::mute())
                .child(n.clone())
                .into_any_element(),
            ListRow::Dir { name, depth, collapsed, files, .. } => base
                .pl(indent(*depth))
                .text_color(if selected { theme::bone() } else { theme::body() })
                .child(widgets::chevron(!*collapsed, self.nerd()))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(format!("{name}/")),
                )
                .child(div().flex_none().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(files.to_string()))
                .into_any_element(),
            ListRow::File { change, depth } => {
                let Some(data) = self.range_data() else { return base.into_any_element() };
                let c = &data.changes[*change];
                let in_tree = self.tree && self.filter.is_empty();
                let (dir, file) = match c.path.rsplit_once('/') {
                    Some((d, f)) => (format!("{d}/"), f.to_string()),
                    None => (String::new(), c.path.clone()),
                };
                let viewed = self.viewed.contains(&c.path);
                let renamed = match (&c.old_path, c.similarity) {
                    (Some(old), Some(sim)) if !in_tree => Some(format!("← {old}  {sim}%")),
                    (Some(_), Some(sim)) => Some(format!("{sim}%")),
                    _ => None,
                };
                let name_color = if selected {
                    theme::bone()
                } else if viewed {
                    theme::mute()
                } else {
                    theme::bone()
                };
                // Flat list: dim folder part, bright file name. Tree: file name only.
                let label = div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(if in_tree || dir.is_empty() {
                        StyledText::new(SharedString::from(file.clone()))
                            .with_highlights([(0..file.len(), HighlightStyle { color: Some(name_color), ..Default::default() })])
                    } else {
                        let text = format!("{dir}{file}");
                        StyledText::new(SharedString::from(text.clone())).with_highlights([
                            (0..dir.len(), HighlightStyle { color: Some(theme::mute()), ..Default::default() }),
                            (dir.len()..text.len(), HighlightStyle { color: Some(name_color), ..Default::default() }),
                        ])
                    });
                let path = c.path.clone();
                base.pl(indent(*depth))
                    .tooltip(move |_, cx| cx.new(|_| PathTip(path.clone())).into())
                    .child(status_glyph(c.status))
                    .child(label)
                    .when_some(renamed, |d, r| {
                        d.child(div().flex_none().max_w(px(140.)).overflow_hidden().text_ellipsis().whitespace_nowrap().text_size(theme::TEXT_CONTROL).text_color(theme::frost()).child(r))
                    })
                    .when(c.binary && !compact, |d| d.child(div().text_size(theme::TEXT_MICRO).text_color(theme::mute()).child("BIN")))
                    .when(c.is_generated() && !compact, |d| d.child(div().text_size(theme::TEXT_MICRO).text_color(theme::mute()).child("GEN")))
                    .child(counts(c.additions.map(u64::from), c.deletions.map(u64::from)))
                    .when(viewed, |d| d.child(div().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child("✓")))
                    .into_any_element()
            }
            ListRow::Group { group, count, open } => base
                .pl(px(10.))
                .text_size(theme::TEXT_MICRO)
                .text_color(theme::mute())
                .child(widgets::chevron(*open, self.nerd()))
                .child(match group {
                    Group::Ahead => "Ahead · In Compare, Not Base",
                    Group::Behind => "Behind · In Base, Not Compare",
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
                let sha = c.oid.to_string();
                let is_base = self.base.as_deref().is_some_and(|b| b.len() >= 7 && sha.starts_with(b));
                let is_compare = self.compare.as_deref().is_some_and(|b| b.len() >= 7 && sha.starts_with(b));
                base.pl(px(10.))
                    .child(widgets::chevron(open, self.nerd()))
                    .child(div().flex_none().text_size(theme::TEXT_CONTROL).text_color(theme::syn_type()).child(c.short()))
                    .when(c.is_merge() && !compact, |d| d.child(div().flex_none().text_size(theme::TEXT_MICRO).text_color(theme::frost()).child("MERGE")))
                    .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().child(c.summary.clone()))
                    .when(is_base, |d| d.child(tag_badge("Base")))
                    .when(is_compare, |d| d.child(tag_badge("Compare")))
                    .when(selected, |d| {
                        d.child(
                            seg(format!("set-base-{ix}"), "Base", false, "Use this commit as base  b").on_click(cx.listener(|_this, _, w, cx| {
                                cx.stop_propagation();
                                w.dispatch_action(Box::new(super::SetBase), cx);
                            })),
                        )
                        .child(
                            seg(format!("set-compare-{ix}"), "Compare", false, "Use this commit as compare  c").on_click(cx.listener(|_this, _, w, cx| {
                                cx.stop_propagation();
                                w.dispatch_action(Box::new(super::SetCompare), cx);
                            })),
                        )
                    })
                    .when(!selected && !compact, |d| d.child(div().flex_none().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(age(c.time))))
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

fn stat_pill(text: String, color: gpui::Hsla, tip: &'static str) -> impl IntoElement {
    div()
        .id(SharedString::from(tip))
        .h(theme::CONTROL_H)
        .px(px(8.))
        .flex()
        .items_center()
        .rounded(theme::RADIUS)
        .bg(theme::crypt())
        .text_size(theme::TEXT_CONTROL)
        .text_color(color)
        .tooltip(move |_, cx| cx.new(|_| widgets::Tip(tip)).into())
        .child(text)
}

fn tag_badge(text: &'static str) -> impl IntoElement {
    div()
        .flex_none()
        .px(px(4.))
        .rounded(theme::RADIUS)
        .border_1()
        .border_color(theme::frost())
        .text_size(theme::TEXT_MICRO)
        .text_color(theme::frost())
        .child(text)
}

/// Five-block add/delete ratio, like GitHub's diffstat.
fn ratio_bar(add: u64, del: u64) -> impl IntoElement {
    let total = add + del;
    let green = if total == 0 { 0 } else { ((add as f64 / total as f64) * 5.).round() as usize };
    let red = if total == 0 { 0 } else { 5 - green };
    div().flex().flex_none().gap(px(2.)).children((0..5).map(move |i| {
        let color = if i < green {
            theme::add_fg()
        } else if i < green + red {
            theme::del_fg()
        } else {
            theme::line_hi()
        };
        div().w(px(7.)).h(px(7.)).rounded(px(1.)).bg(color)
    }))
}
