//! Start page (Zed / VS Code style): shown when no repository is open and no tab is open.
//! The sidebar only exists in git mode, so this page has the whole window.

use super::app::Kerf;
use super::scratch::tilde;
use super::widgets::micro;
use crate::theme;
use gpui::{div, prelude::*, px, AnyElement, Context, Div, FontWeight, SharedString, Stateful};

const MAX_RECENTS: usize = 8;

fn icon(nerd: bool, glyph: &'static str) -> Div {
    div()
        .w(px(20.))
        .flex_none()
        .text_size(theme::TEXT_CODE)
        .text_color(theme::mute())
        .child(if nerd { glyph } else { "›" })
}

/// A start action: icon, label, one-line explanation.
fn action(id: &'static str, nerd: bool, glyph: &'static str, label: &'static str, sub: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_start()
        .gap(px(10.))
        .px(px(10.))
        .py(px(8.))
        .mx(px(-10.))
        .rounded(theme::RADIUS)
        .cursor_pointer()
        .hover(|s| s.bg(theme::ash()))
        .child(icon(nerd, glyph).mt(px(1.)))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(theme::TEXT_LIST).font_weight(FontWeight::MEDIUM).text_color(theme::bone()).child(label))
                .child(div().text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child(sub)),
        )
}

impl Kerf {
    pub fn render_start_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let nerd = self.nerd();
        let recents: Vec<_> = self.persisted.recents.iter().take(MAX_RECENTS).cloned().collect();
        let start = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(micro("Start").mb(px(6.)))
            .child(
                action("start-open", nerd, "\u{ea62}", "Open Repository…", "Compare branches, tags and commits")
                    .on_click(cx.listener(|this, _, _, cx| this.prompt_open(cx))),
            )
            .child(
                action("start-new", nerd, "\u{ea7f}", "New Diff", "Type or paste two texts, diff updates live")
                    .on_click(cx.listener(|this, _, _, cx| this.new_scratch(cx))),
            )
            .child(
                action("start-files", nerd, "\u{eae1}", "Compare Files…", "Diff any two files on disk")
                    .on_click(cx.listener(|this, _, _, cx| this.prompt_compare_files(cx))),
            )
            .child(micro("Learn").mt(px(20.)).mb(px(6.)))
            .child(
                action("learn-keys", nerd, "\u{ea65}", "Keyboard Shortcuts", "Everything Kerf can do from the keyboard")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.shortcuts_open = true;
                        cx.notify();
                    })),
            )
            .child(
                action("learn-views", nerd, "\u{ea74}", "PR Merge vs Compare View", "Two ways to compare branches")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.info_open = true;
                        cx.notify();
                    })),
            );

        let recent = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(micro("Recent").mb(px(6.)))
            .when(recents.is_empty(), |d| {
                d.child(div().py(px(8.)).text_size(theme::TEXT_CONTROL).text_color(theme::mute()).child("Repositories you open show up here."))
            })
            .children(recents.into_iter().enumerate().map(|(i, p)| {
                let exists = p.exists();
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let path: SharedString = tilde(&p.display().to_string()).into();
                let (open_p, remove_p) = (p.clone(), p.clone());
                div()
                    .id(("recent", i))
                    .group("recent")
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(10.))
                    .py(px(6.))
                    .mx(px(-10.))
                    .rounded(theme::RADIUS)
                    .when(exists, |d| {
                        d.cursor_pointer()
                            .hover(|s| s.bg(theme::ash()))
                            .on_click(cx.listener(move |this, _, _, cx| this.open_repo(open_p.clone(), cx)))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(theme::TEXT_LIST)
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if exists { theme::bone() } else { theme::mute() })
                                    .when(!exists, |d| d.line_through())
                                    .child(name),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_size(theme::TEXT_CONTROL)
                                    .text_color(theme::mute())
                                    .child(if exists { path } else { format!("{path} · missing").into() }),
                            ),
                    )
                    .child(
                        div()
                            .id(("recent-remove", i))
                            .flex_none()
                            .w(px(22.))
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(theme::RADIUS)
                            .text_color(theme::mute())
                            .when(exists, |d| d.invisible().group_hover("recent", |s| s.visible()))
                            .hover(|s| s.bg(theme::slate()).text_color(theme::bone()))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.persisted.recents.retain(|r| r != &remove_p);
                                this.persisted.save();
                                cx.notify();
                            }))
                            .child(if nerd { "\u{ea76}" } else { "✕" }),
                    )
            }));

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::void())
            .child(
                div()
                    .w(px(760.))
                    .max_w_full()
                    .px(px(32.))
                    .flex()
                    .flex_col()
                    .gap(px(32.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(div().text_size(px(34.)).font_weight(FontWeight::BOLD).text_color(theme::bone()).child("kerf"))
                            .child(
                                div()
                                    .text_size(theme::TEXT_LIST)
                                    .text_color(theme::mute())
                                    .child("The cut between two branches — or any two texts."),
                            ),
                    )
                    .when_some(self.repo_error.clone(), |d, e| {
                        d.child(
                            div()
                                .px(px(10.))
                                .py(px(8.))
                                .rounded(theme::RADIUS)
                                .border_l_2()
                                .border_color(theme::del_fg())
                                .bg(theme::del_bg())
                                .text_size(theme::TEXT_CONTROL)
                                .text_color(theme::del_fg())
                                .child(e),
                        )
                    })
                    .child(div().flex().gap(px(48.)).child(start).child(recent)),
            )
            .into_any_element()
    }

    /// Leaves git mode: back to the start page (plain-diff tabs stay open).
    pub fn close_repo(&mut self, cx: &mut Context<Self>) {
        self.repo_path = None;
        self.repo_name.clear();
        self.repo_error = None;
        self.refs = Default::default();
        self.commits = Default::default();
        self.base = None;
        self.compare = None;
        self.range = super::app::RangeState::Idle;
        self.rows.clear();
        self.selected = None;
        self.persisted.last_repo = None;
        self.persisted.save();
        // Keep plain-diff tabs; git tabs belong to the closed repo.
        let active_scratch = self.active_scratch().map(|s| s.id);
        self.tabs.retain(|t| t.target.scratch.is_some());
        self.active_tab = None;
        self.diff = super::app::DiffState::Empty;
        let idx = active_scratch
            .and_then(|id| self.tabs.iter().position(|t| t.target.scratch.as_ref().is_some_and(|s| s.id == id)))
            .or_else(|| self.tabs.len().checked_sub(1));
        if let Some(i) = idx {
            self.activate_tab(i, cx);
        }
        cx.notify();
    }
}
