//! Fuzzy ref picker popover (Base / Compare): branches, remotes, tags, recent commits,
//! or any revision typed by hand.

use super::app::{commit_label, Input, Kerf, PickItem, RefLook, Which};
use super::widgets::{age, micro, ref_icon, ref_icon_color};
use crate::git::{short_sha, RefKind};
use crate::theme;
use gpui::{
    div, prelude::*, px, AnyElement, Context, Div, FontWeight, HighlightStyle, SharedString, Stateful, StyledText,
    Window,
};

const MAX_SHOWN: usize = 200;

#[derive(PartialEq, Clone, Copy)]
enum Section {
    Kind(RefKind),
    Commits,
    Raw,
}

fn highlighted(label: &str, positions: &[usize]) -> StyledText {
    let hl: Vec<_> = positions
        .iter()
        .filter_map(|&ci| {
            let (b, ch) = label.char_indices().nth(ci)?;
            Some((
                b..b + ch.len_utf8(),
                HighlightStyle {
                    color: Some(theme::frost()),
                    font_weight: Some(FontWeight::BOLD),
                    ..Default::default()
                },
            ))
        })
        .collect();
    StyledText::new(SharedString::from(label.to_string())).with_highlights(hl)
}

impl Kerf {
    fn pick_row(&self, n: usize, selected: bool, item: PickItem, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id(("pick", n))
            .h(theme::ROW_LIST)
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(10.))
            .text_size(theme::TEXT_LIST)
            .border_l_2()
            .border_color(if selected { theme::frost() } else { gpui::transparent_black() })
            .when(selected, |d| d.bg(theme::slate()))
            .when(!selected, |d| d.hover(|s| s.bg(theme::ash())))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| this.pick(item.clone(), cx)))
    }

    pub fn render_picker(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.picker.as_ref()?;
        let matches = self.picker_matches();
        let selected = p.selected.min(matches.len().saturating_sub(1));
        let title = match p.which {
            Which::Base => "Base — Compare Against",
            Which::Compare => "Compare — Changes To See",
        };
        let current = match p.which {
            Which::Base => self.base.clone(),
            Which::Compare => self.compare.clone(),
        };
        let nerd = self.nerd();
        let top = f32::from(theme::TITLEBAR_H) + 130. + if p.which == Which::Compare { 56. } else { 0. };
        let width = (self.sidebar_w + 220.).min(680.);
        let grouped = p.query.is_empty();

        let mut rows: Vec<AnyElement> = Vec::new();
        let mut last: Option<Section> = None;
        for (n, (item, positions)) in matches.iter().take(MAX_SHOWN).enumerate() {
            let section = match item {
                PickItem::Ref(i) => Section::Kind(self.refs[*i].kind),
                PickItem::Commit(_) => Section::Commits,
                PickItem::Raw(_) => Section::Raw,
            };
            if (grouped || section == Section::Raw) && last != Some(section) {
                last = Some(section);
                rows.push(
                    micro(match section {
                        Section::Kind(RefKind::Local) => "Local Branches",
                        Section::Kind(RefKind::Remote) => "Remote Branches",
                        Section::Kind(RefKind::Tag) => "Tags",
                        Section::Commits => "Recent Commits",
                        Section::Raw => "Revision",
                    })
                    .px(px(10.))
                    .pt(px(8.))
                    .pb(px(2.))
                    .into_any_element(),
                );
            }
            let is_sel = n == selected;
            let row = self.pick_row(n, is_sel, item.clone(), cx);
            let name_color = if is_sel { theme::bone() } else { theme::body() };
            let el = match item {
                PickItem::Ref(i) => {
                    let r = &self.refs[*i];
                    let look = match r.kind {
                        RefKind::Local => RefLook::Branch,
                        RefKind::Remote => RefLook::Remote,
                        RefKind::Tag => RefLook::Tag,
                    };
                    let is_current = current.as_deref() == Some(r.name.as_str());
                    row.child(div().w(px(16.)).flex_none().text_color(ref_icon_color(look)).child(ref_icon(look, nerd)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_color(name_color)
                                .child(highlighted(&r.name, positions)),
                        )
                        .when(r.is_head, |d| {
                            d.child(div().text_size(theme::TEXT_MICRO).text_color(theme::add_fg()).child("HEAD"))
                        })
                        .when(is_current, |d| {
                            d.child(div().text_size(theme::TEXT_MICRO).text_color(theme::frost()).child("Current"))
                        })
                        .child(
                            div()
                                .flex_none()
                                .text_size(theme::TEXT_CONTROL)
                                .text_color(theme::mute())
                                .child(short_sha(r.target)),
                        )
                        .child(age_cell(r.time))
                }
                PickItem::Commit(i) => {
                    let c = &self.commits[*i];
                    let is_current = current.as_deref().is_some_and(|v| c.oid.to_string().starts_with(v));
                    let label = commit_label(c);
                    row.child(
                        div()
                            .w(px(16.))
                            .flex_none()
                            .text_color(ref_icon_color(RefLook::Commit))
                            .child(ref_icon(RefLook::Commit, nerd)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(name_color)
                            .child(highlighted(&label, positions)),
                    )
                    .when(is_current, |d| {
                        d.child(div().text_size(theme::TEXT_MICRO).text_color(theme::frost()).child("Current"))
                    })
                    .child(
                        div()
                            .flex_none()
                            .text_size(theme::TEXT_CONTROL)
                            .text_color(theme::mute())
                            .child(c.author.clone()),
                    )
                    .child(age_cell(c.time))
                }
                PickItem::Raw(q) => row
                    .child(
                        div()
                            .w(px(16.))
                            .flex_none()
                            .text_color(ref_icon_color(RefLook::Rev))
                            .child(ref_icon(RefLook::Rev, nerd)),
                    )
                    .child(div().flex_1().text_color(name_color).child(format!("Use “{q}” as revision"))),
            };
            rows.push(el.into_any_element());
        }
        if matches.is_empty() {
            rows.push(
                div()
                    .px(px(10.))
                    .py(px(8.))
                    .text_size(theme::TEXT_LIST)
                    .text_color(theme::mute())
                    .child("Nothing matches — try a branch, tag, SHA or HEAD~2")
                    .into_any_element(),
            );
        }
        let overflow = matches.len().saturating_sub(MAX_SHOWN);

        Some(
            div()
                .id("picker-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.picker = None;
                    this.input = Input::None;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("picker")
                        .absolute()
                        .top(px(top))
                        .left(px(12.))
                        .w(px(width))
                        .max_h(px(520.))
                        .flex()
                        .flex_col()
                        .bg(theme::crypt())
                        .border_1()
                        .border_color(theme::line_hi())
                        .rounded(theme::RADIUS)
                        .shadow_lg()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex_none()
                                .px(px(10.))
                                .pt(px(8.))
                                .pb(px(6.))
                                .border_b_1()
                                .border_color(theme::line())
                                .child(micro(title).mb(px(4.)))
                                .child(
                                    div()
                                        .h(theme::CONTROL_H)
                                        .flex()
                                        .items_center()
                                        .text_size(theme::TEXT_LIST)
                                        .child(div().text_color(theme::frost()).mr(px(6.)).child("›"))
                                        .when(p.query.is_empty(), |d| {
                                            d.child(
                                                div()
                                                    .text_color(theme::mute())
                                                    .child("branch, tag, commit message or SHA…"),
                                            )
                                        })
                                        .when(!p.query.is_empty(), |d| {
                                            d.child(div().text_color(theme::bone()).child(p.query.clone()))
                                        })
                                        .child(div().w(px(1.)).h(px(16.)).bg(theme::frost())),
                                ),
                        )
                        .child(div().id("picker-list").flex_1().min_h_0().overflow_y_scroll().py(px(4.)).children(rows))
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .gap(px(12.))
                                .px(px(10.))
                                .py(px(4.))
                                .border_t_1()
                                .border_color(theme::line())
                                .text_size(theme::TEXT_CONTROL)
                                .text_color(theme::mute())
                                .child("↑↓ move")
                                .child("↵ pick")
                                .child("esc close")
                                .when(overflow > 0, |d| {
                                    d.child(div().flex_1()).child(format!("+{overflow} more — keep typing"))
                                }),
                        ),
                )
                .into_any_element(),
        )
    }
}

fn age_cell(t: i64) -> Div {
    div()
        .w(px(40.))
        .flex_none()
        .flex()
        .justify_end()
        .text_size(theme::TEXT_CONTROL)
        .text_color(theme::mute())
        .child(age(t))
}
